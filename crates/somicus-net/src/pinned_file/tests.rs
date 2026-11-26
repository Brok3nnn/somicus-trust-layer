//! The pinned download with a fake transport: no network is used.

use std::cell::RefCell;
use std::sync::Mutex;

use super::*;

/// Serves `body` from the requested offset, in the steps it is told: each step sends that many bytes and then ends
/// normally or breaks off.
struct Fake {
    body: Vec<u8>,
    steps: Mutex<Vec<(usize, bool)>>,
    offsets: Mutex<Vec<u64>>,
}

impl Fake {
    fn new(body: &[u8], steps: &[(usize, bool)]) -> Self {
        Self {
            body: body.to_vec(),
            steps: Mutex::new(steps.iter().rev().copied().collect()),
            offsets: Mutex::new(Vec::new()),
        }
    }

    fn offsets(&self) -> Vec<u64> {
        self.offsets
            .lock()
            .map(|offsets| offsets.clone())
            .unwrap_or_default()
    }
}

impl FileTransport for Fake {
    fn fetch(
        &self,
        request: &FetchRequest<'_>,
        sink: &mut dyn FnMut(&[u8]) -> std::io::Result<()>,
    ) -> Result<Fetched, NetError> {
        let FetchRequest { offset, limit, .. } = *request;
        if let Ok(mut offsets) = self.offsets.lock() {
            offsets.push(offset);
        }
        let step = self.steps.lock().ok().and_then(|mut steps| steps.pop());
        let Some((count, finishes)) = step else {
            return Err(NetError::Connection("offline".to_owned()));
        };
        let start = usize::try_from(offset)
            .unwrap_or(usize::MAX)
            .min(self.body.len());
        let end = (start + count).min(self.body.len());
        let sent = &self.body[start..end];
        if u64::try_from(sent.len()).unwrap_or(u64::MAX) > limit {
            return Ok(Fetched::TooLong);
        }
        sink(sent)?;
        let bytes = u64::try_from(sent.len()).unwrap_or(u64::MAX);
        Ok(if finishes {
            Fetched::Finished(bytes)
        } else {
            Fetched::BrokeOff(bytes)
        })
    }
}

const BODY: &[u8] = b"synthetic model weights, not a real model";
const URL: &str = "https://downloads.example.test/models/names-en-v1/abc/model.safetensors";

fn pin(body: &[u8]) -> (u64, String) {
    (
        u64::try_from(body.len()).unwrap_or(u64::MAX),
        hex(&Sha256::digest(body)),
    )
}

