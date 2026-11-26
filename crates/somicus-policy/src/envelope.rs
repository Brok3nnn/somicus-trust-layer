//! The signed policy envelope (ADR-0052): what the policy server sends and the device stores, a JSON Web Signature in the
//! flattened JSON serialization (RFC 7515 §7.2.2) with `EdDSA` over Ed25519 (RFC 8037).
//!
//! ```json
//! {"protected":"<base64url of {"alg":"EdDSA","kid":"…","typ":"somicus-policy-v1+jws"}>",
//!  "payload":"<base64url of the bundle's JSON>","signature":"<base64url of 64 bytes>"}
//! ```
//!
//! The signature covers the transmitted bytes (`protected` `.` `payload`) and is checked with a pinned key before the
//! payload is parsed (ADR-0041 follow-up). The protected header names the algorithm, the type and the key: `kid` is the
//! key's JWK thumbprint (RFC 7638), so the device derives it from the key it pins and no list of IDs can drift from the
//! keys. Only the three header fields are accepted; anything else, `crit` included, makes the envelope unreadable.

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use ed25519_dalek::{Signature, Signer as _, SigningKey, VerifyingKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};

use crate::bundle::PolicyBundle;
use crate::error::PolicyError;

/// The only signature algorithm this build reads (RFC 8037).
pub const ENVELOPE_ALGORITHM: &str = "EdDSA";

/// The only envelope type this build reads; a new bundle format gets a new type.
pub const ENVELOPE_TYPE: &str = "somicus-policy-v1+jws";

/// The largest envelope the device reads, in bytes. A bundle with a few hundred app names is far below it.
pub const MAX_ENVELOPE_BYTES: usize = 64 * 1024;

/// A signed policy bundle as it travels: a flattened JSON Web Signature over the bundle's JSON bytes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SignedPolicyBundle {
    /// The protected header's JSON, base64url without padding.
    pub protected: String,
    /// The bundle's JSON bytes, base64url without padding.
    pub payload: String,
    /// The Ed25519 signature over `protected` `.` `payload`, base64url without padding.
    pub signature: String,
}

/// The protected header: the algorithm, the envelope's type and the signing key's ID, nothing else.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Header {
    alg: String,
    kid: String,
    typ: String,
}

impl SignedPolicyBundle {
    /// Reads an envelope from the bytes that arrived or were stored. Nothing in the payload is read yet.
    ///
    /// # Errors
    /// Returns `PolicyError::UnsupportedEnvelope` when the bytes are larger than [`MAX_ENVELOPE_BYTES`] or are not an
    /// envelope.
    pub fn from_slice(bytes: &[u8]) -> Result<Self, PolicyError> {
        if bytes.len() > MAX_ENVELOPE_BYTES {
            return Err(PolicyError::UnsupportedEnvelope("too_large"));
        }
        serde_json::from_slice(bytes).map_err(|_| PolicyError::UnsupportedEnvelope("malformed"))
    }

    /// The envelope's JSON bytes, to store it.
    ///
    /// # Errors
    /// Returns `PolicyError::SerializationError` if serialization fails.
    pub fn to_vec(&self) -> Result<Vec<u8>, PolicyError> {
        serde_json::to_vec(self).map_err(|e| PolicyError::SerializationError(e.to_string()))
    }

    /// Checks the signature with the trusted key the header names, then reads the bundle from the signed bytes.
    ///
    /// # Errors
    /// Returns `PolicyError::UnsupportedEnvelope` for a header that is not exactly this build's algorithm and type,
    /// `PolicyError::UntrustedKey` for a key this build does not pin, `PolicyError::InvalidSignatureFormat` for a
    /// signature that is not 64 bytes of base64url, `PolicyError::SignatureVerificationFailed` when the signature does
    /// not verify, and `PolicyError::SerializationError` when the signed bytes are not a bundle this build reads.
    pub fn verify(&self, keys: &TrustedKeys) -> Result<PolicyBundle, PolicyError> {
        let header: Header = URL_SAFE_NO_PAD
            .decode(&self.protected)
            .ok()
            .and_then(|json| serde_json::from_slice(&json).ok())
            .ok_or(PolicyError::UnsupportedEnvelope("header"))?;
        if header.alg != ENVELOPE_ALGORITHM {
            return Err(PolicyError::UnsupportedEnvelope("algorithm"));
        }
        if header.typ != ENVELOPE_TYPE {
            return Err(PolicyError::UnsupportedEnvelope("type"));
        }
        let key = keys.get(&header.kid).ok_or(PolicyError::UntrustedKey)?;
        let signature = decode_signature(&self.signature)?;
        key.verify_strict(
            signing_input(&self.protected, &self.payload).as_bytes(),
            &signature,
        )
        .map_err(|e| PolicyError::SignatureVerificationFailed(e.to_string()))?;
        let payload = URL_SAFE_NO_PAD
            .decode(&self.payload)
            .map_err(|_| PolicyError::UnsupportedEnvelope("payload"))?;
        serde_json::from_slice(&payload).map_err(|e| PolicyError::SerializationError(e.to_string()))
    }
}

