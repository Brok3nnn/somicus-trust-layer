//! Requests to the Somicus services (ADR-0032): sign-in, sessions and the model service, with the status code and the
//! RFC 9449 headers. Plain HTTP goes only to a loopback address, the backend on this machine; HTTPS goes through
//! the system's own stack inside the process (`WinHTTP`, `NSURLSession`, libcurl; the `native` submodule). Every body is
//! [`Redacted`], the access token and proof travel only in headers, never on a command line, no answer is read past
//! [`MAX_RESPONSE_BYTES`], and `Debug` shows neither credentials nor bodies.

use std::fmt;
use std::fmt::Write as _;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};

use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
use tokio::net::TcpStream;

use crate::Redacted;
use crate::debug::{ByteCount, Hidden};
use crate::error::NetError;
use crate::urlsession::extract_response_body;

// The HTTPS transport: the system's own stack inside the process (ADR-0049).
mod native;
use native as https;

/// The most bytes this crate reads of what a service sends back. A longer answer is refused, never cut short.
pub const MAX_RESPONSE_BYTES: usize = 8 * 1024 * 1024;

/// The longest an HTTPS request to a service may take, in seconds.
const MAX_SECONDS: u64 = 25;

/// The HTTP methods the services use.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Method {
    /// Reads.
    Get,
    /// Creates or acts.
    Post,
    /// Changes part of a resource.
    Patch,
    /// Replaces a resource (an organisation's policy or single sign-on, ADR-0053).
    Put,
    /// Removes a resource (a member, an invitation, a single sign-on).
    Delete,
}

impl Method {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Get => "GET",
            Self::Post => "POST",
            Self::Patch => "PATCH",
            Self::Put => "PUT",
            Self::Delete => "DELETE",
        }
    }
}

/// The method's name, as the `kind` of a request's span.
impl somicus_telemetry::Label for Method {
    fn label(&self) -> &'static str {
        match self {
            Self::Get => "get",
            Self::Post => "post",
            Self::Patch => "patch",
            Self::Put => "put",
            Self::Delete => "delete",
        }
    }
}

/// The URL's scheme, as the `transport` of a request's span. Nothing else of the URL is recorded: its host, path and
/// query can name the person or carry a credential.
fn transport_of(url: &str) -> &'static str {
    if url.starts_with("https://") {
        "https"
    } else if url.starts_with("http://") {
        "http"
    } else {
        "other"
    }
}

/// A request to a Somicus service. `Debug` shows the URL without its query and hides the credentials.
///
/// The body must be a [`Redacted`] value: a raw body does not compile.
///
/// ```compile_fail
/// use somicus_net::{Method, ServiceRequest};
///
/// let request = ServiceRequest {
///     method: Method::Post,
///     url: "https://api.example.test/v1/turn",
///     body: Some(b"jane.doe@example.com".to_vec()),
///     authorization: None,
///     dpop: None,
/// };
/// ```
pub struct ServiceRequest<'a> {
    /// The method.
    pub method: Method,
    /// The full URL, `http://` or `https://`.
    pub url: &'a str,
    /// The JSON body, if any.
    pub body: Option<Redacted<Vec<u8>>>,
    /// The `Authorization` header's value, for example `DPoP <token>`.
    pub authorization: Option<&'a str>,
    /// The `DPoP` proof.
    pub dpop: Option<&'a str>,
}

impl fmt::Debug for ServiceRequest<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let address = self.url.split(['?', '#']).next().unwrap_or_default();
        formatter
            .debug_struct("ServiceRequest")
            .field("method", &self.method)
            .field("url", &address)
            .field("body", &self.body)
            .field("authorization", &self.authorization.map(|_| Hidden))
            .field("dpop", &self.dpop.map(|_| Hidden))
            .finish()
    }
}

/// A service's answer. `Debug` shows the size of the body, which can hold tokens, not the body.
#[derive(Clone, PartialEq, Eq)]
pub struct ServiceResponse {
    /// The HTTP status code.
    pub status: u16,
    /// The body.
    pub body: Vec<u8>,
}

impl fmt::Debug for ServiceResponse {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ServiceResponse")
            .field("status", &self.status)
            .field("body", &ByteCount(self.body.len()))
            .finish()
    }
}

fn too_large() -> NetError {
    NetError::Connection("the service's answer is larger than the limit".to_owned())
}

/// The address of `host` (`name[:port]` from an `http://` URL) when it is this machine: `localhost` or a loopback IP
/// literal. Any other host gets `None`, so a credential never travels over plain HTTP to another machine.
fn loopback_address(host: &str) -> Option<SocketAddr> {
    let (name, port) = if let Some(bracketed) = host.strip_prefix('[') {
        let (name, after) = bracketed.split_once(']')?;
        let port = if after.is_empty() {
            None
        } else {
            Some(after.strip_prefix(':')?)
        };
        (name, port)
    } else {
        match host.split_once(':') {
            Some((name, port)) => (name, Some(port)),
            None => (host, None),
        }
    };
    let port = match port {
        Some(port) => port.parse::<u16>().ok()?,
        None => 80,
    };
    let ip = if name.eq_ignore_ascii_case("localhost") {
        IpAddr::V4(Ipv4Addr::LOCALHOST)
    } else {
        name.parse::<IpAddr>().ok()?
    };
    ip.is_loopback().then_some(SocketAddr::new(ip, port))
}