fn scratch(name: &str) -> PathBuf {
    // nosemgrep: rust.lang.security.temp-dir.temp-dir -- a test scratch directory, not a security operation
    let directory =
        std::env::temp_dir().join(format!("somicus-net-pinned-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&directory);
    directory
}

fn retry(attempts: u32) -> Retry {
    Retry {
        attempts,
        first_pause: Duration::from_secs(5),
    }
}

/// Runs a download of `BODY` (or of a pin for `pinned`) with `transport`, recording the pauses instead of sleeping.
fn download(
    name: &str,
    pinned: &[u8],
    transport: &Fake,
    attempts: u32,
) -> (Result<(), DownloadError>, PathBuf, Vec<Duration>) {
    let destination = scratch(name).join("models").join("model.safetensors");
    let (length, sha256) = pin(pinned);
    let pauses = RefCell::new(Vec::new());
    let record = |pause| pauses.borrow_mut().push(pause);
    let downloader = Downloader {
        transport,
        retry: retry(attempts),
        pause: &record,
    };
    let result = downloader.download(
        &PinnedFile {
            url: URL,
            length,
            sha256: &sha256,
        },
        &destination,
    );
    (result, destination, pauses.into_inner())
}

#[test]
fn a_whole_file_with_its_pinned_hash_is_moved_into_place() {
    let transport = Fake::new(BODY, &[(BODY.len(), true)]);
    let (result, destination, pauses) = download("whole", BODY, &transport, 3);
    assert!(result.is_ok(), "{result:?}");
    assert_eq!(fs::read(&destination).ok().as_deref(), Some(BODY));
    assert!(!partial_path(&destination).exists());
    assert_eq!(pauses, Vec::<Duration>::new());
    let _ = fs::remove_dir_all(scratch("whole"));
}

#[test]
fn a_file_with_another_hash_is_deleted_and_never_moved_into_place() {
    let tampered = b"synthetic model weights, not a real modeL";
    let transport = Fake::new(tampered, &[(tampered.len(), true)]);
    let (result, destination, _) = download("tampered", BODY, &transport, 3);
    assert!(
        matches!(result, Err(DownloadError::Verification)),
        "{result:?}"
    );
    assert!(!destination.exists());
    assert!(!partial_path(&destination).exists());
    let _ = fs::remove_dir_all(scratch("tampered"));
}

#[test]
fn a_broken_download_resumes_where_it_stopped() {
    let transport = Fake::new(BODY, &[(10, false), (BODY.len(), true)]);
    let (result, destination, pauses) = download("resume", BODY, &transport, 3);
    assert!(result.is_ok(), "{result:?}");
    assert_eq!(transport.offsets(), vec![0, 10]);
    assert_eq!(pauses, vec![Duration::from_secs(5)]);
    assert_eq!(fs::read(&destination).ok().as_deref(), Some(BODY));
    let _ = fs::remove_dir_all(scratch("resume"));
}

#[test]
fn attempts_are_bounded_and_their_pauses_grow() {
    let transport = Fake::new(BODY, &[(3, false), (3, false), (3, false), (3, false)]);
    let (result, destination, pauses) = download("bounded", BODY, &transport, 3);
    assert!(
        matches!(result, Err(DownloadError::Unavailable)),
        "{result:?}"
    );
    assert_eq!(transport.offsets(), vec![0, 3, 6]);
    assert_eq!(
        pauses,
        vec![Duration::from_secs(5), Duration::from_secs(10)]
    );
    assert!(!destination.exists());
    let _ = fs::remove_dir_all(scratch("bounded"));
}

#[test]
fn an_answer_longer_than_the_pin_is_refused() {
    let longer = [BODY, b" and more"].concat();
    let transport = Fake::new(&longer, &[(longer.len(), true)]);
    let (result, destination, _) = download("long", BODY, &transport, 1);
    assert!(
        matches!(result, Err(DownloadError::Unavailable)),
        "{result:?}"
    );
    assert!(!destination.exists());
    assert!(!partial_path(&destination).exists());
    let _ = fs::remove_dir_all(scratch("long"));
}

#[test]
fn only_plain_https_addresses_are_downloaded_from() {
    assert!(is_download_url(URL));
    for refused in [
        "http://downloads.example.test/model.safetensors",
        "https://downloads.example.test/model.safetensors?device=1",
        "https://downloads.example.test/model.safetensors#part",
        "https://user:secret@downloads.example.test/model.safetensors",
        "https:///model.safetensors",
        "https://downloads.example.test/a b",
        "file:///etc/passwd",
    ] {
        assert!(!is_download_url(refused), "{refused}");
    }
    let transport = Fake::new(BODY, &[(BODY.len(), true)]);
    let (length, sha256) = pin(BODY);
    let downloader = Downloader {
        transport: &transport,
        retry: retry(1),
        pause: &|_| {},
    };
    let result = downloader.download(
        &PinnedFile {
            url: "http://downloads.example.test/model.safetensors",
            length,
            sha256: &sha256,
        },
        &scratch("address").join("model.safetensors"),
    );
    assert!(matches!(result, Err(DownloadError::Address)));
    assert_eq!(transport.offsets(), Vec::<u64>::new());
}

#[test]
fn a_native_model_download_never_follows_a_redirect_and_asks_for_the_rest() -> Result<(), NetError>
{
    let whole = FetchRequest {
        url: URL,
        offset: 0,
        limit: 100,
    };
    let (exchange, status) = file_exchange(&whole)?;
    assert_eq!(exchange.limits.redirects, Redirects::Never);
    assert_eq!(exchange.limits.bytes, 100);
    assert_eq!(exchange.limits.budget, MAX_ATTEMPT);
    assert_eq!(exchange.method, "GET");
    assert_eq!(status, 200);
    assert_eq!(
        String::from_utf16_lossy(&exchange.headers),
        "Accept-Encoding: identity"
    );

    let rest = FetchRequest {
        url: URL,
        offset: 42,
        limit: 58,
    };
    let (exchange, status) = file_exchange(&rest)?;
    assert_eq!(status, 206);
    assert_eq!(
        String::from_utf16_lossy(&exchange.headers),
        "Accept-Encoding: identity\r\nRange: bytes=42-"
    );

    let plain = FetchRequest {
        url: "http://downloads.example.test/model.safetensors",
        offset: 0,
        limit: 100,
    };
    assert!(file_exchange(&plain).is_err());
    Ok(())
}
