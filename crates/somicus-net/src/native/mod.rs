//! HTTPS inside the process through the operating system's own HTTP stack (ADR-0049): `WinHTTP` on Windows, `NSURLSession` on macOS and the distribution's libcurl on Linux. No process is started, the server's certificate is checked against the
//! system's trusted roots, and the system's proxy settings apply. Neither applies [`crate::ProxyConfig`] yet.
//!
//! The contract is the same on both: HTTPS only, TLS 1.2 or later, no credentials in the URL, no cookies, cache or
//! stored credentials, never the person's credentials (no automatic sign-in to a server or proxy), redirects only where
//! the caller allows them and only to HTTPS, an answer read no further than its limit, and a request that ends within
//! its time budget. Errors carry a fixed message and the stack's error code, never the URL, a header or a body.
//!
//! This module holds what needs neither system, so its tests run everywhere: the target parsed from the URL, the
//! header block, the time budget, the bounded body reader and the error messages. The system calls are in `winhttp`,
//! which only Windows compiles, `urlsession`, which only macOS compiles, and `libcurl`, which Linux compiles (and the
//! Mac for its tests).

use std::fmt::Write as _;
use std::net::Ipv6Addr;
use std::time::Duration;

use crate::error::NetError;
use crate::service::header_safe;

#[cfg(any(windows, test))]
mod budget;
#[cfg(test)]
#[cfg(any(target_os = "macos", target_os = "linux"))]
mod conformance;
#[cfg(any(not(any(windows, target_os = "macos")), all(test, target_os = "macos")))]
#[cfg_attr(
    target_os = "macos",
    allow(
        dead_code,
        reason = "Linux's transport; the Mac runs its tests against its own libcurl"
    )
)]
mod libcurl;
#[cfg(test)]
mod tests;
#[cfg(target_os = "macos")]
#[allow(
    unsafe_code,
    reason = "the NSURLSession calls; each is behind a safe function with a SAFETY comment"
)]
mod urlsession;
#[cfg(windows)]
#[allow(
    unsafe_code,
    reason = "the WinHTTP calls; each is behind a safe function with a SAFETY comment"
)]
mod winhttp;

#[cfg(any(windows, test))]
use budget::{Budget, Timeouts};
#[cfg(not(any(windows, target_os = "macos")))]
pub(crate) use libcurl::{exchange, exchange_stream};
#[cfg(target_os = "macos")]
pub(crate) use urlsession::{exchange, exchange_stream};
#[cfg(windows)]
pub(crate) use winhttp::{exchange, exchange_stream};

/// The port of an `https://` URL that names none.
const HTTPS_PORT: u16 = 443;
/// How many bytes one read asks `WinHTTP` for at most.
#[cfg(any(windows, test))]
const CHUNK_BYTES: usize = 16 * 1024;

// The `WinHTTP` error codes this module names (`winhttp.h`); a Windows test checks them against the SDK's.
const TIMEOUT: u32 = 12_002;
const NAME_NOT_RESOLVED: u32 = 12_007;
const CANNOT_CONNECT: u32 = 12_029;
const CONNECTION_ERROR: u32 = 12_030;
const SECURE_CERT_DATE_INVALID: u32 = 12_037;
const SECURE_CERT_CN_INVALID: u32 = 12_038;
const CLIENT_AUTH_CERT_NEEDED: u32 = 12_044;
const SECURE_INVALID_CA: u32 = 12_045;
const SECURE_CERT_REV_FAILED: u32 = 12_057;
const INVALID_SERVER_RESPONSE: u32 = 12_152;
const REDIRECT_FAILED: u32 = 12_156;
const SECURE_CHANNEL_ERROR: u32 = 12_157;
const BAD_AUTO_PROXY_SCRIPT: u32 = 12_166;
const UNABLE_TO_DOWNLOAD_SCRIPT: u32 = 12_167;
const SECURE_INVALID_CERT: u32 = 12_169;
const SECURE_CERT_REVOKED: u32 = 12_170;
const SECURE_FAILURE: u32 = 12_175;
const AUTODETECTION_FAILED: u32 = 12_180;

/// Whether `WinHTTP` may follow a redirect.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Redirects {
    /// Never: a 3xx answer is the answer, as curl without `--location` gives it.
    Never,
    /// At most this many, each only to HTTPS.
    HttpsOnly(u32),
}

/// What one request may use: its redirects, its time and the size of its answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Limits {
    /// Which redirects are followed.
    pub(crate) redirects: Redirects,
    /// The longest the whole request may take.
    pub(crate) budget: Duration,
    /// The most bytes of the answer's body that are read; a longer one is refused, never cut short.
    pub(crate) bytes: usize,
}

