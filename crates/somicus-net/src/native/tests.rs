//! The platform-neutral half of the `WinHTTP` transport, tested on every system.

use std::time::Duration;

use super::*;

fn target(host: &str, port: u16, path: &str) -> Target {
    Target {
        host: host.to_owned(),
        port,
        path: path.to_owned(),
    }
}

#[test]
fn an_https_url_gives_its_host_port_and_path_without_the_fragment() -> Result<(), NetError> {
    for (url, expected) in [
        (
            "https://api.example.test/v1/me?state=synthetic#fragment",
            target("api.example.test", 443, "/v1/me?state=synthetic"),
        ),
        (
            "https://api.example.test",
            target("api.example.test", 443, "/"),
        ),
        (
            "https://api.example.test?x=1",
            target("api.example.test", 443, "/?x=1"),
        ),
        (
            "https://API.Example.test:8443/v1/model/turn",
            target("API.Example.test", 8443, "/v1/model/turn"),
        ),
        (
            "https://127.0.0.1:3001/v1",
            target("127.0.0.1", 3001, "/v1"),
        ),
        ("https://[::1]:3001/v1", target("::1", 3001, "/v1")),
        ("https://[2001:db8::1]/", target("2001:db8::1", 443, "/")),
    ] {
        assert_eq!(target_of(url)?, expected, "{url}");
    }
    Ok(())
}

#[test]
fn only_a_valid_https_url_without_credentials_is_a_target() {
    for url in [
        "http://api.example.test/v1/me",
        "HTTPS://api.example.test/",
        "ftp://api.example.test/",
        "https://",
        "https:///v1/me",
        "https://:443/v1",
        "https://api.example.test:/v1",
        "https://api.example.test:0/v1",
        "https://api.example.test:+443/v1",
        "https://api.example.test:65536/v1",
        "https://api.example.test:44a/v1",
        "https://api_example.test/",
        "https://exa%6dple.test/",
        "https://[::1/",
        "https://[not-an-address]/",
        "https://[::1]x/",
        "https://api.example.test/a b",
        "https://api.example.test/v1\r\nX-Evil: 1",
        "https://api.example.test/caf\u{e9}",
        "https://api.example.test/\0",
    ] {
        assert!(target_of(url).is_err(), "{url:?}");
    }
}

#[test]
fn a_url_with_credentials_is_refused_without_repeating_them() {
    for url in [
        "https://synthetic-user:synthetic-password@api.example.test/v1/me",
        "https://synthetic-user@api.example.test/",
    ] {
        let error = target_of(url).err().map(|error| error.to_string());
        assert_eq!(
            error.as_deref(),
            Some("network connection error: a service URL may not carry credentials"),
            "{url}"
        );
    }
}

#[test]
fn headers_become_one_crlf_block_and_none_can_end_its_line() -> Result<(), NetError> {
    let block = header_block(&[
        ("Content-Type", "application/json".to_owned()),
        ("Authorization", "DPoP synthetic-access-token".to_owned()),
        ("DPoP", "synthetic.dpop.proof".to_owned()),
    ])?;
    assert_eq!(
        String::from_utf16_lossy(&block),
        "Content-Type: application/json\r\nAuthorization: DPoP synthetic-access-token\r\nDPoP: synthetic.dpop.proof"
    );
    assert_eq!(header_block(&[])?, Vec::<u16>::new());
    for (name, value) in [
        ("Authorization", "DPoP x\r\nX-Evil: 1"),
        ("Authorization", "DPoP x\nX-Evil: 1"),
        ("Authorization", ""),
        ("Bad Name", "value"),
        ("Bad:Name", "value"),
        ("", "value"),
    ] {
        assert!(
            header_block(&[(name, value.to_owned())]).is_err(),
            "{name:?}: {value:?}"
        );
    }
    Ok(())
}

