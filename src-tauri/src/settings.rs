use serde::{Deserialize, Serialize};
use std::path::Path;

pub const DEFAULT_HOTKEY: &str = "Ctrl+Space";

/// User preferences, persisted as JSON next to the index database.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Global shortcut that toggles the launcher, e.g. "Ctrl+Space".
    pub hotkey: String,
    /// "system", "dark" or "light".
    pub theme: String,
    /// "auto" or a language code such as "en".
    pub language: String,
    /// "google", "bing" or "duckduckgo".
    pub search_engine: String,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            hotkey: DEFAULT_HOTKEY.to_string(),
            theme: "system".to_string(),
            language: "auto".to_string(),
            search_engine: "google".to_string(),
        }
    }
}

impl Settings {
    /// Load settings, falling back to defaults if the file is missing or corrupt.
    pub fn load(path: &Path) -> Self {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, path: &Path) -> Result<(), String> {
        let text = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        std::fs::write(path, text).map_err(|e| format!("Failed to save settings: {}", e))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_and_tolerates_missing_or_partial_files() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");

        assert_eq!(Settings::load(&path), Settings::default());

        let custom = Settings { hotkey: "Alt+Space".into(), theme: "light".into(), language: "de".into(), search_engine: "bing".into() };
        custom.save(&path).unwrap();
        assert_eq!(Settings::load(&path), custom);

        std::fs::write(&path, r#"{"hotkey":"Ctrl+Alt+K"}"#).unwrap();
        let partial = Settings::load(&path);
        assert_eq!(partial.hotkey, "Ctrl+Alt+K");
        assert_eq!(partial.theme, "system");

        std::fs::write(&path, "not json").unwrap();
        assert_eq!(Settings::load(&path), Settings::default());
    }
}