/// Where a request goes: the host, the port and the path with its query. Never printed: the path and query can name
/// the person or carry a value.
#[cfg_attr(test, derive(Debug))]
#[derive(PartialEq, Eq)]
pub(crate) struct Target {
    /// A DNS name or an IP address, an IPv6 address without its brackets.
    pub(crate) host: String,
    /// The TCP port.
    pub(crate) port: u16,
    /// The path and query, starting with `/`; the fragment never leaves the device.
    pub(crate) path: String,
}

/// One HTTPS request, checked and ready for `WinHTTP`.
pub(crate) struct Exchange<'a> {
    /// The HTTP method.
    pub(crate) method: &'static str,
    /// Where it goes.
    pub(crate) target: Target,
    /// The headers as one block of `Name: value` lines in UTF-16, without a terminating NUL.
    pub(crate) headers: Vec<u16>,
    /// The body, which may be empty.
    pub(crate) body: &'a [u8],
    /// The body's length, which `WinHTTP` takes as a `u32`.
    #[cfg_attr(
        not(any(windows, test)),
        allow(dead_code, reason = "only the WinHTTP calls read it")
    )]
    pub(crate) body_length: u32,
    /// The redirects, time and answer size it may use.
    pub(crate) limits: Limits,
}

impl<'a> Exchange<'a> {
    /// A request without a body.
    ///
    /// # Errors
    /// Returns [`NetError::Connection`] for a URL that is not `https://`, carries credentials or is not valid, and for a
    /// header that could end its line early.
    pub(crate) fn new(
        method: &'static str,
        url: &str,
        headers: &[(&str, String)],
        limits: Limits,
    ) -> Result<Self, NetError> {
        Ok(Self {
            method,
            target: target_of(url)?,
            headers: header_block(headers)?,
            body: &[],
            body_length: 0,
            limits,
        })
    }

    /// The same request with `body`.
    ///
    /// # Errors
    /// Returns [`NetError::Connection`] for a body of 4 GiB or more, which `WinHTTP` cannot send in one call.
    pub(crate) fn with_body(self, body: &'a [u8]) -> Result<Self, NetError> {
        Ok(Self {
            body,
            body_length: body_length(body.len())?,
            ..self
        })
    }
}

/// What the server answered.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Answer {
    /// The status and the whole body, at most the limit.
    Complete {
        /// The HTTP status code.
        status: u16,
        /// The body.
        body: Vec<u8>,
    },
    /// The body is longer than the limit; reading stopped there.
    TooLarge,
}

/// A streamed file response, including how many bytes can be resumed after a broken connection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum StreamedBody {
    /// The response reached its end normally.
    Finished(u64),
    /// The response broke off after this many bytes were written.
    BrokeOff(u64),
    /// The response exceeded the caller's byte limit.
    TooLarge,
}

/// Why a request failed. Holds codes only, never a value of the request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Failure {
    /// `WinHTTP` could not set the request up (a handle or an option), with the system error code.
    Setup(u32),
    /// The exchange with the server or the proxy failed, with the `WinHTTP` or system error code.
    #[cfg_attr(
        not(any(windows, test)),
        allow(
            dead_code,
            reason = "only the WinHTTP calls, which Windows compiles, report it"
        )
    )]
    Exchange(u32),
    /// The time budget ran out.
    TimedOut,
    /// The answer has no valid HTTP status.
    NoStatus,
    /// The proxy asks for sign-in (407), which Somicus does not answer.
    ProxyAuthentication,
    /// libcurl ended the request with this `CURLcode`.
    #[cfg_attr(
        any(windows, all(target_os = "macos", not(test))),
        allow(
            dead_code,
            reason = "only the libcurl calls, which Linux compiles, report it"
        )
    )]
    Curl(u32),
    /// The environment asks for a TLS key log (`SSLKEYLOGFILE`), so nothing is sent (ADR-0049 decision 3).
    #[cfg_attr(
        any(windows, all(target_os = "macos", not(test))),
        allow(
            dead_code,
            reason = "only the libcurl calls, which Linux compiles, refuse it"
        )
    )]
    KeyLog,
    /// `NSURLSession` ended the request with this `NSURLError` code (negative, as Foundation defines them).
    #[cfg_attr(
        not(target_os = "macos"),
        allow(
            dead_code,
            reason = "only the NSURLSession calls, which macOS compiles, report it"
        )
    )]
    Url(i32),
}

