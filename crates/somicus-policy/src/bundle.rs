//! Policy bundles and the personal workspace defaults (ADR-0041, ADR-0052).
//!
//! A bundle is typed data the Policy Guard reads: the autonomy cap, the app allow and deny lists, whether confirmations
//! are required, and its version, tenant and validity. Bundles reach the device signed (see [`crate::envelope`]). The
//! built-in deny list is a floor: every bundle denies those apps, whatever its own lists say.

use serde::{Deserialize, Serialize};

use crate::risk::AutonomyLevel;

/// Apps every bundle denies (ADR-0052 decision 3): password managers, banking and brokerage apps, and system security
/// settings. A bundle's own deny list adds to it; nothing removes an entry.
pub const BUILT_IN_DENIED_APPS: &[&str] = &[
    // Password managers
    "1password",
    "com.agilebits.onepassword",
    "bitwarden",
    "com.bitwarden.desktop",
    "keepass",
    "keepassxc",
    "lastpass",
    "dashlane",
    // Banking & Brokerage
    "revolut",
    "wise",
    "schwab",
    "fidelity",
    "banking",
    // System security settings
    "systemsettings",
    "systempreferences",
    "controlpanel",
    "regedit",
    "secpol",
];

/// Workspace type distinction per data model and interaction model.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkspaceType {
    /// Personal workspace (auto-created on signup, capped at L1 in MVP).
    Personal,
    /// Organization tenant workspace (admin policies, higher autonomy possible).
    Organization,
}

/// A declarative workspace policy bundle evaluated locally in the Rust core. A field this build does not know makes a
/// signed bundle unreadable, so a newer, stricter setting is never silently ignored.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PolicyBundle {
    /// The bundle's version: an organisation's settings revision. A device installs only a higher one.
    pub version: u32,
    /// Tenant identifier or "personal".
    pub tenant_id: String,
    /// Workspace classification.
    pub workspace_type: WorkspaceType,
    /// Maximum allowed autonomy cap.
    pub autonomy_cap: AutonomyLevel,
    /// Optional allowlist of permitted app IDs or bundle identifiers.
    /// If None, all apps are permitted except those on the `denied_apps` list.
    pub allowed_apps: Option<Vec<String>>,
    /// Apps the workspace denies, on top of [`BUILT_IN_DENIED_APPS`].
    pub denied_apps: Vec<String>,
    /// Creation timestamp in milliseconds since UNIX epoch.
    pub created_at_epoch_ms: u64,
    /// When the bundle stops being valid for a first install, in milliseconds since the UNIX epoch. A signed bundle
    /// must carry it, so an old captured bundle cannot be installed on a fresh device. The built-in bundles
    /// have none.
    #[serde(default)]
    pub not_after_epoch_ms: Option<u64>,
    /// When true, the organisation forbids the person's switch: R3 and R4 steps wait for a confirmation verified as
    /// physical input even when the person turned confirmations off in the settings (ADR-0040).
    #[serde(default)]
    pub confirmations_required: bool,
}

impl PolicyBundle {
    /// Creates the default signed policy bundle for personal workspaces.
    ///
    /// Per Interaction Model §5:
    /// "Personal workspaces have no admin. They run on the signed default policy bundle...
    /// all apps enabled except a built-in deny list (password managers, banking/brokerage apps,
    /// system security settings), maximum autonomy L1 in the MVP." The deny list is [`BUILT_IN_DENIED_APPS`], which
    /// every bundle applies.
    #[must_use]
    pub fn default_personal() -> Self {
        Self {
            version: 1,
            tenant_id: "personal".to_owned(),
            workspace_type: WorkspaceType::Personal,
            autonomy_cap: AutonomyLevel::L1,
            allowed_apps: None,
            denied_apps: Vec::new(),
            created_at_epoch_ms: 1_700_000_000_000,
            not_after_epoch_ms: None,
            confirmations_required: false,
        }
    }

    /// The personal default with the autonomy cap raised to L3, the level Do mode needs (ADR-0020, ADR-0040).
    /// Somicus performs R0–R2 steps itself; R3 and R4 steps wait for a confirmation verified as physical input unless
    /// the person switched confirmations off in the settings. R5 and the deny list always apply. Debug and release
    /// desktop builds both install it.
    #[must_use]
    pub fn personal_do() -> Self {
        Self {
            autonomy_cap: AutonomyLevel::L3,
            ..Self::default_personal()
        }
    }

    /// Checks whether a given application name or identifier is permitted by this policy bundle: not on
    /// [`BUILT_IN_DENIED_APPS`] or the bundle's deny list, and on its allow list when it has one. Entries name apps by
    /// whole names, never by parts of a name (see [`names_app`]).
    #[must_use]
    pub fn is_app_allowed(&self, app_id: &str) -> bool {
        if BUILT_IN_DENIED_APPS
            .iter()
            .copied()
            .chain(self.denied_apps.iter().map(String::as_str))
            .any(|entry| names_app(entry, app_id))
        {
            return false;
        }
        self.allowed_apps
            .as_ref()
            .is_none_or(|allowed| allowed.iter().any(|entry| names_app(entry, app_id)))
    }
}

/// Whether the policy list entry `entry` names the app `app` (a name such as `1Password 7` or an identifier such as
/// `com.agilebits.onepassword`). It does when the whole name equals the entry, when the name without its separators
/// does (`System Settings` is `systemsettings`), when one of its words does, ignoring a version number at the end of
/// the word (`1Password 7` and `1Password7` are `1password`), or, for an entry that is a bundle identifier, when the
/// app's identifier is that one or one inside it. Part of a word never matches, so `wise` does not name `Likewise`.
fn names_app(entry: &str, app: &str) -> bool {
    let entry = entry.trim().to_lowercase();
    let app = app.trim().to_lowercase();
    if entry.is_empty() || app.is_empty() {
        return false;
    }
    if app == entry {
        return true;
    }
    if entry.contains('.') {
        return app.starts_with(&format!("{entry}."));
    }
    let compact: String = app.chars().filter(|c| c.is_alphanumeric()).collect();
    compact == entry
        || app.split(|c: char| !c.is_alphanumeric()).any(|word| {
            word.strip_prefix(entry.as_str())
                .is_some_and(|rest| rest.chars().all(|c| c.is_ascii_digit()))
        })
}
