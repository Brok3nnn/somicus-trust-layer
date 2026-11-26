//! The rules of ADR-0049 every native transport keeps, checked against a loopback HTTP server: each backend's tests run
//! these checks through its real system calls. The app sends only `https://`; the checks reach the same code over plain
//! HTTP to 127.0.0.1, which a test binary may.

use std::io::{BufRead as _, BufReader, Read as _, Write as _};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use super::{Answer, Exchange, Failure, Limits, Redirects, StreamedBody};

/// A backend's request: the exchange and the scheme to send it with.
pub(super) type Run = fn(&Exchange<'_>, &str) -> Result<Answer, Failure>;

/// A backend's streamed request: the exchange, the scheme, the status it must have and where the body goes.
pub(super) type Stream = fn(
    &Exchange<'_>,
    &str,
    u16,
    &mut dyn FnMut(&[u8]) -> std::io::Result<()>,
) -> Result<StreamedBody, crate::NetError>;

/// What one connection answers, given the request it read.
type Responder = Box<dyn Fn(&str) -> Vec<u8> + Send + Sync>;

/// A loopback server that answers every connection with `respond`, keeping each request's head and body.
struct Server {
    port: u16,
    requests: Arc<Mutex<Vec<String>>>,
}

impl Server {
    fn start(respond: Responder) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("a loopback port");
        let port = listener.local_addr().expect("its address").port();
        let requests = Arc::new(Mutex::new(Vec::new()));
        let kept = Arc::clone(&requests);
        let respond = Arc::new(respond);
        thread::spawn(move || {
            for connection in listener.incoming().flatten() {
                let kept = Arc::clone(&kept);
                let respond = Arc::clone(&respond);
                thread::spawn(move || serve(connection, &kept, &*respond));
            }
        });
        Self { port, requests }
    }

    fn url(&self, path: &str) -> String {
        format!("https://127.0.0.1:{}{path}", self.port)
    }

    fn requests(&self) -> Vec<String> {
        self.requests.lock().expect("the requests").clone()
    }
}

fn serve(
    connection: TcpStream,
    kept: &Mutex<Vec<String>>,
    respond: &(dyn Fn(&str) -> Vec<u8> + Send + Sync),
) {
    let mut reader = BufReader::new(connection.try_clone().expect("the connection"));
    let mut head = String::new();
    let mut length = 0_usize;
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line).unwrap_or(0) == 0 {
            return;
        }
        if let Some(value) = line.to_ascii_lowercase().strip_prefix("content-length:") {
            length = value.trim().parse().unwrap_or(0);
        }
        head.push_str(&line);
        if line == "\r\n" {
            break;
        }
    }
    let mut body = vec![0_u8; length];
    let _ = reader.read_exact(&mut body);
    let request = format!("{head}{}", String::from_utf8_lossy(&body));
    kept.lock().expect("the requests").push(request.clone());
    let mut connection = connection;
    let _ = connection.write_all(&respond(&request));
    let _ = connection.flush();
}

fn answer(status: &str, headers: &str, body: &[u8]) -> Vec<u8> {
    let mut response = format!(
        "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n{headers}\r\n",
        body.len()
    )
    .into_bytes();
    response.extend_from_slice(body);
    response
}

fn limits(redirects: Redirects, bytes: usize) -> Limits {
    Limits {
        redirects,
        budget: Duration::from_secs(10),
        bytes,
    }
}

