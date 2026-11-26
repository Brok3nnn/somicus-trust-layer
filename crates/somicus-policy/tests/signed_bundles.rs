//! Signed policy envelopes (ADR-0052): the signature covers the transmitted bytes and is checked with a pinned key
//! before the payload is read; the format, the algorithm and the key are named; the built-in deny list is a floor; a
//! bundle this device accepted can be restored after its first-install validity ended; and the server's signer and
//! the device agree byte for byte (the golden fixture `tests/fixtures/policy-bundle-v1.json`).

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use ed25519_dalek::{Signer as _, SigningKey};
use serde::Deserialize;
use somicus_policy::envelope::{MAX_ENVELOPE_BYTES, sign_payload};
use somicus_policy::error::{BundleRejection, PolicyError};
use somicus_policy::{
    AutonomyLevel, BundleInstall, IntentVerifierHandle, PolicyBundle, PolicyGuard,
    SignedPolicyBundle, TrustedKeys, WorkspaceType, key_id,
};

const TENANT: &str = "0192f5a4-3c1e-7d2a-9b8f-1a2b3c4d5e6f";
const NOW: u64 = 1_800_000_000_000;

fn signing_key() -> SigningKey {
    SigningKey::from_bytes(&[7; 32])
}

fn trusted() -> TrustedKeys {
    TrustedKeys::new([signing_key().verifying_key()])
}

fn org_bundle(version: u32, not_after: u64) -> PolicyBundle {
    PolicyBundle {
        version,
        tenant_id: TENANT.to_owned(),
        workspace_type: WorkspaceType::Organization,
        autonomy_cap: AutonomyLevel::L2,
        not_after_epoch_ms: Some(not_after),
        confirmations_required: true,
        ..PolicyBundle::personal_do()
    }
}

fn install() -> BundleInstall<'static> {
    BundleInstall {
        device_tenant: TENANT,
        now_epoch_ms: NOW,
    }
}

#[derive(Deserialize)]
struct Golden {
    public_key: String,
    key_id: String,
    payload: String,
    envelope: SignedPolicyBundle,
}

/// An envelope with `header` as its protected header, correctly signed by `key`, so only the header is wrong.
fn with_header(header: &str, payload: &[u8], key: &SigningKey) -> SignedPolicyBundle {
    let protected = URL_SAFE_NO_PAD.encode(header);
    let payload = URL_SAFE_NO_PAD.encode(payload);
    let signature = key.sign(format!("{protected}.{payload}").as_bytes());
    SignedPolicyBundle {
        protected,
        payload,
        signature: URL_SAFE_NO_PAD.encode(signature.to_bytes()),
    }
}

#[test]
fn the_golden_envelope_verifies_and_the_device_signs_the_same_bytes() {
    let golden: Golden =
        serde_json::from_str(include_str!("fixtures/policy-bundle-v1.json")).expect("the fixture");
    let keys = TrustedKeys::parse(&golden.public_key).expect("a valid key");
    let bundle = golden
        .envelope
        .verify(&keys)
        .expect("the golden envelope verifies");
    assert_eq!(bundle.version, 3);
    assert_eq!(bundle.tenant_id, TENANT);
    assert_eq!(bundle.workspace_type, WorkspaceType::Organization);
    assert_eq!(bundle.autonomy_cap, AutonomyLevel::L2);
    assert_eq!(bundle.denied_apps, ["Example Vault"]);
    assert!(bundle.confirmations_required);
    assert_eq!(bundle.not_after_epoch_ms, Some(4_102_444_800_000));

    // Ed25519 is deterministic: the same bytes and key give the same envelope in Rust and in TypeScript.
    assert_eq!(
        sign_payload(golden.payload.as_bytes(), &signing_key()).expect("signed"),
        golden.envelope
    );
    assert_eq!(golden.key_id, key_id(&signing_key().verifying_key()));
    assert_eq!(keys.ids().collect::<Vec<_>>(), [golden.key_id.as_str()]);
}

