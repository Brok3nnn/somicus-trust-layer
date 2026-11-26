//! Privacy Shield on-device redaction and pseudonymization engine (Privacy Shield §4).
//!
//! Encapsulates classification, entity recognition, token vault management,
//! and wrapping into [`Redacted<T>`] before network transmission.
//!
//! Overlapping matches become one token, a recognizer that cannot run redacts the whole text, window URLs keep only
//! their origin and executable paths only their file name.

use std::fmt;
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::error::PrivacyError;
use crate::name_model::NameRecognizer;
use crate::recognizers::{
    EntityType, RecognizedEntity, detect_all_entities, model_names, name_model_input,
};
use crate::redacted::Redacted;
use crate::vault::TokenVault;
use somicus_uigraph::digest::ScreenDigest;
use somicus_uigraph::fingerprint::{FingerprintInput, compute_element_fingerprint};
use somicus_uigraph::graph::UiGraph;
use somicus_uigraph::privacy::{ClassifiedText, PrivacyClass};

/// The on-device Privacy Shield engine.
///
/// Each redaction records a span with how many parts it replaced (`redactions`) and the text's class, the digest's
/// size or the graph's node count; never the text, a token or what it stands for.
#[derive(Debug, Default)]
pub struct PrivacyShield {
    vault: TokenVault,
    /// How many parts of texts this shield has replaced with tokens, for the spans' counts.
    replaced: u64,
    /// The name model and the deadline of the turn this shield serves, if the device has a verified model.
    names: Option<NameModelUse>,
}

/// A name model and how long it may take (ADR-0042 decision 7).
struct NameModelUse {
    recognizer: Arc<dyn NameRecognizer>,
    deadline: Instant,
    /// Texts that went with the rules only because the model did not scan them.
    left_to_rules: u32,
}

impl fmt::Debug for NameModelUse {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("NameModelUse")
            .field("left_to_rules", &self.left_to_rules)
            .finish_non_exhaustive()
    }
}

impl PrivacyShield {
    /// Creates a new Privacy Shield instance with a fresh token vault, recognizing names with the rules only.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Creates a Privacy Shield that also asks `recognizer` for person names (ADR-0042) until `budget` from now has
    /// passed. Texts the model has not scanned by then go with the rules only and are counted in
    /// [`names_left_to_rules`](Self::names_left_to_rules); the model can only add replacements, never remove one.
    #[must_use]
    pub fn with_name_model(recognizer: Arc<dyn NameRecognizer>, budget: Duration) -> Self {
        Self {
            names: Some(NameModelUse {
                recognizer,
                deadline: Instant::now() + budget,
                left_to_rules: 0,
            }),
            ..Self::default()
        }
    }

    /// Whether this shield asks a name model.
    #[must_use]
    pub const fn uses_name_model(&self) -> bool {
        self.names.is_some()
    }

    /// How many texts went with the rules only although this shield has a name model: the model had not scanned them
    /// before the deadline, or failed on them. A turn with any is `partial` (ADR-0042 decision 6).
    #[must_use]
    pub fn names_left_to_rules(&self) -> u32 {
        self.names.as_ref().map_or(0, |names| names.left_to_rules)
    }

    /// Asks the name model about `texts` in one batch before they are redacted one by one, so a screen costs one batched
    /// pass instead of one pass per text; the recognizer keeps the results for the redactions that follow. Without a
    /// name model, or once the deadline has passed, it does nothing.
    pub fn scan_names(&mut self, texts: &[&str]) {
        let Some(names) = self.names.as_ref() else {
            return;
        };
        if Instant::now() >= names.deadline {
            return;
        }
        let inputs: Vec<String> = texts
            .iter()
            .filter_map(|text| {
                let entities = detect_all_entities(text).ok()?;
                name_model_input(text, &entities)
            })
            .collect();
        let inputs: Vec<&str> = inputs.iter().map(String::as_str).collect();
        if !inputs.is_empty() {
            let _ = names.recognizer.find_names(&inputs, names.deadline);
        }
    }

    /// Returns a reference to the internal token vault.
    #[must_use]
    pub const fn vault(&self) -> &TokenVault {
        &self.vault
    }

    /// Returns a mutable reference to the internal token vault.
    pub fn vault_mut(&mut self) -> &mut TokenVault {
        &mut self.vault
    }

    /// Redacts a single classified text string into a [`Redacted<String>`] payload.
    pub fn redact_text(&mut self, classified: &ClassifiedText) -> Redacted<String> {
        // Debug level: a screen has hundreds of texts, and the debug-build output shows `INFO` and above.
        let span = somicus_telemetry::span!(
            DEBUG,
            "privacy_redact_text",
            kind = class_label(classified.classification()),
        );
        let before = self.replaced;
        let redacted = span.in_scope(|| self.redact_text_unobserved(classified));
        somicus_telemetry::record!(span, redactions = self.replaced.saturating_sub(before));
        redacted
    }

