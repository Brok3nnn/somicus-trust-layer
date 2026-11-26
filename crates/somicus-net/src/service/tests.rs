use somicus_privacy::PrivacyShield;
use somicus_telemetry::testing::capture_async;
use somicus_uigraph::ClassifiedText;
use tokio::net::TcpListener;

use super::*;

/// Synthetic credentials: never real ones.
const ACCESS: &str = "DPoP synthetic-access-token";
const PROOF: &str = "synthetic.dpop.proof";

/// Brands `text` the way product code does, through the Privacy Shield.
fn shielded(text: &str) -> Redacted<Vec<u8>> {
    PrivacyShield::new()
        .redact_text(&ClassifiedText::public_label(text))
        .map(String::into_bytes)
}

/// Accepts one connection on `listener`, reads the request up to its `Content-Length` body and answers `answer`.
async fn answer_once(listener: TcpListener, answer: Vec<u8>) -> Result<Vec<u8>, NetError> {
    let (mut socket, _) = listener.accept().await?;
    let mut received = Vec::new();
    let mut buffer = [0_u8; 4096];
    loop {
        let read = socket.read(&mut buffer).await?;
        received.extend_from_slice(&buffer[..read]);
        let text = String::from_utf8_lossy(&received).into_owned();
        let complete = text.find("\r\n\r\n").is_some_and(|end| {
            let length = text
                .lines()
                .find_map(|line| {
                    line.to_ascii_lowercase()
                        .strip_prefix("content-length: ")
                        .map(ToOwned::to_owned)
                })
                .and_then(|value| value.trim().parse::<usize>().ok())
                .unwrap_or(0);
            received.len() >= end + 4 + length
        });
        if complete || read == 0 {
            break;
        }
    }
    socket.write_all(&answer).await?;
    Ok(received)
}

#[test]
fn debug_output_never_shows_credentials_or_bodies() {
    let request = ServiceRequest {
        method: Method::Post,
        url: "https://api.example.test/v1/sessions?state=synthetic-state",
        body: Some(shielded("{}")),
        authorization: Some(ACCESS),
        dpop: Some(PROOF),
    };
    assert_eq!(
        format!("{request:?}"),
        "ServiceRequest { method: Post, url: \"https://api.example.test/v1/sessions\", body: Some(Redacted(…)), \
         authorization: Some(<redacted>), dpop: Some(<redacted>) }"
    );
    let response = ServiceResponse {
        status: 200,
        body: br#"{"access_token":"synthetic-access-token"}"#.to_vec(),
    };
    assert_eq!(
        format!("{response:?}"),
        "ServiceResponse { status: 200, body: <41 bytes> }"
    );
}

#[test]
fn only_this_machine_is_a_plain_http_destination() {
    for (host, address) in [
        ("127.0.0.1:3001", "127.0.0.1:3001"),
        ("127.0.0.2", "127.0.0.2:80"),
        ("localhost:3002", "127.0.0.1:3002"),
        ("LOCALHOST", "127.0.0.1:80"),
        ("[::1]:3001", "[::1]:3001"),
        ("[::1]", "[::1]:80"),
    ] {
        assert_eq!(
            loopback_address(host)
                .map(|found| found.to_string())
                .as_deref(),
            Some(address),
            "{host}"
        );
    }
    for host in [
        "0.0.0.0:80",
        "10.0.0.1:3001",
        "api.example.test",
        "localhost.example.test:3001",
        "[::ffff:127.0.0.1]:80",
        "::1",
        "127.0.0.1:",
        "127.0.0.1:65536",
        "[::1]x",
    ] {
        assert_eq!(loopback_address(host), None, "{host}");
    }
}

#[tokio::test]
async fn plain_http_to_a_host_that_is_not_loopback_is_refused() -> Result<(), NetError> {
    // 0.0.0.0 reaches this machine but is not a loopback address, so it stands in for any other host.
    let listener = TcpListener::bind("0.0.0.0:0").await?;
    let port = listener.local_addr()?.port();
    let server = tokio::spawn(answer_once(
        listener,
        b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n".to_vec(),
    ));
    let url = format!("http://0.0.0.0:{port}/v1/me");
    let sent = send(ServiceRequest {
        method: Method::Get,
        url: &url,
        body: None,
        authorization: Some(ACCESS),
        dpop: Some(PROOF),
    })
    .await;
    server.abort();
    assert!(
        sent.is_err(),
        "a token went over plain HTTP to another host"
    );
    Ok(())
}

