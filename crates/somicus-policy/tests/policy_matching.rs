//! Policy Guard matching on real labels: apps by whole names, risk and field words as whole words, Delete
//! outside text as a commit, and signed bundles that only move forward for the device's own tenant.

use ed25519_dalek::SigningKey;
use somicus_policy::bundle::{PolicyBundle, WorkspaceType};
use somicus_policy::error::{BundleRejection, PolicyError};
use somicus_policy::{
    ActionContext, ActionKind, ActionTarget, AutonomyLevel, BundleInstall, IntentVerifierHandle,
    PolicyGuard, RiskClass, RiskClassifier, TrustedKeys, action_floor, key_floor,
};

#[test]
fn apps_are_matched_by_whole_names_not_by_parts_of_names() {
    let bundle = PolicyBundle::default_personal();
    for denied in [
        "Wise",
        "1Password",
        "1Password 7",
        "Bitwarden",
        "KeePassXC",
        "System Settings",
        "OTP Banking",
        "com.agilebits.onepassword",
        "com.bitwarden.desktop",
    ] {
        assert!(!bundle.is_app_allowed(denied), "{denied} must be denied");
    }
    for allowed in ["Likewise", "Safari", "Calendar", "Swiss Bank Notes"] {
        assert!(bundle.is_app_allowed(allowed), "{allowed} must be allowed");
    }

    let only_slack = PolicyBundle {
        allowed_apps: Some(vec!["slack".to_owned()]),
        ..PolicyBundle::default_personal()
    };
    assert!(only_slack.is_app_allowed("Slack"));
    assert!(!only_slack.is_app_allowed("Slackware Notes"));
}

#[test]
fn risk_words_match_whole_words_only() {
    let classifier = RiskClassifier::new();
    let class = |label: &str| classifier.classify_action(&ActionContext::new(label, "button"));
    for label in [
        "Dropdown",
        "Border",
        "Swipe left",
        "Display",
        "Deleted items",
    ] {
        assert_eq!(class(label), RiskClass::R1, "{label}");
    }
    for label in [
        "Delete",
        "Delete…",
        "Place order",
        "Pay now",
        "Send message",
        "Save",
        "Confirm",
        "Archive",
    ] {
        assert_eq!(class(label), RiskClass::R3, "{label}");
    }
    for label in ["Drop database", "Wipe", "Delete account"] {
        assert_eq!(class(label), RiskClass::R4, "{label}");
    }
}

#[test]
fn return_navigates_only_in_a_search_or_address_bar() {
    let floor = |label: &str, role: &str| {
        action_floor(ActionKind::PressEnter, &ActionTarget::new(label, role))
    };
    assert_eq!(floor("Search mail", "text_field"), RiskClass::R1);
    assert_eq!(floor("Address and search bar", "text_field"), RiskClass::R1);
    assert_eq!(floor("Website URL", "text_field"), RiskClass::R1);
    assert_eq!(floor("Anything", "search_field"), RiskClass::R1);
    // A recipient or postal address field submits a form, and "search" inside a word is not a search field.
    assert_eq!(floor("Email address", "text_field"), RiskClass::R3);
    assert_eq!(floor("Shipping address", "text_field"), RiskClass::R3);
    assert_eq!(floor("Research notes", "text_area"), RiskClass::R3);
}

#[test]
fn delete_is_a_commit_outside_text() {
    assert_eq!(
        key_floor("delete", &ActionTarget::new("Notes", "text_area")),
        RiskClass::R2
    );
    assert_eq!(
        key_floor("delete", &ActionTarget::new("Search", "search_field")),
        RiskClass::R2
    );
    assert_eq!(
        key_floor("delete", &ActionTarget::new("Report.pdf", "list_item")),
        RiskClass::R3
    );
    assert_eq!(
        key_floor("delete", &ActionTarget::new("Invoice 1023", "row")),
        RiskClass::R3
    );
}

