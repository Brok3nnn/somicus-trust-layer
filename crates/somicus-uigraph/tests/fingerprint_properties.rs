//! Property-based tests for element fingerprinting (Perception §6).

use proptest::prelude::*;
use somicus_uigraph::{
    FingerprintInput, Role, bucket_sibling_index, compute_element_fingerprint,
    normalize_name_for_fingerprint,
};

proptest! {
    #[test]
    fn names_are_normalized_regardless_of_surrounding_spaces_and_case(
        base in "[a-zA-Z]{3,15}",
        extra_spaces in " {1,5}",
    ) {
        let n1 = normalize_name_for_fingerprint(&base);
        let n2 = normalize_name_for_fingerprint(&format!("{extra_spaces}{base}{extra_spaces}"));
        let n3 = normalize_name_for_fingerprint(&base.to_uppercase());

        prop_assert_eq!(&n1, &n2);
        prop_assert_eq!(&n1, &n3);
    }

    #[test]
    fn names_differing_only_in_equally_long_digit_runs_normalize_alike(
        prefix in "[a-z]{3,8} ",
        digits1 in "[0-9]{4,6}",
        digits2 in "[0-9]{4,6}",
    ) {
        let s1 = format!("{prefix}{digits1}");
        let s2 = format!("{prefix}{digits2}");

        if digits1.len() == digits2.len() {
            prop_assert_eq!(
                normalize_name_for_fingerprint(&s1),
                normalize_name_for_fingerprint(&s2)
            );
        }
    }

    #[test]
    fn the_same_element_always_gets_the_same_fingerprint(
        app_id in "[a-z_]{3,10}",
        name in "[a-zA-Z0-9 ]{3,20}",
        sibling_idx in 0usize..50,
    ) {
        let input1 = FingerprintInput {
            app_id: &app_id,
            role: Role::Button,
            accessible_name: &name,
            stable_attrs: &["btn_save"],
            ancestor_landmarks: &[("dialog", "Save Dialog")],
            sibling_index: sibling_idx,
        };

        let input2 = FingerprintInput {
            app_id: &app_id,
            role: Role::Button,
            accessible_name: &name,
            stable_attrs: &["btn_save"],
            ancestor_landmarks: &[("dialog", "Save Dialog")],
            sibling_index: sibling_idx,
        };

        let fp1 = compute_element_fingerprint(&input1);
        let fp2 = compute_element_fingerprint(&input2);

        prop_assert_eq!(fp1, fp2);
    }

    #[test]
    fn a_different_role_changes_the_fingerprint(
        name in "[a-zA-Z]{4,12}",
    ) {
        let input_button = FingerprintInput {
            app_id: "test_app",
            role: Role::Button,
            accessible_name: &name,
            stable_attrs: &[],
            ancestor_landmarks: &[],
            sibling_index: 0,
        };

        let input_link = FingerprintInput {
            app_id: "test_app",
            role: Role::Link,
            accessible_name: &name,
            stable_attrs: &[],
            ancestor_landmarks: &[],
            sibling_index: 0,
        };

        let fp1 = compute_element_fingerprint(&input_button);
        let fp2 = compute_element_fingerprint(&input_link);

        prop_assert_ne!(fp1, fp2);
    }

    #[test]
    fn a_different_app_changes_the_fingerprint(
        name in "[a-zA-Z]{4,12}",
    ) {
        let input_app1 = FingerprintInput {
            app_id: "app_one",
            role: Role::Button,
            accessible_name: &name,
            stable_attrs: &[],
            ancestor_landmarks: &[],
            sibling_index: 0,
        };

        let input_app2 = FingerprintInput {
            app_id: "app_two",
            role: Role::Button,
            accessible_name: &name,
            stable_attrs: &[],
            ancestor_landmarks: &[],
            sibling_index: 0,
        };

        let fp1 = compute_element_fingerprint(&input_app1);
        let fp2 = compute_element_fingerprint(&input_app2);

        prop_assert_ne!(fp1, fp2);
    }
}

#[test]
fn sibling_indices_fall_into_fixed_buckets() {
    assert_eq!(bucket_sibling_index(0), 0);
    assert_eq!(bucket_sibling_index(1), 1);
    assert_eq!(bucket_sibling_index(2), 2);
    assert_eq!(bucket_sibling_index(3), 3);
    assert_eq!(bucket_sibling_index(4), 3);
    assert_eq!(bucket_sibling_index(5), 3);
    assert_eq!(bucket_sibling_index(6), 6);
    assert_eq!(bucket_sibling_index(10), 6);
    assert_eq!(bucket_sibling_index(11), 11);
    assert_eq!(bucket_sibling_index(100), 11);
}

#[test]
fn a_fingerprint_round_trips_through_hex() {
    let fp = compute_element_fingerprint(&FingerprintInput {
        app_id: "finance_app",
        role: Role::TextField,
        accessible_name: "Customer ID 1234",
        stable_attrs: &["txt_cust_id"],
        ancestor_landmarks: &[("group", "General Information")],
        sibling_index: 2,
    });

    let hex = fp.to_hex();
    let parsed = somicus_uigraph::ElementFingerprint::from_hex(&hex).expect("valid hex");
    assert_eq!(fp, parsed);
}
