//! Where the server URL and the bearer token live.
//!
//! Deliberately not in the local database and never in the webview: the token is
//! the one secret on this machine, so it stays in a single file that only Rust
//! reads. The webview is only ever told *whether* there is a token.

use anyhow::Context;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Narrowest the opacity slider goes. Below this the text stops being readable
/// over a bright background, and the window becomes hard to find at all.
pub const MIN_OPACITY: f64 = 0.3;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    /// Origin of the NAS server, e.g. `http://notes.example.com:8080`.
    #[serde(default)]
    pub server_url: String,
    #[serde(default)]
    pub token: Option<String>,
    /// The note shown when the floating window starts. This is a local desktop
    /// preference, not part of the synchronized note itself.
    #[serde(default)]
    pub spotlight_note_id: Option<String>,
    /// Window opacity. Persisted because a setting that silently resets on the
    /// next launch is worse than no setting at all.
    #[serde(default = "full_opacity")]
    pub opacity: f64,
    /// Whether the window floats above everything else. On by default — that is
    /// the point of the app — but occasionally in the way.
    #[serde(default = "enabled")]
    pub always_on_top: bool,
}

fn full_opacity() -> f64 {
    1.0
}

fn enabled() -> bool {
    true
}

/// Hand-written rather than derived: `#[derive(Default)]` would make a fresh
/// install fully transparent and not on top.
impl Default for Settings {
    fn default() -> Self {
        Self {
            server_url: String::new(),
            token: None,
            spotlight_note_id: None,
            opacity: full_opacity(),
            always_on_top: enabled(),
        }
    }
}

impl Settings {
    pub fn is_configured(&self) -> bool {
        !self.server_url.is_empty() && self.token.is_some()
    }
}

fn file(app_data: &Path) -> PathBuf {
    app_data.join("settings.json")
}

/// A missing or unreadable file yields defaults rather than an error: a first run
/// and a corrupted file should both land the user on the login screen.
pub fn load(app_data: &Path) -> Settings {
    let path = file(app_data);
    match std::fs::read_to_string(&path) {
        Ok(raw) => serde_json::from_str(&raw).unwrap_or_else(|err| {
            tracing::warn!(%err, path = %path.display(), "settings unreadable, starting fresh");
            Settings::default()
        }),
        Err(_) => Settings::default(),
    }
}

pub fn save(app_data: &Path, settings: &Settings) -> anyhow::Result<()> {
    std::fs::create_dir_all(app_data)
        .with_context(|| format!("failed to create {}", app_data.display()))?;
    let path = file(app_data);
    let json = serde_json::to_string_pretty(settings)?;
    std::fs::write(&path, json).with_context(|| format!("failed to write {}", path.display()))?;
    restrict(&path);
    Ok(())
}

/// Narrows the file to the owner. On Windows the app-data directory is already
/// per-user, so this is a no-op there; it matters if the project ever ships a
/// Linux build.
#[cfg(unix)]
fn restrict(path: &Path) {
    use std::os::unix::fs::PermissionsExt;
    if let Err(err) = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)) {
        tracing::warn!(%err, "could not restrict settings file permissions");
    }
}

#[cfg(not(unix))]
fn restrict(_path: &Path) {}

/// Trims a user-typed origin into something that can be concatenated with an
/// API path. A trailing slash would otherwise produce `//api/v1/...`.
pub fn normalize_url(raw: &str) -> String {
    let trimmed = raw.trim().trim_end_matches('/');
    if trimmed.is_empty() {
        return String::new();
    }
    if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
        trimmed.to_owned()
    } else {
        // The first-stage deployment is plain HTTP, so a bare host means http.
        format!("http://{trimmed}")
    }
}

#[cfg(test)]
mod tests {
    use super::normalize_url;

    #[test]
    fn normalizes_user_typed_origins() {
        assert_eq!(normalize_url("  example.com:8080/ "), "http://example.com:8080");
        assert_eq!(normalize_url("https://n.example.com/"), "https://n.example.com");
        assert_eq!(normalize_url("http://[::1]:8080"), "http://[::1]:8080");
        assert_eq!(normalize_url("   "), "");
    }
}
