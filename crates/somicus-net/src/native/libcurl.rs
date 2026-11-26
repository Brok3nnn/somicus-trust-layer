//! HTTPS through libcurl inside the process, which Linux compiles (ADR-0049): the distribution's own library, its TLS
//! library and its trusted roots, and the proxy the environment names, through the `curl` crate's safe API. The Mac's
//! tests run the same code against macOS's libcurl.
//!
//! Each request is one easy handle: no cookie engine, no `.netrc`, no authentication, no signals (a multithreaded
//! process), TLS 1.2 or later, and the distribution's CA bundle named explicitly so no environment variable picks the
//! roots. libcurl follows no redirect: a redirect the caller allows (at most so many, only to HTTPS) is a new request.
//! The body is passed on piece by piece, and the transfer is aborted the moment it passes its limit. A process whose
//! environment asks for a TLS key log (`SSLKEYLOGFILE`) sends nothing.

use std::cell::Cell;
use std::path::Path;
use std::time::{Duration, Instant};

use curl::easy::{Easy, List, SslVersion};

use super::{Answer, Exchange, Failure, Redirects, StreamedBody, status_of};
use crate::error::NetError;

/// The user agent the requests send, as `WinHTTP`'s and `NSURLSession`'s do.
const AGENT: &str = "Somicus";

/// The distributions' CA bundles, in the order they are looked for: Debian and Ubuntu, Fedora and RHEL, openSUSE, and
/// Alpine and Arch. libcurl's own default is used only on a system that has none of them.
const CA_BUNDLES: [&str; 4] = [
    "/etc/ssl/certs/ca-certificates.crt",
    "/etc/pki/tls/certs/ca-bundle.crt",
    "/etc/ssl/ca-bundle.pem",
    "/etc/ssl/cert.pem",
];

/// The longest a connection may take to open, within the request's own budget.
const CONNECT_SECONDS: u64 = 10;

/// The first CA bundle this system has.
fn ca_bundle() -> Option<&'static str> {
    CA_BUNDLES
        .into_iter()
        .find(|path| Path::new(path).is_file())
}

/// libcurl's code of `error`.
fn code_of(error: &curl::Error) -> u32 {
    error.code()
}

/// A failure setting an option up, with libcurl's code.
fn setup(result: Result<(), curl::Error>) -> Result<(), Failure> {
    result.map_err(|error| Failure::Setup(code_of(&error)))
}

/// What a failed transfer stands for: a proxy that asks for sign-in, the budget, or libcurl's code.
fn failure_of(easy: &Easy, error: &curl::Error) -> Failure {
    if easy.http_connectcode().ok() == Some(407) {
        Failure::ProxyAuthentication
    } else if error.is_operation_timedout() {
        Failure::TimedOut
    } else {
        Failure::Curl(code_of(error))
    }
}

/// The URL of `exchange` with `scheme`: `https` for every request the app sends.
fn url_of(exchange: &Exchange<'_>, scheme: &str) -> String {
    let target = &exchange.target;
    if target.host.contains(':') {
        format!(
            "{scheme}://[{}]:{}{}",
            target.host, target.port, target.path
        )
    } else {
        format!("{scheme}://{}:{}{}", target.host, target.port, target.path)
    }
}

/// The headers of `exchange` as libcurl takes them, from its checked `Name: value` block, and an empty `Expect` so a
/// body is never held back for a `100 Continue`.
fn header_list(exchange: &Exchange<'_>) -> Result<List, Failure> {
    let mut list = List::new();
    for line in String::from_utf16_lossy(&exchange.headers)
        .split("\r\n")
        .filter(|line| !line.is_empty())
    {
        setup(list.append(line))?;
    }
    setup(list.append("Expect:"))?;
    Ok(list)
}

/// Refuses to send when the environment names a TLS key log, which libcurl's TLS library would write.
fn refuse_key_log(key_log: Option<&std::ffi::OsStr>) -> Result<(), Failure> {
    key_log.map_or(Ok(()), |_| Err(Failure::KeyLog))
}

