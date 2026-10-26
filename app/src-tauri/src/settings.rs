//! The user's settings, kept as JSON in the OxiCut data folder
//! (`%APPDATA%\oxicut\settings.json`).

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// Everything the Settings panel edits. Missing fields fall back to defaults,
/// so a settings file from an older version still loads.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AppSettings {
    /// Detector: `v3` or `v7-lite` (v5 and v7 need models and are not available yet).
    pub mode: String,
    pub min_area_percent: f64,
    pub max_area_percent: f64,
    pub inset_px: i32,
    /// `same`, `jpg`, `png` or `tiff`.
    pub format: String,
    pub jpeg_quality: u8,
    /// File name pattern; `{source}` and `{n}` are replaced.
    pub pattern: String,
    /// Where crops go; `None` means an `OxiCut output` folder beside each scan.
    pub output_dir: Option<String>,
    /// Approve scans with nothing to flag as soon as they are detected.
    pub auto_approve: bool,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            mode: "v7-lite".into(),
            min_area_percent: 2.0,
            max_area_percent: 80.0,
            inset_px: 0,
            format: "same".into(),
            jpeg_quality: 92,
            pattern: "{source}-{n}".into(),
            output_dir: None,
            auto_approve: true,
        }
    }
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
    fn partial_and_future_files_still_load() {
        let from_old: AppSettings =
            serde_json::from_str(r#"{"insetPx": 4, "somethingNew": true}"#).unwrap();
        assert_eq!(from_old.inset_px, 4);
        assert_eq!(from_old.format, "same");
        assert_eq!(from_old.pattern, "{source}-{n}");
        assert!(from_old.auto_approve);
    }

    #[test]
    fn settings_round_trip_through_json_with_camel_case_keys() {
        let settings = AppSettings {
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