pub(super) fn answers_with_the_status_and_body_and_sends_the_request_as_given(
    run: Run,
    _stream: Stream,
) {
    let server = Server::start(Box::new(|_| answer("201 Created", "", b"done")));
    let exchange = Exchange::new(
        "POST",
        &server.url("/v1/model/turn?x=1"),
        &[
            ("Authorization", "DPoP token".to_owned()),
            ("Content-Type", "application/json".to_owned()),
        ],
        limits(Redirects::Never, 1024),
    )
    .unwrap()
    .with_body(br#"{"q":1}"#)
    .unwrap();
    assert_eq!(
        run(&exchange, "http").unwrap(),
        Answer::Complete {
            status: 201,
            body: b"done".to_vec()
        }
    );
    let request = server.requests().concat();
    assert!(
        request.starts_with("POST /v1/model/turn?x=1 HTTP/1.1\r\n"),
        "{request}"
    );
    let lower = request.to_ascii_lowercase();
    assert!(lower.contains("authorization: dpop token\r\n"));
    assert!(lower.contains("user-agent: somicus\r\n"));
    assert!(request.ends_with(r#"{"q":1}"#));
    assert!(!lower.contains("\r\ncookie:"));
}

pub(super) fn follows_no_redirect_unless_allowed_and_then_only_to_https(run: Run, _stream: Stream) {
    let server = Server::start(Box::new(|request| {
        if request.starts_with("GET /next") {
            answer("200 OK", "", b"followed")
        } else {
            answer("302 Found", "Location: /next\r\n", b"")
        }
    }));
    let never = Exchange::new(
        "GET",
        &server.url("/start"),
        &[],
        limits(Redirects::Never, 1024),
    )
    .unwrap();
    assert!(matches!(
        run(&never, "http").unwrap(),
        Answer::Complete { status: 302, .. }
    ));
    // The redirect goes to plain HTTP here, which an icon request refuses as well.
    let https_only = Exchange::new(
        "GET",
        &server.url("/start"),
        &[],
        limits(Redirects::HttpsOnly(2), 1024),
    )
    .unwrap();
    assert!(matches!(
        run(&https_only, "http").unwrap(),
        Answer::Complete { status: 302, .. }
    ));
    assert_eq!(server.requests().len(), 2);
}

pub(super) fn cancels_an_answer_over_its_limit(run: Run, _stream: Stream) {
    let server = Server::start(Box::new(|_| answer("200 OK", "", &vec![b'x'; 64 * 1024])));
    let exchange = Exchange::new(
        "GET",
        &server.url("/big"),
        &[],
        limits(Redirects::Never, 1024),
    )
    .unwrap();
    assert_eq!(run(&exchange, "http").unwrap(), Answer::TooLarge);
}

pub(super) fn ends_within_its_time_budget(run: Run, _stream: Stream) {
    let server = Server::start(Box::new(|_| {
        thread::sleep(Duration::from_secs(5));
        answer("200 OK", "", b"late")
    }));
    let exchange = Exchange::new(
        "GET",
        &server.url("/slow"),
        &[],
        Limits {
            redirects: Redirects::Never,
            budget: Duration::from_millis(500),
            bytes: 1024,
        },
    )
    .unwrap();
    let started = Instant::now();
    let outcome = run(&exchange, "http");
    assert!(
        matches!(outcome, Err(Failure::TimedOut | Failure::Url(-1001))),
        "{outcome:?}"
    );
    assert!(started.elapsed() < Duration::from_secs(3));
}

pub(super) fn leaves_a_sign_in_unanswered(run: Run, _stream: Stream) {
    let server = Server::start(Box::new(|request| {
        if request.starts_with("GET /proxy") {
            answer(
                "407 Proxy Authentication Required",
                "Proxy-Authenticate: Basic realm=\"x\"\r\n",
                b"",
            )
        } else {
            answer(
                "401 Unauthorized",
                "WWW-Authenticate: Basic realm=\"x\"\r\n",
                b"no",
            )
        }
    }));
    let server_sign_in = Exchange::new(
        "GET",
        &server.url("/secret"),
        &[],
        limits(Redirects::Never, 1024),
    )
    .unwrap();
    assert!(matches!(
        run(&server_sign_in, "http").unwrap(),
        Answer::Complete { status: 401, .. }
    ));
    let proxy = Exchange::new(
        "GET",
        &server.url("/proxy"),
        &[],
        limits(Redirects::Never, 1024),
    )
    .unwrap();
    assert!(matches!(
        run(&proxy, "http"),
        Err(Failure::ProxyAuthentication)
    ));
    assert!(
        !server
            .requests()
            .concat()
            .to_ascii_lowercase()
            .contains("authorization: basic")
    );
}

pub(super) fn keeps_no_cookie_between_requests(run: Run, _stream: Stream) {
    let server = Server::start(Box::new(|_| {
        answer("200 OK", "Set-Cookie: session=secret; Path=/\r\n", b"ok")
    }));
    for _ in 0..2 {
        let exchange =
            Exchange::new("GET", &server.url("/"), &[], limits(Redirects::Never, 1024)).unwrap();
        assert!(matches!(
            run(&exchange, "http").unwrap(),
            Answer::Complete { status: 200, .. }
        ));
    }
    assert!(
        !server
            .requests()
            .concat()
            .to_ascii_lowercase()
            .contains("\r\ncookie:")
    );
}

pub(super) fn streams_a_file_answer_and_reports_a_broken_or_wrong_one(_run: Run, stream: Stream) {
    let server = Server::start(Box::new(|request| {
        if request.starts_with("GET /whole") {
            answer("200 OK", "", &vec![b'a'; 10_000])
        } else if request.starts_with("GET /broken") {
            let mut partial =
                b"HTTP/1.1 200 OK\r\nContent-Length: 1000\r\nConnection: close\r\n\r\n".to_vec();
            partial.extend_from_slice(&[b'b'; 400]);
            partial
        } else {
            answer("200 OK", "", b"not a range")
        }
    }));
    let mut written = Vec::new();
    let whole = Exchange::new(
        "GET",
        &server.url("/whole"),
        &[],
        limits(Redirects::Never, 20_000),
    )
    .unwrap();
    let outcome = stream(&whole, "http", 200, &mut |chunk| {
        written.extend_from_slice(chunk);
        Ok(())
    })
    .unwrap();
    assert_eq!(outcome, StreamedBody::Finished(10_000));
    assert_eq!(written, vec![b'a'; 10_000]);

    let too_long = Exchange::new(
        "GET",
        &server.url("/whole"),
        &[],
        limits(Redirects::Never, 5_000),
    )
    .unwrap();
    assert_eq!(
        stream(&too_long, "http", 200, &mut |_| Ok(())).unwrap(),
        StreamedBody::TooLarge
    );

    let broken = Exchange::new(
        "GET",
        &server.url("/broken"),
        &[],
        limits(Redirects::Never, 20_000),
    )
    .unwrap();
    assert!(matches!(
        stream(&broken, "http", 200, &mut |_| Ok(())).unwrap(),
        StreamedBody::BrokeOff(bytes) if bytes <= 400
    ));

    let wrong = Exchange::new(
        "GET",
        &server.url("/rest"),
        &[],
        limits(Redirects::Never, 20_000),
    )
    .unwrap();
    assert_eq!(
        stream(&wrong, "http", 206, &mut |_| Ok(())).unwrap(),
        StreamedBody::BrokeOff(0)
    );
}

pub(super) fn passes_a_sink_failure_on(_run: Run, stream: Stream) {
    let server = Server::start(Box::new(|_| answer("200 OK", "", &[b'c'; 100])));
    let exchange = Exchange::new(
        "GET",
        &server.url("/file"),
        &[],
        limits(Redirects::Never, 1024),
    )
    .unwrap();
    let failure = stream(&exchange, "http", 200, &mut |_| {
        Err(std::io::Error::other("disk full"))
    });
    assert!(failure.is_err());
}
