//! The user's settings, kept as JSON in the OxiCut data folder
//! (`%APPDATA%\oxicut\settings.json`).

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// A project the user opened lately, with what the Projects list shows about it (kept up to date
/// whenever the project is saved, so listing needs no file reads).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct RecentProject {
    pub path: String,
    pub scans: u32,
    pub approved: u32,
    /// Seconds since 1970.
    pub updated: u64,
}

/// Everything the Settings panel edits. Missing fields fall back to defaults,
/// so a settings file from an older version still loads.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AppSettings {
    /// How the interface is coloured: `system` (follow Windows), `light` or `dark`.
    pub theme: String,
    /// Detector: `v3`, `v7-lite` or `v7` (the last needs the MobileSAM models).
    pub mode: String,
    pub min_area_percent: f64,
    pub max_area_percent: f64,
    pub inset_px: i32,
    /// What a box is cut around: `card` (the whole card or print) or `picture`.
    pub cut_to: String,
    /// Turn pictures that are sideways or upside down the right way up as they are found.
    pub auto_orient: bool,
    /// `same`, `jpg`, `png` or `tiff`.
    pub format: String,
    pub jpeg_quality: u8,
    /// File name pattern; `{source}` and `{n}` are replaced.
    pub pattern: String,
    /// Copy the scan's resolution, colour profile and EXIF into the crops.
    pub keep_metadata: bool,
    /// Write OxiCut into each crop's `Software` field.
    pub tag_software: bool,
    /// Where crops go; `None` means an `OxiCut output` folder beside each scan.
    pub output_dir: Option<String>,
    /// Approve scans with nothing to flag as soon as they are detected.
    pub auto_approve: bool,
    /// Features that are built but not switched on for everyone (see `docs/feature-flags.md`).
    pub features: Vec<String>,
    /// The project reopened when the app starts.
    pub last_project: Option<String>,
    /// Recently used projects, most recent first.
    pub recent_projects: Vec<RecentProject>,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            theme: "system".into(),
            mode: "v7-lite".into(),
            min_area_percent: 2.0,
            max_area_percent: 80.0,
            inset_px: 0,
            cut_to: "card".into(),
            auto_orient: true,
            format: "same".into(),
            jpeg_quality: 92,
            pattern: "{source}-{n}".into(),
            keep_metadata: true,
            tag_software: true,
            output_dir: None,
            auto_approve: true,
            features: Vec::new(),
            last_project: None,
            recent_projects: Vec::new(),
        }
    }
}

/// The environment variable that switches features on for one run, for example `OXICUT_FEATURES=details`.
pub const FEATURES_ENV: &str = "OXICUT_FEATURES";

/// The features that are on: the ones in the environment variable (separated by commas, semicolons or
/// spaces) and the ones listed in the settings file, in lower case, each once, in a fixed order.
pub fn parse_features(from_env: Option<&str>, listed: &[String]) -> Vec<String> {
    let mut names: Vec<String> = from_env
        .unwrap_or_default()
        .split(|c: char| c == ',' || c == ';' || c.is_whitespace())
        .map(str::to_string)
        .chain(listed.iter().cloned())
        .map(|n| n.trim().to_lowercase())
        .filter(|n| !n.is_empty())
        .collect();
    names.sort();
    names.dedup();
    names
}

/// The features that are on for this run.
pub fn enabled_features() -> Vec<String> {
    parse_features(std::env::var(FEATURES_ENV).ok().as_deref(), &load().features)
}

/// `OXICUT_DATA_DIR` if set (tests, portable use), else `<user data>/oxicut`.
pub fn data_dir() -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os("OXICUT_DATA_DIR") {
        return Some(PathBuf::from(dir));
    }
    directories::BaseDirs::new().map(|d| d.data_dir().join("oxicut"))
}

/// Where preview images are cached: `OXICUT_CACHE_DIR`, else `<user cache>/oxicut/previews`.
pub fn cache_dir() -> PathBuf {
    if let Some(dir) = std::env::var_os("OXICUT_CACHE_DIR") {
        return PathBuf::from(dir);
    }
    directories::BaseDirs::new()
        .map(|d| d.cache_dir().join("oxicut").join("previews"))
        .unwrap_or_else(|| std::env::temp_dir().join("oxicut-previews"))
}