    fn redact_text_unobserved(&mut self, classified: &ClassifiedText) -> Redacted<String> {
        let raw = classified.raw_text();
        if raw.is_empty() {
            return Redacted::new(String::new());
        }

        match classified.classification() {
            PrivacyClass::Secret => Redacted::new(self.token(raw, EntityType::Secret)),
            PrivacyClass::Personal => match detect_all_entities(raw) {
                // The whole value is personal, for example a name.
                Ok(entities) if entities.is_empty() => {
                    Redacted::new(self.token(raw, EntityType::PersonName))
                }
                detected => {
                    let detected = detected.map(|entities| self.add_model_names(raw, entities));
                    Redacted::new(self.pseudonymize_detected(raw, detected))
                }
            },
            PrivacyClass::PublicLabel | PrivacyClass::ContentValue => {
                let pseudonymized = self.pseudonymize_string(raw);
                Redacted::new(pseudonymized)
            }
        }
    }

    /// Redacts a Screen Digest, ensuring all lines have sensitive entities pseudonymized.
    pub fn redact_digest(&mut self, digest: &ScreenDigest) -> Redacted<String> {
        let span =
            somicus_telemetry::span!(INFO, "privacy_redact_digest", bytes = digest.text.len());
        let before = self.replaced;
        let text = span.in_scope(|| self.pseudonymize_string(&digest.text));
        somicus_telemetry::record!(span, redactions = self.replaced.saturating_sub(before));
        Redacted::new(text)
    }

    /// Redacts an entire UI Graph in place, returning a branded [`Redacted<UiGraph>`].
    pub fn redact_uigraph(&mut self, graph: &UiGraph) -> Redacted<UiGraph> {
        let span =
            somicus_telemetry::span!(INFO, "privacy_redact_graph", nodes = graph.nodes.len());
        let before = self.replaced;
        let redacted = span.in_scope(|| self.redact_uigraph_unobserved(graph));
        somicus_telemetry::record!(span, redactions = self.replaced.saturating_sub(before));
        redacted
    }

    fn redact_uigraph_unobserved(&mut self, graph: &UiGraph) -> Redacted<UiGraph> {
        let mut cloned = graph.clone();

        let app_id = cloned
            .apps
            .first()
            .and_then(|a| a.app_id.as_deref())
            .unwrap_or("app");

        // 1. Redact window titles; a window's URL keeps only its origin
        for win in &mut cloned.windows {
            let redacted_title = self.redact_text(&win.title);
            win.title = ClassifiedText::personal(redacted_title.into_inner());
            win.url = win.url.as_deref().and_then(url_origin);
        }

        // 2. Redact nodes
        for node in &mut cloned.nodes {
            let mut name_changed = false;

            // Name
            let new_name = self.redact_text(&node.name).into_inner();
            if new_name != node.name.raw_text() {
                node.name = ClassifiedText::personal(new_name);
                name_changed = true;
            }

            // Description
            if let Some(desc) = &node.description {
                let new_desc = self.redact_text(desc).into_inner();
                node.description = Some(ClassifiedText::personal(new_desc));
            }

            // Value
            if let Some(val) = &node.value {
                let new_val = self.redact_text(val).into_inner();
                node.value = Some(ClassifiedText::personal(new_val));
                node.classification = PrivacyClass::Personal;
            }

            if name_changed {
                let fp_input = FingerprintInput {
                    app_id,
                    role: node.role,
                    accessible_name: node.name.raw_text(),
                    stable_attrs: &[],
                    ancestor_landmarks: &[],
                    sibling_index: 0,
                };
                node.fp = compute_element_fingerprint(&fp_input);
            }
        }

        // 3. An executable's path keeps only its file name: the folders can name the person.
        for app in &mut cloned.apps {
            let kept = app
                .exe_path
                .as_deref()
                .and_then(file_name)
                .map(|name| self.pseudonymize_string(name));
            app.exe_path = kept;
        }

        Redacted::new(cloned)
    }

    /// Rehydrates text received from the model by substituting session tokens with raw data.
    #[must_use]
    pub fn rehydrate(&self, text: &str) -> String {
        self.vault.rehydrate(text)
    }

    /// The vault's token for `raw`, counted as one replaced part.
    fn token(&mut self, raw: &str, entity_type: EntityType) -> String {
        self.replaced = self.replaced.saturating_add(1);
        self.vault.tokenize(raw, entity_type)
    }

    fn pseudonymize_string(&mut self, text: &str) -> String {
        let detected =
            detect_all_entities(text).map(|entities| self.add_model_names(text, entities));
        self.pseudonymize_detected(text, detected)
    }

    /// The rules' `entities` of `text` and the person names the name model finds in what they left; the rules' alone
    /// without a model, or when the model did not scan the text (which is then counted as left to the rules).
    fn add_model_names(
        &mut self,
        text: &str,
        mut entities: Vec<RecognizedEntity>,
    ) -> Vec<RecognizedEntity> {
        let Some(names) = self.names.as_mut() else {
            return entities;
        };
        let Some(blanked) = name_model_input(text, &entities) else {
            return entities;
        };
        match model_names(text, &blanked, names.recognizer.as_ref(), names.deadline) {
            Some(found) => {
                entities.extend(found);
                entities.sort_by_key(|entity| entity.start);
            }
            None => names.left_to_rules = names.left_to_rules.saturating_add(1),
        }
        entities
    }

