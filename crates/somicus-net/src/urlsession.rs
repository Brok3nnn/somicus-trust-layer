//! The macOS slot of the [`Transport`] boundary, not built on `NSURLSession` yet (B6.02): `post_redacted` goes
//! through [`crate::service::send`] and `connect` opens no session connection. The module also holds the HTTP/1.1
//! answer parsing that [`crate::service`] uses.

use crate::Redacted;
use crate::error::NetError;
use crate::proxy::ProxyConfig;
use crate::service::{Method, ServiceRequest};
use crate::transport::{BoxFuture, SessionConnection, Transport};

/// Why `connect` opens no session connection: on macOS the session goes through `post_redacted` for now, and other
/// systems have no `NSURLSession`.
#[cfg(target_os = "macos")]
const NO_WEBSOCKET: &str =
    "URLSession WebSocket uses post_redacted guidance endpoint in local runtime";
#[cfg(not(target_os = "macos"))]
const NO_WEBSOCKET: &str = "URLSession is only available on macOS hosts";

/// The macOS transport slot: it posts through [`crate::service::send`] and returns an error from `connect` until the
/// `NSURLSession` transport lands (B6.02). It reports its [`ProxyConfig`] but does not apply it.
pub struct UrlSessionTransport {
    proxy_config: ProxyConfig,
}

impl UrlSessionTransport {
    /// Creates a new `UrlSessionTransport` with the specified proxy configuration.
    #[must_use]
    pub fn new(proxy_config: ProxyConfig) -> Self {
        Self { proxy_config }
    }
}

impl Transport for UrlSessionTransport {
    fn name(&self) -> &'static str {
        "URLSession"
    }

    fn proxy_config(&self) -> &ProxyConfig {
        &self.proxy_config
    }

    fn connect<'a>(
        &'a self,
        _endpoint: &'a str,
        _session_ticket: &'a Redacted<String>,
        _dpop_proof: &'a str,
    ) -> BoxFuture<'a, Result<Box<dyn SessionConnection>, NetError>> {
        Box::pin(async move { Err(NetError::Connection(NO_WEBSOCKET.to_owned())) })
    }

    /// Posts through [`crate::service::send`], so this path has the same rules: plain HTTP only to this machine, HTTPS
    /// through the platform's transport off the async runtime, and a bounded answer.
    fn post_redacted<'a>(
        &'a self,
        url: &'a str,
        body: Redacted<Vec<u8>>,
        dpop_proof: Option<&'a str>,
    ) -> BoxFuture<'a, Result<Vec<u8>, NetError>> {
        Box::pin(async move {
            // A bearer token goes in `Authorization`; anything else is a `DPoP` proof.
            let (authorization, dpop) = match dpop_proof {
                Some(value) if value.starts_with("Bearer ") => (Some(value), None),
                other => (None, other),
            };
            let response = crate::service::send(ServiceRequest {
                method: Method::Post,
                url,
                body: Some(body),
                authorization,
                dpop,
            })
            .await?;
            Ok(response.body)
        })
    }
}

fn decode_chunked(mut data: &[u8]) -> Vec<u8> {
    let mut decoded = Vec::new();
    while !data.is_empty() {
        let Some(newline_pos) = data.windows(2).position(|w| w == b"\r\n") else {
            break;
        };
        let line = String::from_utf8_lossy(&data[..newline_pos]);
        let hex_str = line.split(';').next().unwrap_or("").trim();
        let Ok(chunk_size) = usize::from_str_radix(hex_str, 16) else {
            break;
        };
        if chunk_size == 0 {
            break;
        }
        let chunk_start = newline_pos + 2;
        // A size from the network can be anything: past the end of the body is malformed, never an overflow.
        let Some(next_start) = chunk_start
            .checked_add(chunk_size)
            .filter(|end| *end <= data.len())
        else {
            break;
        };
        decoded.extend_from_slice(&data[chunk_start..next_start]);
        data = if data.len() >= next_start + 2 && &data[next_start..next_start + 2] == b"\r\n" {
            &data[next_start + 2..]
        } else {
            &data[next_start..]
        };
    }
    if decoded.is_empty() {
        data.to_vec()
    } else {
        decoded
    }
}

pub(crate) fn extract_response_body(raw_response: &[u8]) -> Vec<u8> {
    if let Some(sep_pos) = raw_response.windows(4).position(|w| w == b"\r\n\r\n") {
        let headers = String::from_utf8_lossy(&raw_response[..sep_pos]);
        let body = &raw_response[sep_pos + 4..];
        let is_chunked = headers.lines().any(|l| {
            l.to_ascii_lowercase().starts_with("transfer-encoding:")
                && l.to_ascii_lowercase().contains("chunked")
        });
        if is_chunked {
            decode_chunked(body)
        } else {
            body.to_vec()
        }
    } else {
        raw_response.to_vec()
    }
}

#[cfg(test)]
mod tests {
    use somicus_privacy::PrivacyShield;
    use somicus_uigraph::ClassifiedText;

    use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};

    use super::*;

    #[test]
    fn a_chunk_size_beyond_the_body_is_not_a_panic() {
        let raw = b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\nffffffffffffffff\r\nabc\r\n0\r\n\r\n";
        assert!(extract_response_body(raw).len() <= raw.len());
    }

    #[tokio::test]
    async fn loopback_posts_send_bearer_authorization() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            // Read the whole request, body included, before answering: closing a socket with unread data resets the
            // connection, and the client would read the reset instead of the answer.
            let mut request = Vec::new();
            let mut chunk = [0_u8; 1024];
            let complete = |request: &[u8]| {
                let text = String::from_utf8_lossy(request);
                text.split_once("\r\n\r\n").is_some_and(|(head, body)| {
                    let length = head
                        .lines()
                        .find_map(|line| line.strip_prefix("Content-Length: "))
                        .and_then(|value| value.trim().parse::<usize>().ok())
                        .unwrap_or(0);
                    body.len() >= length
                })
            };
            while !complete(&request) {
                let count = socket.read(&mut chunk).await.unwrap();
                assert!(
                    count > 0,
                    "the client closed before sending the whole request"
                );
                request.extend_from_slice(&chunk[..count]);
            }
            let headers = String::from_utf8_lossy(&request);
            assert!(headers.contains("Authorization: Bearer synthetic-token\r\n"));
            assert!(!headers.contains("DPoP: Bearer"));
            socket
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\n{}")
                .await
                .unwrap();
        });
        let transport = UrlSessionTransport::new(ProxyConfig::default());
        let body = PrivacyShield::new()
            .redact_text(&ClassifiedText::public_label("{}"))
            .map(String::into_bytes);
        let response = transport
            .post_redacted(
                &format!("http://127.0.0.1:{port}/v1/dev/turn"),
                body,
                Some("Bearer synthetic-token"),
            )
            .await
            .unwrap();
        assert_eq!(response, b"{}");
        server.await.unwrap();
    }
}