#[test]
fn an_envelope_with_another_header_or_key_is_refused() {
    let key = signing_key();
    let keys = trusted();
    let signed = org_bundle(2, NOW + 1).sign(&key).unwrap();
    let payload = br#"{"version":2}"#;
    let kid = key_id(&key.verifying_key());

    for (header, reason) in [
        (
            format!(r#"{{"alg":"none","kid":"{kid}","typ":"somicus-policy-v1+jws"}}"#),
            "algorithm",
        ),
        (
            format!(r#"{{"alg":"EdDSA","kid":"{kid}","typ":"somicus-policy-v2+jws"}}"#),
            "type",
        ),
        (
            format!(
                r#"{{"alg":"EdDSA","kid":"{kid}","typ":"somicus-policy-v1+jws","crit":["exp"]}}"#
            ),
            "header",
        ),
        (
            r#"{"alg":"EdDSA","typ":"somicus-policy-v1+jws"}"#.to_owned(),
            "header",
        ),
        ("not json".to_owned(), "header"),
    ] {
        assert_eq!(
            with_header(&header, payload, &key).verify(&keys),
            Err(PolicyError::UnsupportedEnvelope(reason)),
            "{header}"
        );
    }

    // Signed by a key the build does not pin, even when the header claims a pinned key's ID.
    let stranger = SigningKey::from_bytes(&[8; 32]);
    let foreign = org_bundle(2, NOW + 1).sign(&stranger).unwrap();
    assert_eq!(foreign.verify(&keys), Err(PolicyError::UntrustedKey));
    let impostor = with_header(
        &format!(r#"{{"alg":"EdDSA","kid":"{kid}","typ":"somicus-policy-v1+jws"}}"#),
        payload,
        &stranger,
    );
    assert!(matches!(
        impostor.verify(&keys),
        Err(PolicyError::SignatureVerificationFailed(_))
    ));
    assert_eq!(
        signed.verify(&TrustedKeys::default()),
        Err(PolicyError::UntrustedKey)
    );

    // The header is covered by the signature: another valid header breaks it.
    let swapped = SignedPolicyBundle {
        protected: URL_SAFE_NO_PAD.encode(format!(
            r#"{{"alg":"EdDSA","typ":"somicus-policy-v1+jws","kid":"{kid}"}}"#
        )),
        ..signed.clone()
    };
    assert!(matches!(
        swapped.verify(&keys),
        Err(PolicyError::SignatureVerificationFailed(_))
    ));
    let short_signature = SignedPolicyBundle {
        signature: URL_SAFE_NO_PAD.encode([1_u8; 63]),
        ..signed
    };
    assert!(matches!(
        short_signature.verify(&keys),
        Err(PolicyError::InvalidSignatureFormat(_))
    ));
}

#[test]
fn the_signed_bytes_are_read_strictly_after_the_signature_verifies() {
    let keys = trusted();
    // A signed payload with a field this build does not know is refused, never read with the field ignored.
    let unknown_field = br#"{"version":2,"tenant_id":"t","workspace_type":"organization","autonomy_cap":"l3","allowed_apps":null,"denied_apps":[],"created_at_epoch_ms":1,"not_after_epoch_ms":2,"confirmations_required":false,"lets_everything_through":true}"#;
    assert!(matches!(
        sign_payload(unknown_field, &signing_key())
            .unwrap()
            .verify(&keys),
        Err(PolicyError::SerializationError(_))
    ));
    // The retired `confirmations_off` is no longer a field (ADR-0052 decision 2).
    let retired = br#"{"version":2,"tenant_id":"t","workspace_type":"organization","autonomy_cap":"l3","allowed_apps":null,"denied_apps":[],"created_at_epoch_ms":1,"confirmations_off":true}"#;
    assert!(matches!(
        sign_payload(retired, &signing_key()).unwrap().verify(&keys),
        Err(PolicyError::SerializationError(_))
    ));

    assert_eq!(
        SignedPolicyBundle::from_slice(&vec![b' '; MAX_ENVELOPE_BYTES + 1]),
        Err(PolicyError::UnsupportedEnvelope("too_large"))
    );
    assert_eq!(
        SignedPolicyBundle::from_slice(br#"{"protected":"e30","payload":"e30"}"#),
        Err(PolicyError::UnsupportedEnvelope("malformed"))
    );
    let stored = org_bundle(2, NOW + 1).sign(&signing_key()).unwrap();
    assert_eq!(
        SignedPolicyBundle::from_slice(&stored.to_vec().unwrap()),
        Ok(stored)
    );
}

#[test]
fn pinned_keys_must_be_strong_ed25519_public_keys() {
    let key = URL_SAFE_NO_PAD.encode(signing_key().verifying_key().as_bytes());
    let other = URL_SAFE_NO_PAD.encode(SigningKey::from_bytes(&[8; 32]).verifying_key().as_bytes());
    let keys = TrustedKeys::parse(&format!(" {key},\n{other} ")).unwrap();
    assert_eq!(keys.ids().count(), 2);
    assert!(TrustedKeys::parse("").unwrap().is_empty());
    let padded =
        base64::engine::general_purpose::STANDARD.encode(signing_key().verifying_key().as_bytes());
    for bad in [
        "not base64!",
        "AAAA",
        &URL_SAFE_NO_PAD.encode([0_u8; 32]),
        &padded,
    ] {
        assert_eq!(
            TrustedKeys::parse(bad).map(|keys| keys.ids().count()),
            Err(PolicyError::UnsupportedEnvelope("pinned_key")),
            "{bad}"
        );
    }
}

#[test]
fn every_bundle_keeps_the_built_in_deny_list() {
    let only_vaults = PolicyBundle {
        allowed_apps: Some(vec!["1password".to_owned(), "Slack".to_owned()]),
        denied_apps: Vec::new(),
        ..org_bundle(2, NOW + 1)
    };
    assert!(!only_vaults.is_app_allowed("1Password 7"));
    assert!(!only_vaults.is_app_allowed("System Settings"));
    assert!(only_vaults.is_app_allowed("Slack"));
    assert!(!only_vaults.is_app_allowed("Safari"));

    let stricter = PolicyBundle {
        denied_apps: vec!["Example Vault".to_owned()],
        ..org_bundle(2, NOW + 1)
    };
    assert!(!stricter.is_app_allowed("Example Vault"));
    assert!(!stricter.is_app_allowed("Bitwarden"));
    assert!(stricter.is_app_allowed("Safari"));
}

#[test]
fn a_stored_bundle_is_restored_after_its_first_install_validity_ended() -> Result<(), PolicyError> {
    let keys = trusted();
    let expired = org_bundle(4, NOW - 1).sign(&signing_key())?;

    let fresh =
        PolicyGuard::new_with_bundle(PolicyBundle::personal_do(), IntentVerifierHandle::new());
    assert_eq!(
        fresh.update_signed_bundle(&expired, &keys, install()),
        Err(PolicyError::BundleRejected(BundleRejection::Expired))
    );
    fresh.restore_signed_bundle(&expired, &keys, install())?;
    assert_eq!(fresh.active_bundle().version, 4);
    assert!(fresh.active_bundle().confirmations_required);

    // Restoring still checks the signature, the tenant and the version.
    let elsewhere =
        PolicyGuard::new_with_bundle(PolicyBundle::personal_do(), IntentVerifierHandle::new());
    assert_eq!(
        elsewhere.restore_signed_bundle(
            &expired,
            &keys,
            BundleInstall {
                device_tenant: "another-tenant",
                now_epoch_ms: NOW,
            }
        ),
        Err(PolicyError::BundleRejected(BundleRejection::OtherTenant))
    );
    assert_eq!(
        fresh.restore_signed_bundle(
            &org_bundle(3, NOW + 1).sign(&signing_key())?,
            &keys,
            install()
        ),
        Err(PolicyError::BundleRejected(BundleRejection::NotNewer))
    );
    assert_eq!(
        fresh.restore_signed_bundle(&expired, &TrustedKeys::default(), install()),
        Err(PolicyError::UntrustedKey)
    );
    Ok(())
}

#[test]
fn clones_share_the_active_bundle_and_reset_returns_to_the_built_in_one() -> Result<(), PolicyError>
{
    let guard =
        PolicyGuard::new_with_bundle(PolicyBundle::personal_do(), IntentVerifierHandle::new());
    let handle = guard.clone();
    guard.update_signed_bundle(
        &org_bundle(2, NOW + 1).sign(&signing_key())?,
        &trusted(),
        install(),
    )?;
    assert_eq!(handle.active_bundle().tenant_id, TENANT);
    assert_eq!(handle.active_bundle().autonomy_cap, AutonomyLevel::L2);

    handle.reset();
    assert_eq!(guard.active_bundle(), PolicyBundle::personal_do());
    Ok(())
}