#[test]
fn an_exchange_holds_the_checked_target_headers_and_body() -> Result<(), NetError> {
    let limits = Limits {
        redirects: Redirects::Never,
        budget: Duration::from_secs(25),
        bytes: 8,
    };
    let body = b"{\"a\":1}";
    let prepared = Exchange::new(
        "POST",
        "https://api.example.test/v1/turn",
        &[("DPoP", "synthetic.proof".to_owned())],
        limits,
    )?
    .with_body(body)?;
    assert_eq!(prepared.method, "POST");
    assert_eq!(prepared.target, target("api.example.test", 443, "/v1/turn"));
    assert_eq!(
        String::from_utf16_lossy(&prepared.headers),
        "DPoP: synthetic.proof"
    );
    assert_eq!(prepared.body, body);
    assert_eq!(prepared.body_length, 7);
    assert_eq!(prepared.limits, limits);
    assert!(Exchange::new("GET", "http://127.0.0.1:3001/v1/me", &[], limits).is_err());
    assert!(body_length(usize::MAX).is_err());
    Ok(())
}

#[test]
fn time_outs_are_never_zero_and_sending_never_outlasts_the_budget() {
    for left in [
        Duration::ZERO,
        Duration::from_nanos(1),
        Duration::from_millis(3),
        Duration::from_secs(2),
        Duration::from_secs(25),
        Duration::MAX,
    ] {
        for timeouts in [Timeouts::sending(left), Timeouts::receiving(left)] {
            for value in [
                timeouts.resolve,
                timeouts.connect,
                timeouts.send,
                timeouts.receive,
            ] {
                assert!(value >= 1, "{left:?} gave {timeouts:?}");
            }
        }
    }
    let sending = Timeouts::sending(Duration::from_secs(25));
    assert_eq!(
        [
            sending.resolve,
            sending.connect,
            sending.send,
            sending.receive
        ],
        [6250; 4]
    );
    let receiving = Timeouts::receiving(Duration::from_secs(25));
    assert_eq!(receiving.receive, 25_000);
    assert_eq!(receiving.resolve, 6250);
    assert_eq!(Timeouts::sending(Duration::MAX).receive, i32::MAX);
}

#[test]
fn a_spent_budget_fails_before_any_call() {
    assert_eq!(
        Budget::new(Duration::ZERO).for_sending(),
        Err(Failure::TimedOut)
    );
    assert_eq!(
        Budget::new(Duration::ZERO).for_receiving(),
        Err(Failure::TimedOut)
    );
    assert_eq!(
        Budget::new(Duration::MAX).for_sending(),
        Err(Failure::TimedOut),
        "a budget too large to represent must fail closed"
    );
    assert!(Budget::new(Duration::from_secs(25)).for_receiving().is_ok());
}

/// A reader that gives `total` bytes of `x` in pieces of at most `piece`, then the end.
fn pieces(total: usize, piece: usize) -> impl FnMut(&mut [u8]) -> Result<usize, Failure> {
    let mut left = total;
    move |chunk: &mut [u8]| {
        let count = left.min(piece).min(chunk.len());
        chunk[..count].fill(b'x');
        left -= count;
        Ok(count)
    }
}

#[test]
fn an_answer_over_the_limit_is_refused_not_truncated() -> Result<(), Failure> {
    assert_eq!(
        read_limited(pieces(16, 5), 16)?.map(|body| body.len()),
        Some(16)
    );
    assert_eq!(read_limited(pieces(17, 5), 16)?, None);
    assert_eq!(read_limited(pieces(0, 5), 16)?, Some(Vec::new()));
    assert_eq!(
        read_limited(pieces(40_000, 40_000), 64 * 1024)?.map(|body| body.len()),
        Some(40_000)
    );
    // An endless answer stops at the limit.
    assert_eq!(
        read_limited(|chunk: &mut [u8]| Ok(chunk.len()), 1024)?,
        None
    );
    // A reader that claims more than the buffer holds is held to the buffer.
    assert_eq!(
        read_limited(pieces(usize::MAX, usize::MAX), CHUNK_BYTES * 2)?,
        None
    );
    assert_eq!(
        read_limited(|_: &mut [u8]| Err(Failure::Exchange(TIMEOUT)), 16),
        Err(Failure::Exchange(TIMEOUT))
    );
    Ok(())
}

