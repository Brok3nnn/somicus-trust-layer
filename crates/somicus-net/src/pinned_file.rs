//! Files the app downloads once and accepts only with a pinned length and SHA-256, such as the on-device name model of
//! ADR-0042.
//!
//! Like the site icon (ADR-0027), this request carries no [`Redacted`](crate::Redacted) body: it is an HTTPS `GET` to
//! the Somicus download host, without cookies, credentials or a query, so it carries nothing about the person. The
//! answer is streamed into a temporary file next to its destination and hashed as it arrives; it may never be longer
//! than the pinned length, and the file is moved into place only when its length and SHA-256 match. A download that
//! breaks off is resumed from where it stopped on the next attempt, with a bounded number of attempts and a pause
//! that grows between them. Nothing about the request is logged but its outcome and size.

use std::fmt::Write as _;
use std::fs::{self, File, OpenOptions};
use std::io::{Read as _, Write as _};
use std::path::{Path, PathBuf};
use std::time::Duration;

use sha2::{Digest as _, Sha256};

use crate::error::NetError;
use crate::native::{Exchange, Limits, Redirects, StreamedBody, exchange_stream};

/// A file accepted only with exactly this length and SHA-256.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PinnedFile<'a> {
    /// Where to fetch it: an `https://` URL without user information, query or fragment.
    pub url: &'a str,
    /// Its length in bytes.
    pub length: u64,
    /// Its SHA-256, lower-case hexadecimal.
    pub sha256: &'a str,
}

/// How many times a download is tried and how long it waits before each new attempt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Retry {
    /// Attempts in all, at least one.
    pub attempts: u32,
    /// The pause before the second attempt; each later pause is twice the one before.
    pub first_pause: Duration,
}

/// What a transport did with one request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fetched {
    /// The answer ended normally after this many bytes.
    Finished(u64),
    /// The answer broke off, or the server refused, after this many bytes.
    BrokeOff(u64),
    /// The answer was longer than allowed, and was stopped.
    TooLong,
}

/// One request of a download: the part of the file at `url` from byte `offset`, at most `limit` bytes of it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FetchRequest<'a> {
    /// The file's address.
    pub url: &'a str,
    /// The first byte wanted.
    pub offset: u64,
    /// The most bytes accepted.
    pub limit: u64,
}

/// Fetches the bytes of a file over the network: the system's own stack (ADR-0049), or a fake in tests.
pub trait FileTransport: Send + Sync {
    /// Streams the part of the file `request` asks for to `sink`, never handing it more than `request.limit` bytes.
    ///
    /// # Errors
    /// Returns [`NetError`] when the request cannot run or `sink` fails.
    fn fetch(
        &self,
        request: &FetchRequest<'_>,
        sink: &mut dyn FnMut(&[u8]) -> std::io::Result<()>,
    ) -> Result<Fetched, NetError>;
}

/// What carries out downloads: the transport, how often to try, and how to wait between attempts (a sleep in the app,
/// a recorder in tests).
pub struct Downloader<'a> {
    /// The transport.
    pub transport: &'a dyn FileTransport,
    /// Attempts and pauses.
    pub retry: Retry,
    /// Waits for a pause.
    pub pause: &'a dyn Fn(Duration),
}

/// Why a download did not put the file in place.
#[derive(Debug, thiserror::Error)]
pub enum DownloadError {
    /// The URL is not an `https://` URL without user information, query or fragment.
    #[error("the download address is not allowed")]
    Address,
    /// A whole file arrived with another length or SHA-256 than its pin, so it was deleted.
    #[error("the downloaded file does not have the pinned length and SHA-256")]
    Verification,
    /// Every attempt failed before the whole file arrived.
    #[error("the file could not be downloaded")]
    Unavailable,
    /// The file system refused a write, a read or the move into place.
    #[error("the downloaded file could not be stored ({0})")]
    Storage(std::io::ErrorKind),
}

impl From<std::io::Error> for DownloadError {
    fn from(error: std::io::Error) -> Self {
        Self::Storage(error.kind())
    }
}

/// The error's kind, for telemetry.
impl somicus_telemetry::Label for DownloadError {
    fn label(&self) -> &'static str {
        match self {
            Self::Address => "address",
            Self::Verification => "verification",
            Self::Unavailable => "unavailable",
            Self::Storage(_) => "storage",
        }
    }
}

/// Whether `url` may be downloaded from: `https://`, a host, and no user information, query or fragment.
#[must_use]
pub fn is_download_url(url: &str) -> bool {
    let Some(rest) = url.strip_prefix("https://") else {
        return false;
    };
    let host = rest.split('/').next().unwrap_or_default();
    !host.is_empty()
        && !host.contains('@')
        && !url.contains(['?', '#'])
        && url
            .bytes()
            .all(|byte| byte.is_ascii_graphic() && byte != b'"' && byte != b'\\')
}

fn hex(digest: &[u8]) -> String {
    digest
        .iter()
        .fold(String::with_capacity(64), |mut text, byte| {
            let _ = write!(text, "{byte:02x}");
            text
        })
}

/// The temporary file a download of `destination` is streamed into: beside it, so the move into place is a rename.
fn partial_path(destination: &Path) -> PathBuf {
    let mut name = destination.file_name().unwrap_or_default().to_os_string();
    name.push(".part");
    destination.with_file_name(name)
}