impl Failure {
    /// What went wrong, in fixed words with the error code.
    fn message(self) -> String {
        match self {
            Self::Setup(code) => {
                format!("the system's HTTPS stack could not set up the request (error {code})")
            }
            Self::Exchange(code) => format!("{} (WinHTTP error {code})", exchange_message(code)),
            Self::Url(code) => format!("{} (NSURLSession error {code})", url_message(code)),
            Self::Curl(code) => format!("{} (libcurl error {code})", curl_message(code)),
            Self::KeyLog => {
                "the environment asks for a TLS key log, so Somicus sends nothing".to_owned()
            }
            Self::TimedOut => "the HTTPS request timed out".to_owned(),
            Self::NoStatus => "the service sent no HTTP status".to_owned(),
            Self::ProxyAuthentication => {
                "the proxy asks for sign-in, which Somicus does not answer".to_owned()
            }
        }
    }
}

impl From<Failure> for NetError {
    fn from(failure: Failure) -> Self {
        Self::Connection(failure.message())
    }
}

/// The fixed words for a failed exchange's `WinHTTP` error code.
fn exchange_message(code: u32) -> &'static str {
    match code {
        TIMEOUT => "the HTTPS request timed out",
        NAME_NOT_RESOLVED => "the service's host name could not be resolved",
        CANNOT_CONNECT | CONNECTION_ERROR => "the service could not be reached",
        SECURE_FAILURE
        | SECURE_CHANNEL_ERROR
        | SECURE_INVALID_CA
        | SECURE_CERT_CN_INVALID
        | SECURE_CERT_DATE_INVALID
        | SECURE_INVALID_CERT
        | SECURE_CERT_REVOKED
        | SECURE_CERT_REV_FAILED
        | CLIENT_AUTH_CERT_NEEDED => "the service's TLS connection or certificate was refused",
        INVALID_SERVER_RESPONSE => "the service's answer is not HTTP",
        REDIRECT_FAILED => "a redirect was refused",
        AUTODETECTION_FAILED | BAD_AUTO_PROXY_SCRIPT | UNABLE_TO_DOWNLOAD_SCRIPT => {
            "the system's proxy configuration could not be used"
        }
        _ => "the HTTPS request failed",
    }
}

/// The fixed words for a libcurl `CURLcode` (`curl/curl.h`).
fn curl_message(code: u32) -> &'static str {
    match code {
        28 => "the HTTPS request timed out",
        5 | 6 => "the service's host name could not be resolved",
        7 | 52 | 55 | 56 => "the service could not be reached",
        35 | 51 | 53 | 54 | 58 | 59 | 60 | 64 | 66 | 77 | 80 | 82 | 83 | 90 | 91 | 98 => {
            "the service's TLS connection or certificate was refused"
        }
        8 => "the service's answer is not HTTP",
        47 => "a redirect was refused",
        1 | 3 => "the system refused a request that is not HTTPS",
        _ => "the HTTPS request failed",
    }
}

/// The fixed words for an `NSURLError` code (`NSURLError.h`).
fn url_message(code: i32) -> &'static str {
    match code {
        -1001 => "the HTTPS request timed out",
        -1003 | -1006 => "the service's host name could not be resolved",
        -1004 | -1005 | -1009 | -1018 | -1020 => "the service could not be reached",
        -1206..=-1200 => "the service's TLS connection or certificate was refused",
        -1011 => "the service's answer is not HTTP",
        -1007 | -1010 => "a redirect was refused",
        -1012 | -1013 => "the service or a proxy asks for sign-in, which Somicus does not answer",
        -1022 => "the system refused a request that is not HTTPS",
        _ => "the HTTPS request failed",
    }
}

/// The Win32 error code an `HRESULT` from `HRESULT_FROM_WIN32` carries, or the `HRESULT`'s own bits when it carries
/// none.
#[cfg(any(windows, test))]
fn win32_code(hresult: i32) -> u32 {
    let bits = hresult.cast_unsigned();
    if bits & 0xFFFF_0000 == 0x8007_0000 {
        bits & 0xFFFF
    } else {
        bits
    }
}

/// The status of an answer: a 3-digit HTTP status, and never 407, which comes from a proxy that wants sign-in.
fn status_of(code: u32) -> Result<u16, Failure> {
    match u16::try_from(code) {
        Ok(407) => Err(Failure::ProxyAuthentication),
        Ok(status) if (100..600).contains(&status) => Ok(status),
        _ => Err(Failure::NoStatus),
    }
}