#[test]
fn a_signed_bundle_installs_only_when_newer_unexpired_and_for_this_tenant()
-> Result<(), PolicyError> {
    let key = SigningKey::from_bytes(&[7; 32]);
    let keys = TrustedKeys::new([key.verifying_key()]);
    let now = 1_800_000_000_000;
    let org = |version: u32, not_after: Option<u64>| PolicyBundle {
        version,
        tenant_id: "tenant-alpha".to_owned(),
        workspace_type: WorkspaceType::Organization,
        autonomy_cap: AutonomyLevel::L2,
        not_after_epoch_ms: not_after,
        ..PolicyBundle::default_personal()
    };
    let guard = PolicyGuard::new_personal(IntentVerifierHandle::new());
    let install = BundleInstall {
        device_tenant: "tenant-alpha",
        now_epoch_ms: now,
    };

    let second = org(2, Some(now + 60_000)).sign(&key)?;
    guard.update_signed_bundle(&second, &keys, install)?;
    assert_eq!(guard.active_bundle().version, 2);

    let refused = [
        (org(1, Some(now + 60_000)), BundleRejection::NotNewer),
        (org(2, Some(now + 60_000)), BundleRejection::NotNewer),
        (
            PolicyBundle {
                tenant_id: "tenant-beta".to_owned(),
                ..org(3, Some(now + 60_000))
            },
            BundleRejection::OtherTenant,
        ),
        (org(3, Some(now - 1)), BundleRejection::Expired),
        (org(3, None), BundleRejection::NoExpiry),
    ];
    for (bundle, rejection) in refused {
        let signed = bundle.sign(&key)?;
        assert_eq!(
            guard.update_signed_bundle(&signed, &keys, install),
            Err(PolicyError::BundleRejected(rejection))
        );
    }
    assert_eq!(guard.active_bundle().version, 2);
    Ok(())
}

#[test]
fn risk_is_classified_in_every_language_and_a_model_can_only_raise_it() {
    let classifier = RiskClassifier::new();

    // Multilingual Commit (R3)
    assert_eq!(
        classifier.classify_action(&ActionContext::new("Delete contact", "button")),
        RiskClass::R3
    );
    // The German and Hungarian labels stay: defensive data for third-party screen content.
    assert_eq!(
        classifier.classify_action(&ActionContext::new("Kontakt löschen", "button")),
        RiskClass::R3
    );
    assert_eq!(
        classifier.classify_action(&ActionContext::new("Elem törlése", "button")),
        RiskClass::R3
    );
    assert_eq!(
        classifier.classify_action(&ActionContext::new("Supprimer le profil", "button")),
        RiskClass::R3
    );
    assert_eq!(
        classifier.classify_action(&ActionContext::new("Borrar fila", "button")),
        RiskClass::R3
    );

    // Multilingual Critical (R4)
    assert_eq!(
        classifier.classify_action(&ActionContext::new("Delete account", "button")),
        RiskClass::R4
    );
    // The German and Hungarian labels stay: defensive data for third-party screen content.
    assert_eq!(
        classifier.classify_action(&ActionContext::new("Konto löschen", "button")),
        RiskClass::R4
    );
    assert_eq!(
        classifier.classify_action(&ActionContext::new("Fiók törlése", "button")),
        RiskClass::R4
    );

    // Bulk destruction escalation
    assert_eq!(
        classifier.classify_action(
            &ActionContext::new("Remove item", "button")
                .with_destructive(true)
                .with_bulk(true)
        ),
        RiskClass::R4
    );

    // Edit (R2)
    assert_eq!(
        classifier.classify_action(&ActionContext::new("Edit name", "textbox")),
        RiskClass::R2
    );
    // The Hungarian label stays: defensive data for third-party screen content.
    assert_eq!(
        classifier.classify_action(&ActionContext::new("Név szerkesztése", "textbox")),
        RiskClass::R2
    );

    // Conservative model bias: model can only raise, never lower
    assert_eq!(
        classifier.combine_with_model(RiskClass::R2, RiskClass::R3),
        RiskClass::R3
    );
    assert_eq!(
        classifier.combine_with_model(RiskClass::R3, RiskClass::R1),
        RiskClass::R3
    );
    assert_eq!(
        classifier.combine_with_model(RiskClass::R4, RiskClass::R2),
        RiskClass::R4
    );
}