#[test]
fn a_streamed_file_is_bounded_and_can_resume_after_a_broken_read() {
    let mut written = Vec::new();
    {
        let mut sink = |chunk: &[u8]| {
            written.extend_from_slice(chunk);
            Ok(())
        };
        assert_eq!(
            read_streamed(pieces(16, 5), 16, &mut sink).unwrap(),
            StreamedBody::Finished(16)
        );
    }
    assert_eq!(written, vec![b'x'; 16]);

    written.clear();
    {
        let mut sink = |chunk: &[u8]| {
            written.extend_from_slice(chunk);
            Ok(())
        };
        assert_eq!(
            read_streamed(pieces(17, 5), 16, &mut sink).unwrap(),
            StreamedBody::TooLarge
        );
    }
    assert_eq!(written.len(), 15);

    let mut calls = 0;
    written.clear();
    {
        let mut sink = |chunk: &[u8]| {
            written.extend_from_slice(chunk);
            Ok(())
        };
        assert_eq!(
            read_streamed(
                |chunk: &mut [u8]| {
                    calls += 1;
                    if calls == 2 {
                        Err(Failure::Exchange(TIMEOUT))
                    } else {
                        chunk[..5].fill(b'x');
                        Ok(5)
                    }
                },
                16,
                &mut sink
            )
            .unwrap(),
            StreamedBody::BrokeOff(5)
        );
    }
    assert_eq!(written.len(), 5);
}

#[test]
fn only_a_three_digit_status_is_an_answer_and_407_is_the_proxy() {
    assert_eq!(status_of(200), Ok(200));
    assert_eq!(status_of(101), Ok(101));
    assert_eq!(status_of(401), Ok(401));
    assert_eq!(status_of(599), Ok(599));
    assert_eq!(status_of(407), Err(Failure::ProxyAuthentication));
    for code in [0, 99, 600, 70_000, u32::MAX] {
        assert_eq!(status_of(code), Err(Failure::NoStatus), "{code}");
    }
}

#[test]
fn an_hresult_gives_its_win32_code() {
    // HRESULT_FROM_WIN32(ERROR_WINHTTP_TIMEOUT) and E_FAIL.
    assert_eq!(win32_code(0x8007_2EE2_u32.cast_signed()), TIMEOUT);
    assert_eq!(win32_code(0x8000_4005_u32.cast_signed()), 0x8000_4005);
    assert_eq!(win32_code(1168), 1168);
}

#[test]
fn failures_say_what_happened_in_fixed_words_with_the_code() {
    let message = |failure: Failure| NetError::from(failure).to_string();
    assert_eq!(
        message(Failure::Exchange(TIMEOUT)),
        "network connection error: the HTTPS request timed out (WinHTTP error 12002)"
    );
    assert!(message(Failure::Exchange(SECURE_INVALID_CA)).contains("certificate was refused"));
    assert!(message(Failure::Exchange(CANNOT_CONNECT)).contains("could not be reached"));
    assert!(message(Failure::Exchange(AUTODETECTION_FAILED)).contains("proxy configuration"));
    assert!(message(Failure::Exchange(1)).contains("the HTTPS request failed (WinHTTP error 1)"));
    assert!(message(Failure::Setup(8)).contains("could not set up"));
    assert!(message(Failure::ProxyAuthentication).contains("proxy asks for sign-in"));
    assert_eq!(
        message(Failure::NoStatus),
        "network connection error: the service sent no HTTP status"
    );
    assert_eq!(
        message(Failure::TimedOut),
        "network connection error: the HTTPS request timed out"
    );
}

#[test]
fn wide_strings_end_with_one_nul() {
    assert_eq!(
        wide("GET"),
        [u16::from(b'G'), u16::from(b'E'), u16::from(b'T'), 0]
    );
    assert_eq!(wide(""), [0]);
}
