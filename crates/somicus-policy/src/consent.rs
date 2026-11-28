//! The person's choice for consent questions (ADR-0056): which buttons answer a cookie banner by that choice. The
//! device performs such a click unseen, in every mode, only when every word of the label is in a closed vocabulary of
//! banner buttons and the label makes the person's choice, and the button sits in a banner that names cookies; a bare
//! verb ("Accept", "I agree") also needs a banner that names no terms, so a sign-up's terms button never runs unseen.
//! The agent runtime reads labels alike and only marks the click; `consent/labels.json` holds the labels
//! both must read the same way.

/// The person's choice for cookie banners.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BannerChoice {
    /// Only the necessary cookies: the default.
    NecessaryOnly,
    /// Every cookie.
    AcceptAll,
}

/// What the banner around a button names besides cookies.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Banner {
    /// Whether it also names terms, conditions or an agreement.
    pub names_terms: bool,
}

/// What a word of a banner's button says.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Word {
    Reject,
    Negation,
    Accept,
    All,
    Only,
    Essential,
    Optional,
    Cookie,
    Filler,
}

/// The closed vocabulary of cookie banner buttons in English, German, Hungarian, French, Spanish, Italian and Dutch,
/// the same as the agent runtime's. A word ending in `*` matches every word that begins with it; any word
/// outside it makes a label no banner button at all, so "Accept all changes" or "Decline meeting" never count.
/// `consent/labels.json` holds the labels both must read alike.
const VOCABULARY: &[(Word, &[&str])] = &[
    (
        Word::Reject,
        &[
            "reject*",
            "decline*",
            "deny",
            "refus*",
            "ablehn*",
            "verweiger*",
            "elutasít*",
            "rechaz*",
            "rifiut*",
            "weiger*",
        ],
    ),
    (
        Word::Negation,
        &[
            "not", "no", "don", "without", "nicht", "ohne", "kein*", "nem", "nélkül", "sans",
            "pas", "sin", "senza", "zonder", "niet",
        ],
    ),
    (
        Word::Accept,
        &[
            "accept*",
            "allow*",
            "agree*",
            "consent*",
            "akzeptier*",
            "annehm*",
            "zulass*",
            "zustimm*",
            "erlaub*",
            "einverstanden",
            "einwillig*",
            "elfogad*",
            "fogad*",
            "engedélyez*",
            "egyetért*",
            "hozzájárul*",
            "autoris*",
            "acept*",
            "permit*",
            "accett*",
            "toestaan",
            "akkoord",
        ],
    ),
    (
        Word::All,
        &[
            "all", "alle", "allen", "aller", "alles", "összes", "mind", "minden", "mindet", "tout",
            "tous", "toutes", "todo", "todos", "todas", "tutto", "tutti", "tutte",
        ],
    ),
    (
        Word::Only,
        &[
            "only",
            "just",
            "nur",
            "lediglich",
            "csak",
            "kizárólag",
            "seulement",
            "uniquement",
            "solo",
            "sólo",
            "solamente",
            "soltanto",
            "alleen",
        ],
    ),
    (
        Word::Essential,
        &[
            "necessary",
            "essential",
            "required",
            "strictly",
            "notwendig*",
            "erforderlich*",
            "essenziell*",
            "essentiell*",
            "technisch*",
            "szükséges*",
            "alapvető*",
            "nélkülözhetetlen*",
            "elengedhetetlen*",
            "feltétlenül",
            "nécessaire*",
            "essentiel*",
            "strictement",
            "necesari*",
            "esencial*",
            "imprescindible*",
            "necessari*",
            "essenzial*",
            "tecnic*",
            "noodzakelijk*",
            "essentie*",
        ],
    ),
    (
        Word::Optional,
        &[
            "optional*",
            "non",
            "additional",
            "other",
            "zusätzlich*",
            "opcionális",
            "további",
            "optionnel*",
            "opcional*",
            "opzional*",
            "facoltativ*",
            "overige",
            "optione*",
        ],
    ),
    (Word::Cookie, &["cookie*", "süti*", "traceur*"]),
    (
        Word::Filler,
        &[
            "the",
            "a",
            "an",
            "and",
            "my",
            "me",
            "i",
            "j",
            "t",
            "use",
            "to",
            "for",
            "close",
            "continu*",
            "proceed",
            "die",
            "der",
            "den",
            "das",
            "und",
            "ich",
            "weiter",
            "schließen",
            "verwenden",
            "az",
            "és",
            "k",
            "el",
            "bezár*",
            "folytat*",
            "les",
            "le",
            "la",
            "l",
            "de",
            "des",
            "du",
            "et",
            "je",
            "fermer",
            "los",
            "las",
            "y",
            "e",
            "il",
            "lo",
            "gli",
            "di",
            "cerrar",
            "chiudi",
            "en",
            "het",
            "sluiten",
            "doorgaan",
        ],
    ),
];

