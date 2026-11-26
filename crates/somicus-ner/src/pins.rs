//! The model files this build accepts (ADR-0042), compiled into the app. Changing a pin is a security-sensitive
//! change.

/// A file the app accepts only with exactly this length and SHA-256.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PinnedFile {
    /// The file's name in the model folder.
    pub name: &'static str,
    /// Its length in bytes.
    pub length: u64,
    /// Its SHA-256, lower-case hexadecimal.
    pub sha256: &'static str,
}

/// `dslim/distilbert-NER` at commit `dfa2838a127384aabb82ed7719e16dab84c42a2a`: the upstream weights (32-bit floats),
/// Apache-2.0. Read only by the conversion tool and the parity check; the app never downloads it.
pub const UPSTREAM_MODEL: PinnedFile = PinnedFile {
    name: "model.safetensors",
    length: 260_803_668,
    sha256: "235ecd699f650ca36513b122d11ce5a80339be1388f06c33c2278368b99914e1",
};

/// The upstream weights converted to 16-bit floats by `convert::to_half_precision` (see the `convert_name_model`
/// example; same tensors, same names, in name order): the file the app downloads and loads.
pub const MODEL: PinnedFile = PinnedFile {
    name: "model.safetensors",
    length: 130_407_898,
    sha256: "2ba3be83cefc98856af65a4b846fb3c76fc7a490cb63434d7a605530bf092597",
};

/// The model's `tokenizer.json` at the same commit (BERT's cased `WordPiece`, 28,996 entries).
pub const TOKENIZER: PinnedFile = PinnedFile {
    name: "tokenizer.json",
    length: 669_021,
    sha256: "cb26b43c98e8266ae3e99c2a583cf8315d73b33a17e6b20b4df7ff1f22392d34",
};

/// The folder of this model version inside the app's model folder: `names-en-v1/<model SHA-256>`, so a new pin never
/// reads an old file.
#[must_use]
pub fn folder() -> String {
    format!("names-en-v1/{}", MODEL.sha256)
}