/// The SHA-256 state and length of what an earlier attempt left in `partial`, or a fresh start when nothing usable is
/// there.
fn resume_from(partial: &Path, length: u64) -> Result<(Sha256, u64), DownloadError> {
    let mut hasher = Sha256::new();
    let Ok(mut file) = File::open(partial) else {
        return Ok((hasher, 0));
    };
    if file.metadata()?.len() > length {
        drop(file);
        fs::remove_file(partial)?;
        return Ok((hasher, 0));
    }
    let mut buffer = vec![0u8; 64 * 1024];
    let mut kept = 0u64;
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
        kept += u64::try_from(read).unwrap_or(u64::MAX);
    }
    Ok((hasher, kept))
}

impl Downloader<'_> {
    /// Downloads `file` to `destination`, trying up to `retry.attempts` times with a growing pause between attempts,
    /// and moves it into place only when its length and SHA-256 match the pin. Blocking: run it off the async runtime,
    /// never on a turn's path.
    ///
    /// Its `net_pinned_file` span records the outcome and the size; never the URL or the path.
    ///
    /// # Errors
    /// Returns [`DownloadError`] when the URL is not allowed, the whole file arrived and failed its check, every
    /// attempt broke off, or the file system refused.
    pub fn download(&self, file: &PinnedFile<'_>, destination: &Path) -> Result<(), DownloadError> {
        let span = somicus_telemetry::span!(INFO, "net_pinned_file", transport = "https");
        let result = span.in_scope(|| self.download_unobserved(file, destination));
        if result.is_ok() {
            somicus_telemetry::record!(span, bytes = file.length);
        }
        span.record_result(&result);
        result
    }

    fn download_unobserved(
        &self,
        file: &PinnedFile<'_>,
        destination: &Path,
    ) -> Result<(), DownloadError> {
        if !is_download_url(file.url) {
            return Err(DownloadError::Address);
        }
        if let Some(folder) = destination.parent() {
            fs::create_dir_all(folder)?;
        }
        let partial = partial_path(destination);
        let mut wait = self.retry.first_pause;
        let mut started_over = false;
        for attempt in 0..self.retry.attempts.max(1) {
            if attempt > 0 {
                (self.pause)(wait);
                wait = wait.saturating_mul(2);
            }
            let (mut hasher, kept) = resume_from(&partial, file.length)?;
            let total = if kept == file.length {
                kept
            } else {
                let mut writer = OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(&partial)?;
                let mut sink = |chunk: &[u8]| {
                    hasher.update(chunk);
                    writer.write_all(chunk)
                };
                let request = FetchRequest {
                    url: file.url,
                    offset: kept,
                    limit: file.length - kept,
                };
                let fetched = self.transport.fetch(&request, &mut sink);
                writer.flush()?;
                drop(writer);
                match fetched {
                    Ok(Fetched::Finished(bytes) | Fetched::BrokeOff(bytes)) => {
                        kept.saturating_add(bytes)
                    }
                    Ok(Fetched::TooLong) => {
                        fs::remove_file(&partial)?;
                        continue;
                    }
                    Err(_) => continue,
                }
            };
            if total < file.length {
                continue;
            }
            if hex(&hasher.finalize()) == file.sha256 {
                fs::rename(&partial, destination)?;
                return Ok(());
            }
            // A whole file with the wrong hash: a resumed answer may have started from another place, so start over once;
            // a fresh download that fails the check is refused.
            fs::remove_file(&partial)?;
            if kept == 0 || started_over {
                return Err(DownloadError::Verification);
            }
            started_over = true;
        }
        Err(DownloadError::Unavailable)
    }
}

/// The longest one attempt may take: the model is about 130 MB.
const MAX_ATTEMPT: Duration = Duration::from_mins(15);

/// The native request for `request` and the only status its answer may have: 200 for a whole file, 206 for the rest
/// of a broken one. Redirects are never followed, so a 3xx answer is a failed attempt, as with curl without
/// `--location`.
fn file_exchange<'a>(request: &FetchRequest<'_>) -> Result<(Exchange<'a>, u16), NetError> {
    if !is_download_url(request.url) {
        return Err(NetError::Connection("not a download address".to_owned()));
    }
    let mut headers = vec![("Accept-Encoding", "identity".to_owned())];
    if request.offset > 0 {
        headers.push(("Range", format!("bytes={}-", request.offset)));
    }
    let limits = Limits {
        redirects: Redirects::Never,
        budget: MAX_ATTEMPT,
        bytes: usize::try_from(request.limit).unwrap_or(usize::MAX),
    };
    let exchange = Exchange::new("GET", request.url, &headers, limits)?;
    Ok((exchange, if request.offset == 0 { 200 } else { 206 }))
}

/// The app's file transport: the system's own stack (`WinHTTP`, `NSURLSession`, libcurl; ADR-0049) with no redirects,
/// cookies or credentials, streaming to the pinned downloader. No process is started.
#[derive(Debug, Default, Clone, Copy)]
pub struct NativeFileTransport;

impl FileTransport for NativeFileTransport {
    fn fetch(
        &self,
        request: &FetchRequest<'_>,
        sink: &mut dyn FnMut(&[u8]) -> std::io::Result<()>,
    ) -> Result<Fetched, NetError> {
        let (exchange, expected_status) = file_exchange(request)?;
        Ok(match exchange_stream(&exchange, expected_status, sink)? {
            StreamedBody::Finished(bytes) => Fetched::Finished(bytes),
            StreamedBody::BrokeOff(bytes) => Fetched::BrokeOff(bytes),
            StreamedBody::TooLarge => Fetched::TooLong,
        })
    }
}

#[cfg(test)]
mod tests;