    /// Replaces every detected part of `text` with its token. Overlapping matches, such as a secret that contains an
    /// e-mail address, become one part named after its most sensitive type, and the parts are replaced from the end,
    /// so every replacement uses positions in the original text. When a recognizer could not run, the whole text
    /// becomes one token rather than leaving with what that recognizer would have found.
    fn pseudonymize_detected(
        &mut self,
        text: &str,
        detected: Result<Vec<RecognizedEntity>, PrivacyError>,
    ) -> String {
        let Ok(entities) = detected else {
            return self.token(text, EntityType::Secret);
        };
        let mut result = text.to_owned();
        for span in merge_overlapping(entities).iter().rev() {
            // Spans do not overlap and run from the end, so the text before this span is still the original.
            let Some(raw) = text.get(span.start..span.end) else {
                return self.token(text, EntityType::Secret);
            };
            let token = self.token(raw, span.entity_type);
            result.replace_range(span.start..span.end, &token);
        }
        result
    }
}

/// The class's name, as the `kind` of a text's redaction span.
const fn class_label(class: PrivacyClass) -> &'static str {
    match class {
        PrivacyClass::PublicLabel => "public_label",
        PrivacyClass::ContentValue => "content_value",
        PrivacyClass::Personal => "personal",
        PrivacyClass::Secret => "secret",
    }
}

/// A part of a text that one token replaces.
struct Span {
    start: usize,
    end: usize,
    entity_type: EntityType,
}

/// How sensitive an entity type is, so a merged span is named after its most sensitive part.
const fn sensitivity(entity_type: EntityType) -> u8 {
    match entity_type {
        EntityType::Secret => 7,
        EntityType::PaymentCard => 6,
        EntityType::Iban => 5,
        EntityType::NationalId => 4,
        EntityType::Email => 3,
        EntityType::Phone => 2,
        EntityType::PersonName => 1,
        EntityType::Address => 0,
    }
}

/// Merges overlapping matches into spans that do not overlap, sorted by where they start.
fn merge_overlapping(mut entities: Vec<RecognizedEntity>) -> Vec<Span> {
    entities.sort_by_key(|entity| (entity.start, std::cmp::Reverse(entity.end)));
    let mut spans: Vec<Span> = Vec::with_capacity(entities.len());
    for entity in entities {
        match spans.last_mut() {
            Some(last) if entity.start < last.end => {
                last.end = last.end.max(entity.end);
                if sensitivity(entity.entity_type) > sensitivity(last.entity_type) {
                    last.entity_type = entity.entity_type;
                }
            }
            _ => spans.push(Span {
                start: entity.start,
                end: entity.end,
                entity_type: entity.entity_type,
            }),
        }
    }
    spans
}

/// The origin (`scheme://host[:port]`, lower-case) of an `http` or `https` URL. The user information, path, query and
/// fragment can all hold personal data or credentials and are dropped; any other URL gives `None`.
fn url_origin(url: &str) -> Option<String> {
    let (scheme, rest) = url.split_once("://")?;
    let scheme = scheme.to_ascii_lowercase();
    if scheme != "http" && scheme != "https" {
        return None;
    }
    let authority = rest.split(['/', '?', '#']).next().unwrap_or_default();
    let host = authority
        .rsplit_once('@')
        .map_or(authority, |(_, host)| host);
    is_host_and_port(host).then(|| format!("{scheme}://{}", host.to_ascii_lowercase()))
}

/// Whether `authority` is `host[:port]`: a DNS name or IPv4 address, or a bracketed IPv6 address.
fn is_host_and_port(authority: &str) -> bool {
    let (host_is_valid, port) = if let Some(bracketed) = authority.strip_prefix('[') {
        let Some((address, rest)) = bracketed.split_once(']') else {
            return false;
        };
        let valid = !address.is_empty()
            && address
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit() || matches!(byte, b':' | b'.'));
        (valid, rest)
    } else {
        let (name, rest) = authority
            .find(':')
            .map_or((authority, ""), |colon| authority.split_at(colon));
        let valid = !name.is_empty()
            && name
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-'));
        (valid, rest)
    };
    host_is_valid
        && (port.is_empty()
            || port.strip_prefix(':').is_some_and(|digits| {
                !digits.is_empty() && digits.bytes().all(|byte| byte.is_ascii_digit())
            }))
}

/// The last component of a file path, on any platform's separators, or `None` when it has none.
fn file_name(path: &str) -> Option<&str> {
    path.rsplit(['/', '\\'])
        .next()
        .filter(|name| !name.is_empty())
}

#[cfg(test)]
mod name_model_tests;
#[cfg(test)]
mod tests;
