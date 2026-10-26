//! The model manager: what is downloaded, and downloading it.

use std::path::Path;

use oxicut_models::{ModelSpec, Status, ensure_model, model_path, status};
use serde::Serialize;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ModelStatusDto {
    pub id: String,
    pub label: String,
    pub filename: String,
    pub approx_bytes: u64,
    /// `missing`, `ready` or `corrupt`.
    pub state: String,
    pub size_bytes: Option<u64>,
    /// Which detectors use it, for the explanation next to it.
    pub used_by: String,
}

fn used_by(spec: &ModelSpec) -> &'static str {
    match spec.id {
        "mobilesam-encoder" | "mobilesam-decoder" => "Detectors v5 and v7",
        _ => "Not used yet (planned: telling which way up a print is)",
    }
}

pub fn statuses(dir: &Path) -> Result<Vec<ModelStatusDto>, String> {
    oxicut_models::all()
        .into_iter()
        .map(|spec| {
            let state = match status(dir, spec).map_err(|e| e.to_string())? {
                Status::Missing => "missing",
                Status::Ready => "ready",
                Status::Corrupt { .. } => "corrupt",
            };
            Ok(ModelStatusDto {
                id: spec.id.into(),
                label: spec.label.into(),
                filename: spec.filename.into(),
                approx_bytes: spec.approx_bytes,
                state: state.into(),
                size_bytes: std::fs::metadata(model_path(dir, spec))
                    .ok()
                    .map(|m| m.len()),
                used_by: used_by(spec).into(),
            })
        })
        .collect()
}

#[derive(Debug, Clone, Serialize)]
#[serde(
    tag = "event",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum DownloadEvent {
    Progress {
        id: String,
        downloaded: u64,
        total: Option<u64>,
    },
    Finished {
        id: String,
    },
    Failed {
        id: String,
        error: String,
    },
}

/// Download (and verify) each model in `ids`, one after another. A failure is
/// reported for that model and the rest still run.
pub fn download(dir: &Path, ids: &[String], emit: &dyn Fn(DownloadEvent)) -> Result<(), String> {
    for id in ids {
        let spec = oxicut_models::find(id).ok_or_else(|| format!("unknown model '{id}'"))?;
        let result = ensure_model(dir, spec, |p| {
            emit(DownloadEvent::Progress {
                id: id.clone(),
                downloaded: p.downloaded,
                total: p.total,
            });
        });
        emit(match result {
            Ok(_) => DownloadEvent::Finished { id: id.clone() },
            Err(error) => DownloadEvent::Failed {
                id: id.clone(),
                error: error.to_string(),
            },
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> std::path::PathBuf {
        let dir =
            std::env::temp_dir().join(format!("oxicut-appmodels-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn every_registered_model_is_listed_and_a_wrong_file_is_corrupt() {
        let dir = scratch("status");
        let before = statuses(&dir).unwrap();
        assert_eq!(before.len(), oxicut_models::all().len());
        assert!(
            before
                .iter()
                .all(|m| m.state == "missing" && m.size_bytes.is_none())
        );

        std::fs::write(
            dir.join(oxicut_models::MOBILESAM_ENCODER.filename),
            b"half a model",
        )
        .unwrap();
        let after = statuses(&dir).unwrap();
        let encoder = after.iter().find(|m| m.id == "mobilesam-encoder").unwrap();
        assert_eq!(
            (encoder.state.as_str(), encoder.size_bytes),
            ("corrupt", Some(12))
        );
        assert!(encoder.used_by.contains("v7"));
    }

    #[test]
    fn unknown_ids_are_an_error_before_anything_downloads() {
        let dir = scratch("unknown");
        let error = download(&dir, &["no-such-model".into()], &|_| {}).unwrap_err();
        assert!(error.contains("no-such-model"));
    }

    #[test]
    fn status_serialises_in_the_shape_the_screen_reads() {
        let json = serde_json::to_value(&statuses(&scratch("json")).unwrap()[0]).unwrap();
        for key in [
            "id",
            "label",
            "filename",
            "approxBytes",
            "state",
            "sizeBytes",
            "usedBy",
        ] {
            assert!(json.get(key).is_some(), "missing {key}: {json}");
        }
    }
}
