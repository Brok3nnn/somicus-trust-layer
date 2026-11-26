//! On-device person-name recognition for the Privacy Shield (ADR-0042): `dslim/distilbert-NER` (Apache-2.0), a
//! `DistilBERT` token classifier, run by candle on the CPU, behind the `NameRecognizer` trait of `somicus-privacy`.
//!
//! - [`NameModel::load`] reads the model folder the desktop provides, checks each file's length and SHA-256 against the
//!   pins compiled into the app ([`pins`]), and parses the very bytes it checked. A missing or altered file loads
//!   nothing; the shield then works with its rules only.
//! - [`NameModel`] marks person names (`B-PER`, `I-PER`) in what the rules left of a text, in windows of at most
//!   [`MAX_PIECES`] word pieces, batched, until the turn's deadline, and caches the offsets it found per text.
//! - [`convert::to_half_precision`] makes the 16-bit file the app downloads from the pinned upstream file.
//!
//! This crate reads files and never opens a network connection; downloading is `somicus-net`'s.

#![forbid(unsafe_code)]

pub mod convert;
mod distilbert;
pub mod error;
mod gelu;
pub mod load;
pub mod pins;
mod recognizer;
mod spans;
mod tokenizer;
mod views;

pub use error::NerError;
pub use recognizer::{MAX_PIECES, NameModel};
