//! User settings, persisted as JSON in the platform's config directory.
//!
//! Every field is `#[serde(default)]`, so a settings file written by an older
//! or newer build still loads: unknown keys are ignored and missing ones fall
//! back to the default. A corrupt file is never fatal — the app starts with
//! defaults instead of refusing to run.

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Bumped only for changes that need migration code, not for added fields.
pub const CURRENT_VERSION: u32 = 1;

const UI_SCALE_RANGE: std::ops::RangeInclusive<f32> = 0.75..=2.0;
const FONT_SIZE_RANGE: std::ops::RangeInclusive<f32> = 10.0..=24.0;

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
#[serde(rename_all = "lowercase")]
pub enum Theme {
    #[default]
    System,
    Light,
    Dark,
}

impl Theme {
    pub const ALL: &'static [Theme] = &[Theme::System, Theme::Light, Theme::Dark];

    pub fn label(self) -> &'static str {
        match self {
            Theme::System => "System",
            Theme::Light => "Light",
            Theme::Dark => "Dark",
        }
    }
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
#[serde(default)]
pub struct Settings {
    pub version: u32,
    pub theme: Theme,
    /// Scales the whole interface, including widget padding.
    pub ui_scale: f32,
    /// Point size of the text in the input and output panes.
    pub font_size: f32,
    /// Soft-wrap long lines in the panes instead of scrolling horizontally.
    pub wrap: bool,
    /// Tool to reopen on launch. `None`, or an id that no longer exists,
    /// falls back to the first tool.
    pub selected_tool: Option<String>,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            version: CURRENT_VERSION,
            theme: Theme::default(),
            ui_scale: 1.0,
            font_size: 14.0,
            wrap: true,
            selected_tool: None,
        }
    }
}

impl Settings {
    /// Where settings live on this platform. `None` if the environment gives us
    /// nowhere to write, in which case settings simply don't persist.
    pub fn path() -> Option<PathBuf> {
        config_dir().map(|dir| dir.join("settings.json"))
    }

    /// Loads settings, falling back to defaults for anything missing, corrupt
    /// or out of range. Never fails.
    pub fn load() -> Self {
        match Settings::path() {
            Some(path) => Settings::load_from(&path),
            None => Settings::default(),
        }
    }

    pub fn load_from(path: &Path) -> Self {
        let Ok(text) = fs::read_to_string(path) else {
            return Settings::default(); // First run: no file yet.
        };

        match serde_json::from_str::<Settings>(&text) {
            Ok(settings) => settings.sanitized(),
            Err(error) => {
                // Losing preferences is annoying; refusing to start is worse.
                eprintln!("rustafari: ignoring unreadable settings at {path:?}: {error}");
                Settings::default()
            }
        }
    }

    pub fn save(&self) {
        if let Some(path) = Settings::path() {
            if let Err(error) = self.save_to(&path) {
                eprintln!("rustafari: could not save settings to {path:?}: {error}");
            }
        }
    }

    /// Writes via a temporary file and a rename, so an interrupted save leaves
    /// the previous settings intact rather than a truncated file.
    pub fn save_to(&self, path: &Path) -> std::io::Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        let json = serde_json::to_string_pretty(self)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

        let temp = path.with_extension("json.tmp");
        fs::write(&temp, json)?;
        fs::rename(&temp, path)
    }

    /// Clamps values into ranges the UI can actually render. A hand-edited file
    /// with `"ui_scale": 100` should not produce an unusable window.
    fn sanitized(mut self) -> Self {
        let defaults = Settings::default();

        self.ui_scale = if self.ui_scale.is_finite() {
            self.ui_scale
                .clamp(*UI_SCALE_RANGE.start(), *UI_SCALE_RANGE.end())
        } else {
            defaults.ui_scale
        };

        self.font_size = if self.font_size.is_finite() {
            self.font_size
                .clamp(*FONT_SIZE_RANGE.start(), *FONT_SIZE_RANGE.end())
        } else {
            defaults.font_size
        };

        self.version = CURRENT_VERSION;
        self
    }
}

pub fn ui_scale_range() -> std::ops::RangeInclusive<f32> {
    UI_SCALE_RANGE
}

pub fn font_size_range() -> std::ops::RangeInclusive<f32> {
    FONT_SIZE_RANGE
}

