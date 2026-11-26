# ADR-0052: Signed policy bundles: a JWS envelope, pinned keys and a built-in deny-list floor

- **Status:** Accepted
- **Date:** 2026-11-09

## Context

[ADR-0041](ADR-0041-policy-as-rust-rules-with-signed-bundles.md) kept device policy as Rust rules with a typed,
signed, workspace-bound and forward-only bundle. The first signature covered the device's own re-serialization of the
parsed bundle, which a signer in another language must reproduce byte for byte and which a field unknown to the device
breaks; the key and the algorithm were not named; and the built-in deny list was only a bundle field, so a signed
bundle could leave password managers out.

## Decision

1. **The envelope is a JSON Web Signature** in the flattened JSON serialization (RFC 7515 §7.2.2):
   `{"protected", "payload", "signature"}`, base64url without padding. The protected header is exactly
   `{"alg":"EdDSA","kid":…,"typ":"somicus-policy-v1+jws"}`: Ed25519 (RFC 8037), the signing key's ID, and the
   envelope's type. Any other header field, `crit` included, makes the envelope unreadable. The signature covers the
   transmitted bytes; the device checks it with a pinned key **before** it parses the payload. An envelope larger than
   64 KiB is refused.
2. **The bundle is read strictly.** A payload with a field the device does not know is refused, so a stricter setting
   added later is never silently ignored by an older app. A bundle without an end of validity is refused.
3. **The built-in deny list is a floor.** Password managers, banking and brokerage apps and system security settings
   (`BUILT_IN_DENIED_APPS`) are denied under every bundle; a bundle's deny list adds to them and its allow list cannot
   bring them back.
4. **Keys.** The server signs with one Ed25519 key per deployment stage; the key ID is the key's JWK thumbprint
   (RFC 7638), derived, never configured. The app pins the verifying keys at build time (`TrustedKeys`). Rotation: ship
   a build that pins the old and the new key, switch the server, and drop the old key from a later build.
5. **Installing.** A bundle is installed only when it is newer and for the device's own workspace. The accepted
   envelope is stored; after a restart it is verified and installed again without the first-install validity limit,
   which only stops an old captured bundle on a fresh device, so an offline device never falls back to a looser policy.

## Alternatives considered

| Option | Why not |
|---|---|
| Our own envelope format | No gain over the standard |
| Keep signing the re-serialized bundle | Fragile across languages; an unknown field breaks a valid bundle |
| COSE (CBOR) | A CBOR library on both sides; JSON is what both sides already read |
| Signed defaults for personal workspaces too | A server key would decide a person's own policy |

## Consequences

- (+) A bundle signed by the server verifies on the device byte for byte (the golden fixture
  `crates/somicus-policy/tests/fixtures/policy-bundle-v1.json`); keys can rotate without guesswork.
- (−) A new bundle setting needs a new envelope type and an app release before the server may send it.

## Threat-model note

A forged or altered bundle fails the signature check before anything in it is read; a bundle signed by another key is
refused by its ID; a header that asks for another algorithm (`none`, a MAC) or adds critical extensions is refused. An
older bundle cannot replace a newer one, a bundle for another workspace cannot be installed, and an old captured bundle
cannot be installed on a fresh device after its first-install validity. The built-in deny list and the compiled
invariants hold under every bundle.