/// The easy handle for one request to `url`, with `left` of the budget.
fn handle(exchange: &Exchange<'_>, url: &str, left: Duration) -> Result<Easy, Failure> {
    refuse_key_log(std::env::var_os("SSLKEYLOGFILE").as_deref())?;
    let mut easy = Easy::new();
    setup(easy.signal(false))?;
    setup(easy.url(url))?;
    if exchange.body.is_empty() {
        setup(easy.get(true))?;
    } else {
        setup(easy.post_fields_copy(exchange.body))?;
    }
    setup(easy.custom_request(exchange.method))?;
    setup(easy.http_headers(header_list(exchange)?))?;
    setup(easy.useragent(AGENT))?;
    setup(easy.follow_location(false))?;
    setup(easy.timeout(left))?;
    setup(easy.connect_timeout(left.min(Duration::from_secs(CONNECT_SECONDS))))?;
    setup(easy.ssl_min_max_version(SslVersion::Tlsv12, SslVersion::Default))?;
    if let Some(bundle) = ca_bundle() {
        setup(easy.cainfo(bundle))?;
    }
    Ok(easy)
}

/// What one transfer gave: the status, where a redirect points, and the body or that it was too large.
struct Transfer {
    status: u16,
    location: Option<String>,
    body: Option<Vec<u8>>,
}

/// Performs one request to `url`, reading at most `limit` bytes of its body.
fn transfer(
    exchange: &Exchange<'_>,
    url: &str,
    deadline: Instant,
    limit: usize,
) -> Result<Transfer, Failure> {
    let left = deadline.saturating_duration_since(Instant::now());
    if left.is_zero() {
        return Err(Failure::TimedOut);
    }
    let mut easy = handle(exchange, url, left)?;
    let mut body = Vec::new();
    let mut too_large = false;
    let mut location = None;
    let performed = {
        let mut transfer = easy.transfer();
        setup(transfer.header_function(|line| {
            if let Ok(line) = std::str::from_utf8(line)
                && let Some((name, value)) = line.split_once(':')
                && name.eq_ignore_ascii_case("location")
            {
                location = Some(value.trim().to_owned());
            }
            true
        }))?;
        setup(transfer.write_function(|data| {
            if data.len() > limit.saturating_sub(body.len()) {
                too_large = true;
                return Ok(0);
            }
            body.extend_from_slice(data);
            Ok(data.len())
        }))?;
        transfer.perform()
    };
    if let Err(error) = performed
        && !(too_large && error.is_write_error())
    {
        return Err(failure_of(&easy, &error));
    }
    let status = status_of(
        easy.response_code()
            .map_err(|error| Failure::Curl(code_of(&error)))?,
    )?;
    Ok(Transfer {
        status,
        location,
        body: (!too_large).then_some(body),
    })
}

/// Where a redirect from `url` to `location` goes, when it may be followed: only to HTTPS.
fn redirect_target(url: &str, location: &str) -> Option<String> {
    if location.starts_with("https://") {
        return Some(location.to_owned());
    }
    // A path on the same host keeps the scheme of `url`, which is HTTPS only when `url` is.
    let after_scheme = url.strip_prefix("https://")?;
    let authority = after_scheme.split('/').next()?;
    location
        .starts_with('/')
        .then(|| format!("https://{authority}{location}"))
}

/// Sends `exchange` and reads its answer. Blocking: run it off the async runtime.
///
/// # Errors
/// Returns [`Failure`] when the request cannot be set up, the exchange fails, the budget runs out, the answer has no
/// valid status, a proxy asks for sign-in, or the environment asks for a TLS key log.
pub(crate) fn exchange(exchange: &Exchange<'_>) -> Result<Answer, Failure> {
    run(exchange, "https")
}

