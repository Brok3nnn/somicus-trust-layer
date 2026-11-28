//! Somicus network egress and the typed [`Redacted<T>`] boundary.
//!
//! [`send`] is the one request path the app uses on every operating system: plain HTTP only to this machine, HTTPS
//! inside the process through the system's own stack (ADR-0049): `WinHTTP` on Windows, `NSURLSession` on macOS and the
//! distribution's libcurl on Linux. No request starts a process. The two documented requests without a [`Redacted<T>`]
//! body are [`fetch_site_icon`] (ADR-0027) and the [`Downloader`] of the on-device name model's hash-pinned files
//! (ADR-0042).
//!
//! This is the only crate of the workspace that opens connections: elsewhere Clippy's `disallowed-methods`
//! (`clippy.toml`) rejects socket, DNS and process calls, and `deny.toml` keeps HTTP clients and the system stacks'
//! bindings out.
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
pub mod service;
pub mod site_icon;

pub use error::NetError;
pub use pinned_file::NativeFileTransport;
pub use pinned_file::{
    DownloadError, Downloader, FetchRequest, Fetched, FileTransport, PinnedFile, Retry,
    is_download_url,
};
pub use service::{MAX_RESPONSE_BYTES, Method, ServiceRequest, ServiceResponse, send};
pub use site_icon::{SiteIcon, fetch_site_icon, is_icon_host};
/// The egress brand, defined by the Privacy Shield so that only its output or reviewed framing can be sent.
pub use somicus_privacy::Redacted;
