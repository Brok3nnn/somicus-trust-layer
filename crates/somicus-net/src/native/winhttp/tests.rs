//! The `WinHTTP` calls against a local socket and a public server. They run only on Windows and have not run yet.

use std::io::Read as _;
use std::net::TcpListener;
use std::time::{Duration, Instant};

use windows::Win32::Networking::WinHttp::{
    ERROR_WINHTTP_AUTODETECTION_FAILED, ERROR_WINHTTP_BAD_AUTO_PROXY_SCRIPT,
    ERROR_WINHTTP_CANNOT_CONNECT, ERROR_WINHTTP_CLIENT_AUTH_CERT_NEEDED,
    ERROR_WINHTTP_CONNECTION_ERROR, ERROR_WINHTTP_INVALID_SERVER_RESPONSE,
    ERROR_WINHTTP_NAME_NOT_RESOLVED, ERROR_WINHTTP_REDIRECT_FAILED,
    ERROR_WINHTTP_SECURE_CERT_CN_INVALID, ERROR_WINHTTP_SECURE_CERT_DATE_INVALID,
    ERROR_WINHTTP_SECURE_CERT_REV_FAILED, ERROR_WINHTTP_SECURE_CERT_REVOKED,
    ERROR_WINHTTP_SECURE_CHANNEL_ERROR, ERROR_WINHTTP_SECURE_FAILURE,
    ERROR_WINHTTP_SECURE_INVALID_CA, ERROR_WINHTTP_SECURE_INVALID_CERT, ERROR_WINHTTP_TIMEOUT,
    ERROR_WINHTTP_UNABLE_TO_DOWNLOAD_SCRIPT,
};

use super::super::{Limits, Redirects};
use super::*;
use crate::NetError;

fn limits(budget: Duration) -> Limits {
    Limits {
        redirects: Redirects::Never,
        budget,
        bytes: 1024,
    }
}

#[test]
fn the_named_error_codes_are_the_sdk_codes() {
    use super::super as codes;
    for (named, sdk) in [
        (codes::TIMEOUT, ERROR_WINHTTP_TIMEOUT),
        (codes::NAME_NOT_RESOLVED, ERROR_WINHTTP_NAME_NOT_RESOLVED),
        (codes::CANNOT_CONNECT, ERROR_WINHTTP_CANNOT_CONNECT),
        (codes::CONNECTION_ERROR, ERROR_WINHTTP_CONNECTION_ERROR),
        (
            codes::SECURE_CERT_DATE_INVALID,
            ERROR_WINHTTP_SECURE_CERT_DATE_INVALID,
        ),
        (
            codes::SECURE_CERT_CN_INVALID,
            ERROR_WINHTTP_SECURE_CERT_CN_INVALID,
        ),
        (
            codes::CLIENT_AUTH_CERT_NEEDED,
            ERROR_WINHTTP_CLIENT_AUTH_CERT_NEEDED,
        ),
        (codes::SECURE_INVALID_CA, ERROR_WINHTTP_SECURE_INVALID_CA),
        (
            codes::SECURE_CERT_REV_FAILED,
            ERROR_WINHTTP_SECURE_CERT_REV_FAILED,
        ),
        (
            codes::INVALID_SERVER_RESPONSE,
            ERROR_WINHTTP_INVALID_SERVER_RESPONSE,
        ),
        (codes::REDIRECT_FAILED, ERROR_WINHTTP_REDIRECT_FAILED),
        (
            codes::SECURE_CHANNEL_ERROR,
            ERROR_WINHTTP_SECURE_CHANNEL_ERROR,
        ),
        (
            codes::BAD_AUTO_PROXY_SCRIPT,
            ERROR_WINHTTP_BAD_AUTO_PROXY_SCRIPT,
        ),
        (
            codes::UNABLE_TO_DOWNLOAD_SCRIPT,
            ERROR_WINHTTP_UNABLE_TO_DOWNLOAD_SCRIPT,
        ),
        (
            codes::SECURE_INVALID_CERT,
            ERROR_WINHTTP_SECURE_INVALID_CERT,
        ),
        (
            codes::SECURE_CERT_REVOKED,
            ERROR_WINHTTP_SECURE_CERT_REVOKED,
        ),
        (codes::SECURE_FAILURE, ERROR_WINHTTP_SECURE_FAILURE),
        (
            codes::AUTODETECTION_FAILED,
            ERROR_WINHTTP_AUTODETECTION_FAILED,
        ),
    ] {
        assert_eq!(named, sdk);
    }
}

#[test]
fn a_session_allows_tls_1_2_or_later_only() {
    assert!(open_session().is_ok());
}

#[test]
fn the_first_bytes_sent_are_a_tls_handshake_never_plain_http() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let server = std::thread::spawn(move || {
        let (mut socket, _) = listener.accept().unwrap();
        let mut first = [0_u8; 11];
        socket.read_exact(&mut first).unwrap();
        first
    });
    let url = format!("https://127.0.0.1:{port}/v1/me");
    let headers = [("Authorization", "DPoP synthetic-access-token".to_owned())];
    let prepared = Exchange::new("GET", &url, &headers, limits(Duration::from_secs(5))).unwrap();
    let failure = exchange(&prepared).unwrap_err();
    let first = server.join().unwrap();
    // A TLS record of type handshake, holding a ClientHello whose version is TLS 1.2 (later versions say so too).
    assert_eq!(first[0], 0x16, "the client did not start a TLS handshake");
    assert_eq!(
        first[5], 0x01,
        "the first handshake message is not a ClientHello"
    );
    assert_eq!(&first[9..11], &[3, 3]);
    let message = NetError::from(failure).to_string();
    assert!(!message.contains("synthetic"), "{message}");
    assert!(!message.contains("127.0.0.1"), "{message}");
}

#[test]
fn a_server_that_never_answers_is_given_up_within_the_budget() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let server = std::thread::spawn(move || {
        let (socket, _) = listener.accept().unwrap();
        std::thread::sleep(Duration::from_secs(6));
        drop(socket);
    });
    let url = format!("https://127.0.0.1:{port}/v1/me");
    let prepared = Exchange::new("GET", &url, &[], limits(Duration::from_secs(1))).unwrap();
    let started = Instant::now();
    let failure = exchange(&prepared).unwrap_err();
    assert!(
        started.elapsed() < Duration::from_secs(3),
        "the request outlasted its budget: {:?}",
        started.elapsed()
    );
    assert!(
        matches!(failure, Failure::TimedOut | Failure::Exchange(_)),
        "{failure:?}"
    );
    server.join().unwrap();
}

#[test]
#[ignore = "needs the internet"]
fn a_public_https_server_answers_with_its_status() {
    let prepared = Exchange::new(
        "GET",
        "https://example.com/",
        &[],
        limits(Duration::from_secs(25)),
    )
    .unwrap();
    let answer = exchange(&prepared).unwrap();
    assert!(
        matches!(
            answer,
            Answer::Complete { status: 200, .. } | Answer::TooLarge
        ),
        "{answer:?}"
    );
}