/// Where `url` goes. Only `https://`; no user name or password before the host; visible ASCII only, so nothing can end
/// the request line early; a DNS name, an IPv4 address or a bracketed IPv6 address; a port from 1 to 65535.
fn target_of(url: &str) -> Result<Target, NetError> {
    let invalid = || NetError::Connection("the service URL is not valid".to_owned());
    let rest = url.strip_prefix("https://").ok_or_else(|| {
        NetError::Connection("the native transport sends only https URLs".to_owned())
    })?;
    if !rest.bytes().all(|byte| byte.is_ascii_graphic()) {
        return Err(invalid());
    }
    let rest = rest.split('#').next().unwrap_or_default();
    let (authority, path) = rest.split_at(rest.find(['/', '?']).unwrap_or(rest.len()));
    if authority.contains('@') {
        return Err(NetError::Connection(
            "a service URL may not carry credentials".to_owned(),
        ));
    }
    let (host, port) = host_and_port(authority).ok_or_else(invalid)?;
    let path = match path.chars().next() {
        None => "/".to_owned(),
        Some('?') => format!("/{path}"),
        Some(_) => path.to_owned(),
    };
    Ok(Target {
        host: host.to_owned(),
        port,
        path,
    })
}

/// The host and port of an authority without credentials, when both are valid.
fn host_and_port(authority: &str) -> Option<(&str, u16)> {
    let (host, port, bracketed) = if let Some(inner) = authority.strip_prefix('[') {
        let (address, after) = inner.split_once(']')?;
        address.parse::<Ipv6Addr>().ok()?;
        let port = if after.is_empty() {
            None
        } else {
            Some(after.strip_prefix(':')?)
        };
        (address, port, true)
    } else {
        match authority.split_once(':') {
            Some((host, port)) => (host, Some(port), false),
            None => (authority, None, false),
        }
    };
    let port = match port {
        None => HTTPS_PORT,
        Some(digits) if !digits.is_empty() && digits.bytes().all(|byte| byte.is_ascii_digit()) => {
            digits.parse::<u16>().ok().filter(|port| *port != 0)?
        }
        Some(_) => return None,
    };
    let name = bracketed
        || host
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'.');
    (!host.is_empty() && host.len() <= 253 && name).then_some((host, port))
}

/// Whether `name` may be a header name: an RFC 9110 token.
fn header_name_safe(name: &str) -> bool {
    !name.is_empty()
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&byte))
}

/// The headers as `WinHTTP` takes them: `Name: value` lines joined by CRLF, in UTF-16, without a terminating NUL.
fn header_block(headers: &[(&str, String)]) -> Result<Vec<u16>, NetError> {
    let mut block = String::new();
    for (name, value) in headers {
        if !header_name_safe(name) || !header_safe(value) {
            return Err(NetError::Connection(
                "a request header is not valid".to_owned(),
            ));
        }
        if !block.is_empty() {
            block.push_str("\r\n");
        }
        let _ = write!(block, "{name}: {value}");
    }
    Ok(block.encode_utf16().collect())
}

/// The body's length as `WinHTTP` takes it.
fn body_length(length: usize) -> Result<u32, NetError> {
    u32::try_from(length)
        .map_err(|_| NetError::Connection("the request body is too large to send".to_owned()))
}

/// `text` as a NUL-terminated UTF-16 string. Every caller passes checked text without a NUL of its own.
#[cfg(any(windows, test))]
fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Reads chunks from `read` until it gives 0 bytes, and gives `None` as soon as there would be more than `limit`: a
/// longer answer is refused, never cut short, and never read to its end.
#[cfg(any(windows, test))]
fn read_limited<E>(
    mut read: impl FnMut(&mut [u8]) -> Result<usize, E>,
    limit: usize,
) -> Result<Option<Vec<u8>>, E> {
    let mut body = Vec::new();
    let mut chunk = vec![0_u8; CHUNK_BYTES];
    loop {
        let count = read(&mut chunk)?.min(chunk.len());
        if count == 0 {
            return Ok(Some(body));
        }
        if count > limit.saturating_sub(body.len()) {
            return Ok(None);
        }
        body.extend_from_slice(&chunk[..count]);
    }
}

/// Streams no more than `limit` bytes without keeping the response in memory. A read failure preserves the number of
/// bytes already written so the pinned downloader can resume; a sink failure is returned to the caller.
#[cfg(any(windows, test))]
fn read_streamed<E>(
    mut read: impl FnMut(&mut [u8]) -> Result<usize, E>,
    limit: u64,
    sink: &mut dyn FnMut(&[u8]) -> std::io::Result<()>,
) -> std::io::Result<StreamedBody> {
    let mut written = 0_u64;
    let mut chunk = [0_u8; CHUNK_BYTES];
    loop {
        let count = match read(&mut chunk) {
            Ok(count) => count.min(chunk.len()),
            Err(_) => return Ok(StreamedBody::BrokeOff(written)),
        };
        if count == 0 {
            return Ok(StreamedBody::Finished(written));
        }
        let count = u64::try_from(count).unwrap_or(u64::MAX);
        if count > limit.saturating_sub(written) {
            return Ok(StreamedBody::TooLarge);
        }
        sink(&chunk[..usize::try_from(count).unwrap_or(chunk.len())])?;
        written += count;
    }
}