fn config_dir() -> Option<PathBuf> {
    let home = || std::env::var_os("HOME").map(PathBuf::from);

    if cfg!(target_os = "macos") {
        home().map(|h| h.join("Library/Application Support/rustafari"))
    } else if cfg!(target_os = "windows") {
        std::env::var_os("APPDATA").map(|a| PathBuf::from(a).join("rustafari"))
    } else {
        std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| home().map(|h| h.join(".config")))
            .map(|c| c.join("rustafari"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A scratch directory that cleans itself up, so tests never touch the
    /// real config location.
    struct TempDir(PathBuf);

    impl TempDir {
        fn new(name: &str) -> Self {
            let dir = std::env::temp_dir().join(format!("rustafari-test-{name}"));
            let _ = fs::remove_dir_all(&dir);
            fs::create_dir_all(&dir).unwrap();
            TempDir(dir)
        }

        fn file(&self) -> PathBuf {
            self.0.join("settings.json")
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn round_trips_through_disk() {
        let dir = TempDir::new("round-trip");
        let settings = Settings {
            theme: Theme::Dark,
            ui_scale: 1.25,
            font_size: 18.0,
            wrap: false,
            selected_tool: Some("base64".into()),
            ..Settings::default()
        };

        settings.save_to(&dir.file()).unwrap();
        assert_eq!(Settings::load_from(&dir.file()), settings);
    }

    #[test]
    fn missing_file_yields_defaults() {
        let dir = TempDir::new("missing");
        assert_eq!(Settings::load_from(&dir.file()), Settings::default());
    }

    #[test]
    fn corrupt_file_yields_defaults_instead_of_failing() {
        let dir = TempDir::new("corrupt");
        fs::write(dir.file(), "{ this is not json").unwrap();
        assert_eq!(Settings::load_from(&dir.file()), Settings::default());
    }

    #[test]
    fn partial_file_keeps_known_keys_and_defaults_the_rest() {
        let dir = TempDir::new("partial");
        fs::write(dir.file(), r#"{"theme":"light"}"#).unwrap();

        let loaded = Settings::load_from(&dir.file());
        assert_eq!(loaded.theme, Theme::Light);
        assert_eq!(loaded.font_size, Settings::default().font_size);
    }

    #[test]
    fn keys_from_a_newer_build_are_ignored() {
        let dir = TempDir::new("forward-compat");
        fs::write(
            dir.file(),
            r#"{"theme":"dark","some_future_setting":{"a":1},"version":99}"#,
        )
        .unwrap();

        let loaded = Settings::load_from(&dir.file());
        assert_eq!(loaded.theme, Theme::Dark);
        assert_eq!(loaded.version, CURRENT_VERSION);
    }

    #[test]
    fn absurd_hand_edited_values_are_clamped() {
        let dir = TempDir::new("clamp");
        fs::write(dir.file(), r#"{"ui_scale":100.0,"font_size":-5.0}"#).unwrap();

        let loaded = Settings::load_from(&dir.file());
        assert_eq!(loaded.ui_scale, *UI_SCALE_RANGE.end());
        assert_eq!(loaded.font_size, *FONT_SIZE_RANGE.start());
    }

    #[test]
    fn non_finite_values_fall_back_to_defaults() {
        let dir = TempDir::new("nan");
        // serde_json cannot represent NaN, so this is the shape a hand-edited
        // or third-party-written file would take.
        fs::write(dir.file(), r#"{"ui_scale":1e400}"#).unwrap();
        assert_eq!(
            Settings::load_from(&dir.file()).ui_scale,
            Settings::default().ui_scale
        );
    }

    #[test]
    fn saving_leaves_no_temporary_file_behind() {
        let dir = TempDir::new("atomic");
        Settings::default().save_to(&dir.file()).unwrap();

        let leftovers: Vec<_> = fs::read_dir(&dir.0)
            .unwrap()
            .filter_map(Result::ok)
            .map(|e| e.file_name())
            .filter(|name| name.to_string_lossy().ends_with(".tmp"))
            .collect();
        assert!(leftovers.is_empty(), "{leftovers:?}");
    }

    #[test]
    fn save_overwrites_an_existing_file() {
        let dir = TempDir::new("overwrite");
        Settings::default().save_to(&dir.file()).unwrap();

        let changed = Settings {
            font_size: 20.0,
            ..Settings::default()
        };
        changed.save_to(&dir.file()).unwrap();

        assert_eq!(Settings::load_from(&dir.file()), changed);
    }
}
