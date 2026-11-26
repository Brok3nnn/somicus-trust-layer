//! Somicus network egress and the typed [`Redacted<T>`] boundary.
//!
//! [`send`] is the one request path the app uses on every operating system: plain HTTP only to this machine, HTTPS
//! inside the process through the system's own stack (ADR-0049): `WinHTTP` on Windows, `NSURLSession` on macOS and the
//! distribution's libcurl on Linux. No request starts a process. [`fetch_site_icon`] is one
//! documented request without a [`Redacted<T>`] body (ADR-0027); [`Downloader`] fetches the on-device name model's
//! hash-pinned files without a body (ADR-0042). The [`Transport`] trait is the boundary for OS-native session
//! transports; [`UrlSessionTransport`] posts through [`send`] and opens no session connection yet.
//!
//! `unsafe` code exists only in the `WinHTTP` module that Windows compiles and the `NSURLSession` module that macOS
//! compiles; everywhere else, Linux included, the crate forbids it.

#![cfg_attr(not(any(windows, target_os = "macos")), forbid(unsafe_code))]
#![cfg_attr(any(windows, target_os = "macos"), deny(unsafe_code))]
#![deny(missing_docs)]

mod debug;
pub mod error;
mod native;
pub mod pinned_file;
pub mod proxy;
pub mod service;
pub mod site_icon;
pub mod transport;
pub mod urlsession;

pub use error::NetError;
pub use pinned_file::NativeFileTransport;
pub use pinned_file::{
    DownloadError, Downloader, FetchRequest, Fetched, FileTransport, PinnedFile, Retry,
    is_download_url,
};
pub use proxy::{ProxyAuth, ProxyConfig};
pub use service::{MAX_RESPONSE_BYTES, Method, ServiceRequest, ServiceResponse, send};
pub use site_icon::{SiteIcon, fetch_site_icon, is_icon_host};
/// The egress brand, defined by the Privacy Shield so that only its output or reviewed framing can be sent.
pub use somicus_privacy::Redacted;
pub use transport::{BoxFuture, SessionConnection, Transport};
pub use urlsession::UrlSessionTransport;