/// The longest label a banner's button has; a longer one is a sentence, not a choice.
const MAX_LABEL_WORDS: usize = 8;

/// How a banner's button answers it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ButtonChoice {
    /// Which choice the button makes.
    pub choice: BannerChoice,
    /// Whether it is a bare verb ("Accept", "I agree", "Decline"), which a sign-up's terms use as well.
    pub bare: bool,
}

/// What one word says, or `None` for a word outside the vocabulary.
fn meaning(word: &str) -> Option<Word> {
    VOCABULARY.iter().find_map(|(kind, entries)| {
        entries
            .iter()
            .any(|entry| {
                entry
                    .strip_suffix('*')
                    .map_or(word == *entry, |stem| word.starts_with(stem))
            })
            .then_some(*kind)
    })
}

/// The kinds of the words of `label`, or `None` when it is too long or has a word outside the vocabulary.
fn kinds_of(label: &str) -> Option<Vec<Word>> {
    let label = label.replace('&', " and ");
    let all: Vec<String> = words(&label).collect();
    if all.is_empty() || all.len() > MAX_LABEL_WORDS {
        return None;
    }
    all.iter().map(|word| meaning(word)).collect()
}

/// How a button labelled `label` answers a cookie banner, read word by word: rejecting, keeping only what is needed
/// or not accepting keeps the necessary cookies; accepting without a limit accepts all; anything else is `None`.
#[must_use]
pub fn button_choice(label: &str) -> Option<ButtonChoice> {
    let kinds = kinds_of(label)?;
    let has = |kind: Word| kinds.contains(&kind);
    let limited = has(Word::Only) || has(Word::Essential) || has(Word::Optional);
    let bare = !has(Word::All) && !has(Word::Cookie) && !limited;
    let keeps_necessary = has(Word::Reject)
        || (has(Word::Only) && has(Word::Essential))
        || (has(Word::Negation) && has(Word::Accept) && !has(Word::All));
    let choice = if keeps_necessary {
        BannerChoice::NecessaryOnly
    } else if has(Word::Accept) && !has(Word::Negation) && !limited {
        BannerChoice::AcceptAll
    } else {
        return None;
    };
    Some(ButtonChoice { choice, bare })
}

/// Word beginnings that name cookies or consent.
const COOKIE_WORDS: &[&str] = &[
    "cookie",
    "süti",
    "consent",
    "consens",
    "einwillig",
    "hozzájárul",
    "gdpr",
    "dsgvo",
    "rgpd",
];

/// Whole words that name terms.
const TERMS_WORDS: &[&str] = &["terms", "term", "tos", "agb", "termini"];

/// Word parts that name conditions or an agreement.
const TERMS_PARTS: &[&str] = &[
    "condition",
    "agreement",
    "bedingung",
    "feltétel",
    "términos",
    "condicion",
    "condizion",
];

/// Whether a button labelled `label` answers a cookie banner by `choice`, inside a banner that names cookies and, as
/// `banner` says, perhaps terms: a bare verb answers only a banner that names no terms.
#[must_use]
pub fn answers_banner(label: &str, choice: BannerChoice, banner: Banner) -> bool {
    button_choice(label)
        .is_some_and(|button| button.choice == choice && !(button.bare && banner.names_terms))
}

/// The words of `text`, lower case.
fn words(text: &str) -> impl Iterator<Item = String> + '_ {
    text.split(|character: char| !character.is_alphanumeric())
        .filter(|word| !word.is_empty())
        .map(str::to_lowercase)
}

/// Whether `text` names cookies or consent.
#[must_use]
pub fn names_cookies(text: &str) -> bool {
    words(text).any(|word| {
        COOKIE_WORDS
            .iter()
            .any(|beginning| word.starts_with(beginning))
    })
}

/// Whether `text` names terms, conditions or an agreement.
#[must_use]
pub fn names_terms(text: &str) -> bool {
    words(text).any(|word| {
        TERMS_WORDS.contains(&word.as_str()) || TERMS_PARTS.iter().any(|part| word.contains(part))
    })
}

#[cfg(test)]
mod tests;
