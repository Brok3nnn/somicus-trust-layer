//! Loading the model: each file is read whole into memory, its length and SHA-256 are checked against the pin, and the
//! very bytes that were checked are parsed (no memory map, so nothing can change the file between the check and the
//! use). Safetensors and JSON carry no code.

use std::fmt::Write as _;
use std::io::Read as _;
use std::path::Path;

use sha2::{Digest as _, Sha256};

use crate::distilbert::{DistilBert, VOCABULARY};
use crate::error::NerError;
use crate::pins::{MODEL, PinnedFile, TOKENIZER};
use crate::recognizer::NameModel;
use crate::tokenizer::WordPiece;

/// The lower-case hexadecimal SHA-256 of `bytes`.
#[must_use]
pub fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .fold(String::with_capacity(64), |mut hex, byte| {
            let _ = write!(hex, "{byte:02x}");
            hex
        })
}

/// The bytes of the file at `path` when it has `pin`'s length and SHA-256.
///
/// # Errors
/// Returns [`NerError::Read`] when the file cannot be read and [`NerError::Verification`] when its length or hash
/// differ; a file of the wrong length is not read at all.
pub fn read_verified(path: &Path, pin: &PinnedFile) -> Result<Vec<u8>, NerError> {
    let file = std::fs::File::open(path)?;
    if file.metadata()?.len() != pin.length {
        return Err(NerError::Verification);
    }
    let capacity = usize::try_from(pin.length).map_err(|_| NerError::Verification)?;
    let mut bytes = Vec::with_capacity(capacity);
    file.take(pin.length.saturating_add(1))
        .read_to_end(&mut bytes)?;
    verify(&bytes, pin)?;
    Ok(bytes)
}

/// Checks that `bytes` have `pin`'s length and SHA-256.
///
/// # Errors
/// Returns [`NerError::Verification`] when they do not.
pub fn verify(bytes: &[u8], pin: &PinnedFile) -> Result<(), NerError> {
    if u64::try_from(bytes.len()).ok() == Some(pin.length) && sha256_hex(bytes) == pin.sha256 {
        Ok(())
    } else {
        Err(NerError::Verification)
    }
}

impl NameModel {
    /// Loads the pinned model (16-bit weights) and tokenizer from `folder`, checking both files first.
    ///
    /// # Errors
    /// Returns [`NerError::Read`] or [`NerError::Verification`] for a missing or altered file, and
    /// [`NerError::Tokenizer`] or [`NerError::Model`] when verified files cannot be used.
    pub fn load(folder: &Path) -> Result<Self, NerError> {
        Self::load_pinned(
            &folder.join(MODEL.name),
            &MODEL,
            &folder.join(TOKENIZER.name),
        )
    }

    /// Loads the weights at `model` checked against `pin`, with the pinned tokenizer at `tokenizer`. The parity check
    /// uses it to load the upstream 32-bit weights.
    ///
    /// # Errors
    /// As [`NameModel::load`].
    pub fn load_pinned(model: &Path, pin: &PinnedFile, tokenizer: &Path) -> Result<Self, NerError> {
        let span = somicus_telemetry::span!(INFO, "privacy_name_model_load");
        let result = span.in_scope(|| {
            let tokenizer = WordPiece::from_json(&read_verified(tokenizer, &TOKENIZER)?)?;
            if tokenizer.vocabulary_size() != VOCABULARY {
                return Err(NerError::Tokenizer("the vocabulary does not fit the model"));
            }
            let weights = read_verified(model, pin)?;
            let model = DistilBert::from_safetensors(&weights)?;
            Ok(Self::new(tokenizer, model))
        });
        span.record_result(&result);
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> std::path::PathBuf {
        // nosemgrep: rust.lang.security.temp-dir.temp-dir -- a test scratch directory, not a security operation
        let directory =
            std::env::temp_dir().join(format!("somicus-ner-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&directory);
        let _ = std::fs::create_dir_all(&directory);
        directory
    }

    #[test]
    fn the_hash_is_lower_case_hexadecimal() {
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn a_file_is_used_only_with_its_pinned_length_and_hash() -> Result<(), NerError> {
        let directory = scratch("verify");
        let path = directory.join("file");
        std::fs::write(&path, b"synthetic model bytes")?;
        let digest = sha256_hex(b"synthetic model bytes");
        let pin = PinnedFile {
            name: "file",
            length: 21,
            sha256: Box::leak(digest.into_boxed_str()),
        };
        assert_eq!(read_verified(&path, &pin)?, b"synthetic model bytes");
        // Same length, one byte changed.
        std::fs::write(&path, b"synthetic model bytez")?;
        assert!(matches!(
            read_verified(&path, &pin),
            Err(NerError::Verification)
        ));
        // Another length is refused before it is read.
        std::fs::write(&path, b"longer synthetic model bytes")?;
        assert!(matches!(
            read_verified(&path, &pin),
            Err(NerError::Verification)
        ));
        assert!(matches!(
            read_verified(&directory.join("missing"), &pin),
            Err(NerError::Read(std::io::ErrorKind::NotFound))
        ));
        let _ = std::fs::remove_dir_all(&directory);
        Ok(())
    }

    #[test]
    fn a_folder_without_the_model_loads_nothing() {
        let directory = scratch("empty");
        assert!(matches!(
            NameModel::load(&directory),
            Err(NerError::Read(std::io::ErrorKind::NotFound))
        ));
        let _ = std::fs::remove_dir_all(&directory);
    }
}