impl PolicyBundle {
    /// Signs this bundle into an envelope, as the policy server does; the device itself never signs (tests do).
    ///
    /// # Errors
    /// Returns `PolicyError::SerializationError` if the bundle cannot be serialized.
    pub fn sign(&self, signing_key: &SigningKey) -> Result<SignedPolicyBundle, PolicyError> {
        let payload =
            serde_json::to_vec(self).map_err(|e| PolicyError::SerializationError(e.to_string()))?;
        sign_payload(&payload, signing_key)
    }
}

/// Signs `payload`, a bundle's JSON bytes, into an envelope whose header is `{"alg","kid","typ"}` in that order.
///
/// # Errors
/// Returns `PolicyError::SerializationError` if the header cannot be serialized.
pub fn sign_payload(
    payload: &[u8],
    signing_key: &SigningKey,
) -> Result<SignedPolicyBundle, PolicyError> {
    let header = Header {
        alg: ENVELOPE_ALGORITHM.to_owned(),
        kid: key_id(&signing_key.verifying_key()),
        typ: ENVELOPE_TYPE.to_owned(),
    };
    let header =
        serde_json::to_vec(&header).map_err(|e| PolicyError::SerializationError(e.to_string()))?;
    let protected = URL_SAFE_NO_PAD.encode(header);
    let payload = URL_SAFE_NO_PAD.encode(payload);
    let signature = signing_key.sign(signing_input(&protected, &payload).as_bytes());
    Ok(SignedPolicyBundle {
        protected,
        payload,
        signature: URL_SAFE_NO_PAD.encode(signature.to_bytes()),
    })
}

/// The JWS signing input: `protected` `.` `payload` (RFC 7515 §5.1).
fn signing_input(protected: &str, payload: &str) -> String {
    format!("{protected}.{payload}")
}

fn decode_signature(encoded: &str) -> Result<Signature, PolicyError> {
    let bytes = URL_SAFE_NO_PAD
        .decode(encoded)
        .map_err(|e| PolicyError::InvalidSignatureFormat(e.to_string()))?;
    let raw: [u8; 64] = bytes.try_into().map_err(|_| {
        PolicyError::InvalidSignatureFormat("Ed25519 signature must be exactly 64 bytes".to_owned())
    })?;
    Ok(Signature::from_bytes(&raw))
}

/// A key's ID: its JWK thumbprint (RFC 7638, RFC 8037 §2), the base64url SHA-256 of
/// `{"crv":"Ed25519","kty":"OKP","x":"<base64url public key>"}`.
#[must_use]
pub fn key_id(key: &VerifyingKey) -> String {
    let jwk = format!(
        r#"{{"crv":"Ed25519","kty":"OKP","x":"{}"}}"#,
        URL_SAFE_NO_PAD.encode(key.as_bytes())
    );
    URL_SAFE_NO_PAD.encode(Sha256::digest(jwk.as_bytes()))
}

/// The verifying keys a build pins, each with its ID.
#[derive(Debug, Clone, Default)]
pub struct TrustedKeys {
    keys: Vec<(String, VerifyingKey)>,
}

impl TrustedKeys {
    /// Trusts `keys`.
    #[must_use]
    pub fn new(keys: impl IntoIterator<Item = VerifyingKey>) -> Self {
        Self {
            keys: keys.into_iter().map(|key| (key_id(&key), key)).collect(),
        }
    }

    /// Reads the keys of a build setting: 32-byte Ed25519 public keys in base64url without padding, separated by
    /// commas, spaces or line breaks. An empty setting trusts no key.
    ///
    /// # Errors
    /// Returns `PolicyError::UnsupportedEnvelope("pinned_key")` when an entry is not a valid, strong Ed25519 public
    /// key.
    pub fn parse(setting: &str) -> Result<Self, PolicyError> {
        let mut keys = Vec::new();
        for entry in setting
            .split(|c: char| c == ',' || c.is_whitespace())
            .filter(|entry| !entry.is_empty())
        {
            let raw: [u8; 32] = URL_SAFE_NO_PAD
                .decode(entry)
                .ok()
                .and_then(|bytes| bytes.try_into().ok())
                .ok_or(PolicyError::UnsupportedEnvelope("pinned_key"))?;
            let key = VerifyingKey::from_bytes(&raw)
                .map_err(|_| PolicyError::UnsupportedEnvelope("pinned_key"))?;
            if key.is_weak() {
                return Err(PolicyError::UnsupportedEnvelope("pinned_key"));
            }
            keys.push(key);
        }
        Ok(Self::new(keys))
    }

    /// Whether no key is trusted, so no signed bundle can be installed.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.keys.is_empty()
    }

    /// The IDs of the trusted keys.
    pub fn ids(&self) -> impl Iterator<Item = &str> {
        self.keys.iter().map(|(id, _)| id.as_str())
    }

    fn get(&self, id: &str) -> Option<&VerifyingKey> {
        self.keys
            .iter()
            .find(|(known, _)| known == id)
            .map(|(_, key)| key)
    }
}