fn run(exchange: &Exchange<'_>, scheme: &str) -> Result<Answer, Failure> {
    let deadline = Instant::now() + exchange.limits.budget;
    let mut url = url_of(exchange, scheme);
    let mut followed = 0;
    loop {
        let answer = transfer(exchange, &url, deadline, exchange.limits.bytes)?;
        if let (Redirects::HttpsOnly(most), true, Some(location)) = (
            exchange.limits.redirects,
            (300..400).contains(&answer.status),
            answer.location.as_deref(),
        ) && followed < most
            && let Some(next) = redirect_target(&url, location)
        {
            followed += 1;
            url = next;
            continue;
        }
        return Ok(answer
            .body
            .map_or(Answer::TooLarge, |body| Answer::Complete {
                status: answer.status,
                body,
            }));
    }
}

/// Streams a file answer into `sink`, accepting only `expected_status` and never following redirects. A broken read
/// reports the bytes already written so the pinned downloader can retry from that offset.
///
/// # Errors
/// Returns [`NetError`] when the request cannot be set up or the sink fails.
pub(crate) fn exchange_stream(
    exchange: &Exchange<'_>,
    expected_status: u16,
    sink: &mut dyn FnMut(&[u8]) -> std::io::Result<()>,
) -> Result<StreamedBody, NetError> {
    stream(exchange, "https", expected_status, sink)
}

/// How a streamed transfer stopped early.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Stop {
    WrongStatus,
    TooLarge,
    Sink,
}

fn stream(
    exchange: &Exchange<'_>,
    scheme: &str,
    expected_status: u16,
    sink: &mut dyn FnMut(&[u8]) -> std::io::Result<()>,
) -> Result<StreamedBody, NetError> {
    let mut easy = handle(exchange, &url_of(exchange, scheme), exchange.limits.budget)?;
    let limit = u64::try_from(exchange.limits.bytes).unwrap_or(u64::MAX);
    let mut written = 0_u64;
    let status = Cell::new(None);
    let mut stop = None;
    let mut sink_error = None;
    let outcome = {
        let mut transfer = easy.transfer();
        transfer
            .header_function(|line| {
                if let Some(code) = std::str::from_utf8(line)
                    .ok()
                    .and_then(|line| line.strip_prefix("HTTP/"))
                    .and_then(|rest| rest.split_whitespace().nth(1))
                    .and_then(|code| code.parse::<u16>().ok())
                {
                    status.set(Some(code));
                }
                true
            })
            .map_err(|error| NetError::from(Failure::Setup(code_of(&error))))?;
        transfer
            .write_function(|data| {
                if status.get() != Some(expected_status) {
                    stop = Some(Stop::WrongStatus);
                    return Ok(0);
                }
                let count = u64::try_from(data.len()).unwrap_or(u64::MAX);
                if count > limit.saturating_sub(written) {
                    stop = Some(Stop::TooLarge);
                    return Ok(0);
                }
                if let Err(error) = sink(data) {
                    stop = Some(Stop::Sink);
                    sink_error = Some(error);
                    return Ok(0);
                }
                written += count;
                Ok(data.len())
            })
            .map_err(|error| NetError::from(Failure::Setup(code_of(&error))))?;
        transfer.perform()
    };
    match (stop, outcome) {
        (Some(Stop::Sink), _) => {
            Err(NetError::Io(sink_error.unwrap_or_else(|| {
                std::io::Error::other("the download could not be written")
            })))
        }
        (Some(Stop::TooLarge), _) => Ok(StreamedBody::TooLarge),
        (None, Ok(())) if status.get() == Some(expected_status) => {
            Ok(StreamedBody::Finished(written))
        }
        (Some(Stop::WrongStatus), _) | (None, Ok(())) => Ok(StreamedBody::BrokeOff(0)),
        (None, Err(_)) => Ok(StreamedBody::BrokeOff(written)),
    }
}

#[cfg(test)]
mod tests;