/// Whether `value` may be a header value: visible ASCII and spaces only, so it cannot end the header early.
pub(crate) fn header_safe(value: &str) -> bool {
    !value.is_empty()
        && value
            .bytes()
            .all(|byte| byte == b' ' || byte.is_ascii_graphic())
}

fn headers_of(request: &ServiceRequest<'_>) -> Result<Vec<(&'static str, String)>, NetError> {
    let mut headers = Vec::new();
    if request.body.is_some() {
        headers.push(("Content-Type", "application/json".to_owned()));
    }
    for (name, value) in [
        ("Authorization", request.authorization),
        ("DPoP", request.dpop),
    ] {
        if let Some(value) = value {
            if !header_safe(value) {
                return Err(NetError::Connection(format!(
                    "the {name} header is not a valid value"
                )));
            }
            headers.push((name, value.to_owned()));
        }
    }
    Ok(headers)
}

fn status_of(raw: &[u8]) -> Result<u16, NetError> {
    let line_end = raw
        .windows(2)
        .position(|pair| pair == b"\r\n")
        .unwrap_or(raw.len());
    let line = String::from_utf8_lossy(&raw[..line_end]);
    line.split_whitespace()
        .nth(1)
        .and_then(|code| code.parse::<u16>().ok())
        .filter(|code| (100..600).contains(code))
        .ok_or_else(|| NetError::Connection("the service sent no HTTP status".to_owned()))
}

async fn send_http(request: ServiceRequest<'_>) -> Result<ServiceResponse, NetError> {
    let headers = headers_of(&request)?;
    let rest = request.url.strip_prefix("http://").unwrap_or_default();
    let (host, path) = rest
        .find('/')
        .map_or((rest, "/"), |slash| (&rest[..slash], &rest[slash..]));
    if host.is_empty() || !header_safe(host) || !header_safe(path) {
        return Err(NetError::Connection(
            "the service URL is not valid".to_owned(),
        ));
    }
    let address = loopback_address(host).ok_or_else(|| {
        NetError::Connection("plain HTTP goes only to this machine; use https".to_owned())
    })?;
    let body = request.body.map(Redacted::into_inner).unwrap_or_default();
    let mut head = format!(
        "{} {path} HTTP/1.1\r\nHost: {host}\r\n",
        request.method.as_str()
    );
    for (name, value) in &headers {
        let _ = write!(head, "{name}: {value}\r\n");
    }
    let _ = write!(
        head,
        "Content-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    let mut stream = TcpStream::connect(address)
        .await
        .map_err(|error| NetError::Connection(format!("could not reach {address}: {error}")))?;
    let mut message = head.into_bytes();
    message.extend_from_slice(&body);
    stream.write_all(&message).await?;
    let mut raw = Vec::new();
    let allowed = u64::try_from(MAX_RESPONSE_BYTES)
        .unwrap_or(u64::MAX)
        .saturating_add(1);
    (&mut stream).take(allowed).read_to_end(&mut raw).await?;
    if raw.len() > MAX_RESPONSE_BYTES {
        return Err(too_large());
    }
    Ok(ServiceResponse {
        status: status_of(&raw)?,
        body: extract_response_body(&raw),
    })
}

/// Sends a request to a Somicus service and returns its status and body, whatever the status.
///
/// Its `net_request` span records the method, the scheme, the status, the answer's size and the error's kind; never
/// the URL, a header or a body.
///
/// # Errors
/// Returns [`NetError`] when the service cannot be reached, the URL or a header value is not valid, a plain `http://`
/// URL names another machine, the answer is larger than [`MAX_RESPONSE_BYTES`], or it has no HTTP status.
pub async fn send(request: ServiceRequest<'_>) -> Result<ServiceResponse, NetError> {
    let span = somicus_telemetry::span!(
        INFO,
        "net_request",
        kind = request.method,
        transport = transport_of(request.url),
    );
    let result = span.instrument(send_unobserved(request)).await;
    if let Ok(response) = &result {
        somicus_telemetry::record!(span, status = response.status, bytes = response.body.len());
    }
    span.record_result(&result);
    result
}

async fn send_unobserved(request: ServiceRequest<'_>) -> Result<ServiceResponse, NetError> {
    if request.url.starts_with("http://") {
        send_http(request).await
    } else if request.url.starts_with("https://") {
        tokio::task::spawn_blocking({
            let url = request.url.to_owned();
            let authorization = request.authorization.map(ToOwned::to_owned);
            let dpop = request.dpop.map(ToOwned::to_owned);
            let method = request.method;
            let body = request.body;
            move || {
                https::send(ServiceRequest {
                    method,
                    url: &url,
                    body,
                    authorization: authorization.as_deref(),
                    dpop: dpop.as_deref(),
                })
            }
        })
        .await
        .map_err(|_| NetError::Connection("the HTTPS request was cancelled".to_owned()))?
    } else {
        Err(NetError::Connection(
            "the service URL is not http or https".to_owned(),
        ))
    }
}

#[cfg(test)]
mod tests;
