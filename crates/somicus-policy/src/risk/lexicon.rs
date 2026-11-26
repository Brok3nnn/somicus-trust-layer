//! The multilingual label lexicon the risk classifier matches as whole words: critical, commit, edit and navigate
//! words in English, German, Hungarian, French and Spanish.

/// Whether `keyword` occurs in `term` as whole words, not inside a longer word: `drop` is not in `dropdown`, `order`
/// not in `border` and `wipe` not in `swipe`.
fn has_words(term: &str, keyword: &str) -> bool {
    term.match_indices(keyword).any(|(start, found)| {
        let before = term[..start].chars().next_back();
        let after = term[start + found.len()..].chars().next();
        !before.is_some_and(char::is_alphanumeric) && !after.is_some_and(char::is_alphanumeric)
    })
}

pub(super) fn is_critical_keyword(term: &str) -> bool {
    const CRITICAL_KEYWORDS: &[&str] = &[
        // English
        "delete account",
        "erase all",
        "factory reset",
        "drop database",
        "wipe",
        "format disk",
        "terminate contract",
        // German
        "konto löschen",
        "alles löschen",
        "werkseinstellungen",
        "datenbank löschen",
        // Hungarian
        "fiók törlése",
        "összes törlése",
        "gyári visszaállítás",
        "adatbázis törlése",
        // French
        "supprimer le compte",
        "tout effacer",
        "réinitialisation",
        // Spanish
        "eliminar cuenta",
        "borrar todo",
        "restablecimiento",
    ];

    CRITICAL_KEYWORDS.iter().any(|&kw| has_words(term, kw))
}

pub(super) fn is_commit_keyword(term: &str) -> bool {
    const COMMIT_KEYWORDS: &[&str] = &[
        // English
        "delete",
        "remove",
        "drop",
        "destroy",
        "send",
        "post",
        "submit",
        "pay",
        "checkout",
        "transfer",
        "approve",
        "authorize",
        "purchase",
        "buy",
        "order",
        "publish",
        "save",
        "confirm",
        "archive",
        // German
        "löschen",
        "entfernen",
        "senden",
        "abschicken",
        "buchen",
        "zahlen",
        "überweisen",
        "genehmigen",
        "freigeben",
        "kaufen",
        "bestellen",
        "veröffentlichen",
        // Hungarian
        "törlés",
        "törlése",
        "eltávolítás",
        "küldés",
        "könyvelés",
        "fizetés",
        "utalás",
        "jóváhagyás",
        "engedélyezés",
        "vásárlás",
        "megrendelés",
        "közzététel",
        // French
        "supprimer",
        "effacer",
        "envoyer",
        "poster",
        "payer",
        "virer",
        "approuver",
        "autoriser",
        "acheter",
        "commander",
        "publier",
        // Spanish
        "borrar",
        "eliminar",
        "enviar",
        "publicar",
        "pagar",
        "transferir",
        "aprobar",
        "autorizar",
        "comprar",
        "ordenar",
    ];

    COMMIT_KEYWORDS.iter().any(|&kw| has_words(term, kw))
}

pub(super) fn is_edit_keyword(term: &str) -> bool {
    const EDIT_KEYWORDS: &[&str] = &[
        // English
        "edit",
        "type",
        "input",
        "write",
        "fill",
        "change",
        "select",
        // German
        "bearbeiten",
        "eingeben",
        "schreiben",
        "ausfüllen",
        "ändern",
        "auswählen",
        // Hungarian
        "szerkesztés",
        "beírás",
        "kitöltés",
        "módosítás",
        "kiválasztás",
        // French
        "modifier",
        "saisir",
        "écrire",
        "remplir",
        "changer",
        "sélectionner",
        // Spanish
        "editar",
        "escribir",
        "rellenar",
        "cambiar",
        "seleccionar",
    ];

    EDIT_KEYWORDS.iter().any(|&kw| has_words(term, kw))
}

pub(super) fn is_navigate_keyword(term: &str) -> bool {
    const NAVIGATE_KEYWORDS: &[&str] = &[
        // English
        "open",
        "view",
        "goto",
        "tab",
        "next",
        "previous",
        "back",
        "forward",
        "expand",
        "collapse",
        // German
        "öffnen",
        "ansicht",
        "weiter",
        "zurück",
        "erweitern",
        "zuklappen",
        // Hungarian
        "megnyitás",
        "nézet",
        "következő",
        "vissza",
        "kibontás",
        "összecsukás",
        // French
        "ouvrir",
        "afficher",
        "suivant",
        "précédent",
        "développer",
        "réduire",
        // Spanish
        "abrir",
        "ver",
        "siguiente",
        "anterior",
        "expandir",
        "contraer",
    ];

    NAVIGATE_KEYWORDS.iter().any(|&kw| has_words(term, kw))
}