#[tokio::test]
async fn an_answer_over_the_limit_is_refused_not_truncated() -> Result<(), NetError> {
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let port = listener.local_addr()?.port();
    let mut answer = b"HTTP/1.1 200 OK\r\nConnection: close\r\n\r\n".to_vec();
    answer.resize(answer.len() + 8 * 1024 * 1024 + 1, b'x');
    let server = tokio::spawn(answer_once(listener, answer));
    let url = format!("http://127.0.0.1:{port}/v1/me");
    let sent = send(ServiceRequest {
        method: Method::Get,
        url: &url,
        body: None,
        authorization: None,
        dpop: None,
    })
    .await;
    server.abort();
    assert!(sent.is_err(), "an answer of more than 8 MiB was read");
    Ok(())
}

#[tokio::test]
async fn refuses_header_injection_and_other_schemes() {
    let injected = send(ServiceRequest {
        method: Method::Get,
        url: "http://127.0.0.1:1/v1/me",
        body: None,
        authorization: Some("DPoP x\r\nX-Evil: 1"),
        dpop: None,
    })
    .await;
    assert!(injected.is_err());
    let ftp = send(ServiceRequest {
        method: Method::Get,
        url: "ftp://x/",
        body: None,
        authorization: None,
        dpop: None,
    })
    .await;
    assert!(ftp.is_err());
    assert!(status_of(b"garbage").is_err());
    assert_eq!(
        status_of(b"HTTP/1.1 204 No Content\r\n\r\n").ok(),
        Some(204)
    );
}

#[tokio::test]
async fn a_request_span_carries_only_method_scheme_status_and_size() -> Result<(), NetError> {
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let port = listener.local_addr()?.port();
    let server = tokio::spawn(answer_once(
        listener,
        b"HTTP/1.1 201 Created\r\nContent-Length: 2\r\n\r\n{}".to_vec(),
    ));
    let url = format!("http://127.0.0.1:{port}/v1/people/anna?email=anna%40example.test");
    let (sent, records) = capture_async(send(ServiceRequest {
        method: Method::Post,
        url: &url,
        body: Some(shielded("{\"note\":\"Anna Kovacs\"}")),
        authorization: Some(ACCESS),
        dpop: Some(PROOF),
    }))
    .await;
    server.abort();
    assert_eq!(sent?.status, 201);
    let refused = capture_async(send(ServiceRequest {
        method: Method::Get,
        url: "http://192.0.2.1/v1/people/anna",
        body: None,
        authorization: Some(ACCESS),
        dpop: None,
    }))
    .await
    .1;

    let [ok, failed] = [records, refused].map(|mut records| {
        assert_eq!(records.len(), 1, "{records:?}");
        records.remove(0)
    });
    assert_eq!(ok.name, "net_request");
    assert_eq!(
        ok.field_names(),
        ["kind", "transport", "status", "bytes", "outcome"]
    );
    assert_eq!(ok.label("kind"), Some("post"));
    assert_eq!(ok.label("transport"), Some("http"));
    assert_eq!(ok.count("status"), Some(201));
    assert_eq!(ok.count("bytes"), Some(2));
    assert_eq!(
        failed.field_names(),
        ["kind", "transport", "outcome", "reason"]
    );
    assert_eq!(failed.label("reason"), Some("connection"));
    for record in [ok, failed] {
        assert_eq!(record.dropped, 0, "{record:?}");
        let line = record.to_string();
        for content in [
            "anna",
            "Anna",
            "127.0.0.1",
            "192.0.2.1",
            "/v1/",
            "synthetic",
        ] {
            assert!(!line.contains(content), "{line}");
        }
    }
    Ok(())
}
