//! Ground-truth evaluation datasets for R-suite v0 across DE, EN, and HU.
//!
//! Synthetic screens with labeled checksum identifiers and personal names.

/// Ground truth test sample for evaluation.
#[derive(Debug, Clone)]
pub struct LabeledSample {
    /// Synthetic UI string containing the entity.
    pub text: &'static str,
    /// Expected raw entity substring.
    pub expected_raw: &'static str,
    /// Whether this is a checksum-backed identifier or a personal name.
    pub is_checksum_id: bool,
    /// Language / locale tag ("DE", "EN", "HU").
    pub locale: &'static str,
}

/// Returns the labeled dataset for German (DE) screens.
#[must_use]
pub fn de_samples() -> Vec<LabeledSample> {
    vec![
        // Checksum IDs
        LabeledSample {
            text: "Kundenkonto Email: m.weber@firma.example bitte aktualisieren",
            expected_raw: "m.weber@firma.example",
            is_checksum_id: true,
            locale: "DE",
        },
        LabeledSample {
            text: "Telefonnummer für Rückfragen: +49 30 1234567 erreichbar",
            expected_raw: "+49 30 1234567",
            is_checksum_id: true,
            locale: "DE",
        },
        LabeledSample {
            text: "IBAN DE89370400440532013000 für die Überweisung",
            expected_raw: "DE89370400440532013000",
            is_checksum_id: true,
            locale: "DE",
        },
        LabeledSample {
            text: "Kreditkarte: 4532 0150 1234 5671 belastet",
            expected_raw: "4532 0150 1234 5671",
            is_checksum_id: true,
            locale: "DE",
        },
        LabeledSample {
            text: "Steuer-ID: 123 456 789 03 beim Finanzamt eingereicht",
            expected_raw: "123 456 789 03",
            is_checksum_id: true,
            locale: "DE",
        },
        // Personal Names
        LabeledSample {
            text: "Sehr geehrter Herr Alexander Müller,",
            expected_raw: "Herr Alexander Müller",
            is_checksum_id: false,
            locale: "DE",
        },
        LabeledSample {
            text: "Frau Maria Weber hat die Rechnung freigegeben",
            expected_raw: "Frau Maria Weber",
            is_checksum_id: false,
            locale: "DE",
        },
        LabeledSample {
            text: "Bearbeiter: Thomas Schmidt",
            expected_raw: "Thomas Schmidt",
            is_checksum_id: false,
            locale: "DE",
        },
        LabeledSample {
            text: "Ansprechpartner: Michael Hoffmann",
            expected_raw: "Michael Hoffmann",
            is_checksum_id: false,
            locale: "DE",
        },
        LabeledSample {
            text: "Dr. Julia Schneider wurde als Ärztin hinzugefügt",
            expected_raw: "Dr. Julia Schneider",
            is_checksum_id: false,
            locale: "DE",
        },
    ]
}

/// Returns the labeled dataset for English (EN) screens.
#[must_use]
pub fn en_samples() -> Vec<LabeledSample> {
    vec![
        // Checksum IDs
        LabeledSample {
            text: "Billing contact: alice.smith@acme.corp",
            expected_raw: "alice.smith@acme.corp",
            is_checksum_id: true,
            locale: "EN",
        },
        LabeledSample {
            text: "Call support at: +1-555-839-2041",
            expected_raw: "+1-555-839-2041",
            is_checksum_id: true,
            locale: "EN",
        },
        LabeledSample {
            text: "Wire transfer to GB29NWBK60161331926819 confirmed",
            expected_raw: "GB29NWBK60161331926819",
            is_checksum_id: true,
            locale: "EN",
        },
        LabeledSample {
            text: "Payment method: 4532 0150 1234 5671",
            expected_raw: "4532 0150 1234 5671",
            is_checksum_id: true,
            locale: "EN",
        },
        LabeledSample {
            text: "Employee SSN: 123-45-6789 on file",
            expected_raw: "123-45-6789",
            is_checksum_id: true,
            locale: "EN",
        },
        // Personal Names
        LabeledSample {
            text: "Welcome Dr. Jane Smith to the portal",
            expected_raw: "Dr. Jane Smith",
            is_checksum_id: false,
            locale: "EN",
        },
        LabeledSample {
            text: "Customer: John Johnson placed an order",
            expected_raw: "John Johnson",
            is_checksum_id: false,
            locale: "EN",
        },
        LabeledSample {
            text: "Contact: David Miller from accounts",
            expected_raw: "David Miller",
            is_checksum_id: false,
            locale: "EN",
        },
        LabeledSample {
            text: "Assigned to: Robert Brown for review",
            expected_raw: "Robert Brown",
            is_checksum_id: false,
            locale: "EN",
        },
        LabeledSample {
            text: "Ms. Patricia Williams signed the agreement",
            expected_raw: "Ms. Patricia Williams",
            is_checksum_id: false,
            locale: "EN",
        },
    ]
}

/// Returns the labeled dataset for Hungarian (HU) screens.
#[must_use]
pub fn hu_samples() -> Vec<LabeledSample> {
    vec![
        // Checksum IDs
        LabeledSample {
            text: "Kapcsolattartó email: kovacs.bela@ceg.example rögzítve",
            expected_raw: "kovacs.bela@ceg.example",
            is_checksum_id: true,
            locale: "HU",
        },
        LabeledSample {
            text: "Ügyfélszolgálat telefonszáma: +36 1 234 5678 elérhető",
            expected_raw: "+36 1 234 5678",
            is_checksum_id: true,
            locale: "HU",
        },
        LabeledSample {
            text: "Bankszámlaszám: HU56117730161111222200000000 utaláshoz",
            expected_raw: "HU56117730161111222200000000",
            is_checksum_id: true,
            locale: "HU",
        },
        LabeledSample {
            text: "Bankkártya adatok: 4532 0150 1234 5671 ellenőrizve",
            expected_raw: "4532 0150 1234 5671",
            is_checksum_id: true,
            locale: "HU",
        },
        LabeledSample {
            text: "Adóazonosító jel: 8423456781 a bevalláshoz",
            expected_raw: "8423456781",
            is_checksum_id: true,
            locale: "HU",
        },
        // Personal Names
        LabeledSample {
            text: "Tisztelt Dr. Kovács István ügyfél",
            expected_raw: "Dr. Kovács István",
            is_checksum_id: false,
            locale: "HU",
        },
        LabeledSample {
            text: "Ügyfél: Nagy Zoltán részére kiállítva",
            expected_raw: "Nagy Zoltán",
            is_checksum_id: false,
            locale: "HU",
        },
        LabeledSample {
            text: "Kapcsolattartó: Tóth Katalin elérhető",
            expected_raw: "Tóth Katalin",
            is_checksum_id: false,
            locale: "HU",
        },
        LabeledSample {
            text: "Felelős munkatárs: Varga Péter kijelölve",
            expected_raw: "Varga Péter",
            is_checksum_id: false,
            locale: "HU",
        },
        LabeledSample {
            text: "özv. Szabó Jánosné kérelme jóváhagyva",
            expected_raw: "özv. Szabó Jánosné",
            is_checksum_id: false,
            locale: "HU",
        },
    ]
}
