//! Known numbers and checked calculations.

use proptest::prelude::*;

use super::*;

#[test]
fn a_number_is_known_whole_or_by_its_digit_groups() {
    let known = KnownNumbers::from_text("1. | Northport | 1 686 222");
    assert!(known.allow("Northport: 1686222 people"));
    assert!(known.allow("Northport: 1 686 222 people"));
    assert!(known.allow("Northport: 1,686,222 people"));
    assert!(!known.allow("Northport: 1 700 000 people"));
    assert!(known.allow("No number here."));
}

#[test]
fn a_spreadsheet_formula_may_use_whole_numbers_up_to_a_thousand() {
    // A finance homework task: 12 months and 48 or 120 periods are no invented figures.
    let known = KnownNumbers::from_text("A loan of 12000 at 6% a year for 4 years");
    assert!(known.allow("=PMT(6%/12,48,-12000)"));
    assert!(known.allow(" =FV(5%/12,120,-200)"));
    // Outside a formula, and above a thousand or with a decimal part inside one, a number must still be known.
    assert!(!known.allow("The payment is 48 a month"));
    assert!(!known.allow("=12000*1200"));
    assert!(!known.allow("=12000*1.07"));
}

#[test]
fn a_calculation_of_known_numbers_adds_its_result() {
    let mut known = KnownNumbers::from_text("Northport 1 686 222, Eastvale 199 520");
    assert!(known.add_calculation("1686222 + 199520"));
    assert!(known.allow("Total: 1 885 742"));
    assert!(known.add_calculation("(1 686 222 + 199 520) / 2"));
    assert!(known.allow("Average: 942871"));
}

#[test]
fn a_calculation_with_an_unknown_number_or_anything_else_is_refused() {
    let mut known = KnownNumbers::from_text("12400");
    assert!(!known.add_calculation("12400 * 999"));
    assert!(known.add_calculation("12400 * 2"));
    assert!(!known.add_calculation("12400 / 0"));
    assert!(!known.add_calculation("process.exit(12400)"));
    assert!(!known.add_calculation("((((((((((12400))))))))))"));
    assert!(!known.allow("9999"));
}

#[test]
fn percentages_and_decimals_round_to_the_forms_a_text_writes() {
    let mut known = KnownNumbers::from_text("15 12400");
    assert!(known.add_calculation("12400 * 15 / 100"));
    assert!(known.allow("The tip is 1860."));
    let mut shares = KnownNumbers::from_text("1 3");
    assert!(shares.add_calculation("1 / 3"));
    assert!(shares.allow("The share is 0.33."));
}

#[test]
fn evaluates_with_the_usual_precedence() {
    let known = KnownNumbers::from_text("2 3 4 10");
    assert_eq!(evaluate("2 + 3 * 4", &known), Some(14.0));
    assert_eq!(evaluate("(2 + 3) * 4", &known), Some(20.0));
    assert_eq!(evaluate("-2 + 10 % 3", &known), Some(-1.0));
    assert_eq!(evaluate("10 - 4 - 3", &known), Some(3.0));
}

proptest! {
    #[test]
    fn a_number_never_seen_is_never_allowed(seen in 0_u32..100_000, unseen in 100_000_u32..1_000_000) {
        let known = KnownNumbers::from_text(&seen.to_string());
        prop_assert!(known.allow(&seen.to_string()));
        let text = ["about ", &unseen.to_string(), " people"].concat();
        prop_assert!(!known.allow(&text));
    }

    #[test]
    fn evaluation_never_panics(text in "\\PC{0,60}") {
        let known = KnownNumbers::from_text(&text);
        let _ = evaluate(&text, &known);
    }
}
