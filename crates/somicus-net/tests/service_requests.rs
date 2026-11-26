//! Service requests over plain HTTP: method, RFC 9449 headers, body and any status come through.

use somicus_net::{Method, NetError, Redacted, ServiceRequest, send};
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
use tokio::net::TcpListener;

/// Accepts one request, reads all of it (head and `Content-Length` body), answers `answer` and returns the request.
async fn serve_once(
    answer: &'static [u8],
) -> Result<(String, tokio::task::JoinHandle<Result<String, NetError>>), NetError> {
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let port = listener.local_addr()?.port();
    let handle = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await?;
        let mut received = Vec::new();
        let mut buffer = [0_u8; 1024];
        loop {
            let read = socket.read(&mut buffer).await?;
            received.extend_from_slice(&buffer[..read]);
            let text = String::from_utf8_lossy(&received).into_owned();
            if let Some(end) = text.find("\r\n\r\n") {
                let length = text
                    .lines()
                    .find_map(|line| line.strip_prefix("Content-Length: "))
                    .and_then(|value| value.trim().parse::<usize>().ok())
                    .unwrap_or(0);
                if received.len() >= end + 4 + length || read == 0 {
                    break;
                }
            } else if read == 0 {
                break;
            }
        }
        socket.write_all(answer).await?;
        Ok(String::from_utf8_lossy(&received).into_owned())
    });
    Ok((format!("http://127.0.0.1:{port}"), handle))
}

#[tokio::test]
async fn sends_the_method_headers_and_body_and_reads_any_status() -> Result<(), NetError> {
    let (base, server) =
        serve_once(b"HTTP/1.1 401 Unauthorized\r\nContent-Length: 14\r\n\r\n{\"error\":\"x\"}\n")
            .await?;
    let url = format!("{base}/v1/me");
    let response = send(ServiceRequest {
        method: Method::Patch,
        url: &url,
        body: Some(Redacted::without_content(br#"{"a":1}"#.to_vec())),
        authorization: Some("DPoP token.value"),
        dpop: Some("proof.value.sig"),
    })
    .await?;
    assert_eq!(response.status, 401);
    assert_eq!(response.body, b"{\"error\":\"x\"}\n");
    let sent = server
        .await
        .map_err(|_| NetError::Connection("the test server stopped".to_owned()))??;
    assert!(sent.starts_with("PATCH /v1/me HTTP/1.1\r\n"));
    assert!(sent.contains("Authorization: DPoP token.value\r\n"));
    assert!(sent.contains("DPoP: proof.value.sig\r\n"));
    assert!(sent.contains("Content-Type: application/json\r\n"));
    assert!(sent.ends_with("\r\n\r\n{\"a\":1}"));
    Ok(())
}