fn settings_file() -> Option<PathBuf> {
    data_dir().map(|d| d.join("settings.json"))
}

/// The saved settings, or the defaults when there is no usable file.
pub fn load() -> AppSettings {
    settings_file()
        .and_then(|path| std::fs::read_to_string(path).ok())
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

pub fn save(settings: &AppSettings) -> Result<(), String> {
    let path = settings_file().ok_or("no settings folder on this system")?;
    let dir = path.parent().expect("settings file has a parent");
    std::fs::create_dir_all(dir).map_err(|e| format!("could not create {}: {e}", dir.display()))?;
    let json = serde_json::to_string_pretty(settings).map_err(|e| e.to_string())?;
    // Write then rename so a crash cannot leave a half-written file.
    let part = path.with_extension("json.part");
    std::fs::write(&part, json).map_err(|e| format!("could not write settings: {e}"))?;
    std::fs::rename(&part, &path).map_err(|e| format!("could not save settings: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn features_come_from_the_environment_and_the_settings_file() {
        // Nothing is on by default.
        assert!(AppSettings::default().features.is_empty());
        assert!(parse_features(None, &[]).is_empty());
        // The variable: commas, semicolons and spaces all separate, case does not matter.
        assert_eq!(
            parse_features(Some("Photo-Details, other;third  fourth"), &[]),
            ["fourth", "other", "photo-details", "third"]
        );
        // The settings file adds to it, and a feature named twice is on once.
        let listed = vec!["PHOTO-DETAILS".to_string(), " extra ".to_string(), String::new()];
        assert_eq!(parse_features(Some("photo-details"), &listed), ["extra", "photo-details"]);
        // A settings file from before the list existed still loads, with none on.
        let old: AppSettings = serde_json::from_str(r#"{"insetPx": 4}"#).unwrap();
        assert!(old.features.is_empty());
        let new: AppSettings = serde_json::from_str(r#"{"features": ["photo-details"]}"#).unwrap();
        assert_eq!(new.features, ["photo-details"]);
    }

    #[test]
    fn partial_and_future_files_still_load() {
        let from_old: AppSettings =
            serde_json::from_str(r#"{"insetPx": 4, "somethingNew": true}"#).unwrap();
        assert_eq!(from_old.inset_px, 4);
        // A file from before the choice existed cuts around the whole card.
        assert_eq!(from_old.cut_to, "card");
        // And turns pictures the right way up when it can.
        assert!(from_old.auto_orient);
        assert_eq!(from_old.format, "same");
        assert_eq!(from_old.pattern, "{source}-{n}");
        assert!(from_old.auto_approve);
        assert_eq!(from_old.theme, "system");
        assert!(from_old.last_project.is_none() && from_old.recent_projects.is_empty());
    }

    #[test]
    fn settings_round_trip_through_json_with_camel_case_keys() {
        let settings = AppSettings {
            theme: "dark".into(),
            output_dir: Some("D:\\out".into()),
            jpeg_quality: 70,
            ..Default::default()
        };
        let json = serde_json::to_string(&settings).unwrap();
        assert!(
            json.contains("\"outputDir\"") && json.contains("\"jpegQuality\""),
            "{json}"
        );
        assert_eq!(
            serde_json::from_str::<AppSettings>(&json).unwrap(),
            settings
        );
    }

    #[test]
    fn save_and_load_use_the_data_dir_override() {
        // The override is process-wide, so this is the only test that sets it.
        let dir = std::env::temp_dir().join(format!("oxicut-settings-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        // SAFETY: only this test touches OXICUT_DATA_DIR.
        unsafe { std::env::set_var("OXICUT_DATA_DIR", &dir) };
        assert_eq!(load(), AppSettings::default(), "no file yet");
        let changed = AppSettings {
            inset_px: 3,
            format: "tiff".into(),
            ..Default::default()
        };
        save(&changed).unwrap();
        assert_eq!(load(), changed);
        // A corrupt file falls back to defaults rather than failing.
        std::fs::write(dir.join("settings.json"), b"{ nope").unwrap();
        assert_eq!(load(), AppSettings::default());
        unsafe { std::env::remove_var("OXICUT_DATA_DIR") };
    }
}
