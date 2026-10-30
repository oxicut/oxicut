//! Projects: one `.oxicut` file (JSON) per batch of scans, holding the scan list,
//! boxes and approvals so that work survives closing the app. Plain Rust, tested
//! without a window; the Tauri commands in `lib.rs` only move data to and from it.
//!
//! The file stores no pixels (previews and thumbnails are rebuilt from the scans), so it
//! stays small enough to copy or mail. Scans are named by absolute path and also by path
//! relative to the project file, so a project folder moved to another computer with its
//! scans still finds them.

use std::path::{Component, Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::engine::BoxDto;

pub const EXTENSION: &str = "oxicut";
const FORMAT_VERSION: u32 = 1;

/// What a project file holds. Missing fields load as defaults, so older and newer files still open.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Project {
    pub format_version: u32,
    /// The OxiCut that last wrote the file (for information).
    pub app_version: String,
    /// Seconds since 1970.
    pub created: u64,
    pub updated: u64,
    pub scans: Vec<ScanRecord>,
}

/// One scan of the project. The screen ids of boxes are not kept: they are made up fresh on open.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ScanRecord {
    pub id: u32,
    pub path: String,
    /// Path from the project file's folder, with `/` separators.
    pub rel_path: Option<String>,
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub status: String,
    pub boxes: Vec<BoxDto>,
    /// `boxesKey` of what detection produced, to tell whether the user changed anything.
    pub detected_key: String,
    /// `boxesKey` of the boxes last exported successfully.
    pub exported_key: Option<String>,
    /// The files the last export wrote for this scan.
    pub exported_files: Vec<String>,
    /// The scan holding the backs of this scan's pictures, when this is a scan of fronts.
    pub backs_on: Option<u32>,
    /// Which picture on this scan goes with which on `backs_on`: positions in the two box lists
    /// (front, back), since screen ids of boxes are not kept.
    pub pairs: Vec<[usize; 2]>,
    pub error: Option<String>,
}

/// A project as opened: with the scans' paths resolved and the ones that cannot be found listed.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenedProject {
    pub path: String,
    /// The file name without `.oxicut`.
    pub name: String,
    pub project: Project,
    /// Ids of scans whose file is gone.
    pub missing: Vec<u32>,
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn file_label(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string())
}

/// The project's name: its file name without the extension.
pub fn project_name(path: &Path) -> String {
    path.file_stem()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "Project".into())
}

/// `Documents\OxiCut`, or `OXICUT_PROJECTS_DIR` (tests, portable use).
pub fn default_dir() -> PathBuf {
    if let Some(dir) = std::env::var_os("OXICUT_PROJECTS_DIR") {
        return PathBuf::from(dir);
    }
    directories::UserDirs::new()
        .map(|d| {
            d.document_dir()
                .map(Path::to_path_buf)
                .unwrap_or_else(|| d.home_dir().to_path_buf())
                .join("OxiCut")
        })
        .unwrap_or_else(|| std::env::temp_dir().join("OxiCut"))
}

pub fn load(path: &Path) -> Result<Project, String> {
    let text = std::fs::read_to_string(path)
        .map_err(|e| format!("Could not read {}: {e}", file_label(path)))?;
    // Notepad and PowerShell like to start a UTF-8 file with a byte order mark.
    let project: Project = serde_json::from_str(text.trim_start_matches('\u{feff}'))
        .map_err(|e| format!("{} is not a usable OxiCut project: {e}", file_label(path)))?;
    if project.format_version > FORMAT_VERSION {
        return Err(format!(
            "{} was made by a newer version of OxiCut. Update OxiCut to open it.",
            file_label(path)
        ));
    }
    Ok(project)
}

/// Save `project` to `path`: write a temporary file beside it, then rename over the old one, so a
/// crash or a full disk never leaves a half-written project.
pub fn save(path: &Path, project: &mut Project) -> Result<(), String> {
    let stamp = now();
    project.format_version = FORMAT_VERSION;
    project.app_version = oxicut_core::VERSION.to_string();
    if project.created == 0 {
        // The screen may not know it (a project it just made): keep what the file says.
        project.created = load(path).map(|p| p.created).unwrap_or(0);
    }
    if project.created == 0 {
        project.created = stamp;
    }
    project.updated = stamp;
    let folder = path.parent().unwrap_or(Path::new("."));
    for scan in &mut project.scans {
        scan.rel_path = relative_path(folder, Path::new(&scan.path));
    }
    let json = serde_json::to_string_pretty(project).map_err(|e| e.to_string())?;
    std::fs::create_dir_all(folder)
        .map_err(|e| format!("Could not create {}: {e}", folder.display()))?;
    let part = path.with_extension("oxicut.tmp");
    std::fs::write(&part, json).map_err(|e| format!("Could not write {}: {e}", file_label(path)))?;
    std::fs::rename(&part, path).map_err(|e| {
        let _ = std::fs::remove_file(&part);
        format!("Could not save {}: {e}", file_label(path))
    })
}

fn lower(component: &Component) -> String {
    component.as_os_str().to_string_lossy().to_lowercase()
}

/// `target` relative to the folder `base`, with `/` separators, or `None` if they are on different drives.
pub fn relative_path(base: &Path, target: &Path) -> Option<String> {
    let base: Vec<Component> = base.components().collect();
    let target: Vec<Component> = target.components().collect();
    let common = base
        .iter()
        .zip(&target)
        .take_while(|(a, b)| lower(a) == lower(b))
        .count();
    // Nothing shared, not even a drive: no relative way from one to the other.
    if common == 0 {
        return None;
    }
    let mut parts: Vec<String> = vec!["..".into(); base.len() - common];
    parts.extend(target[common..].iter().map(|c| c.as_os_str().to_string_lossy().into_owned()));
    Some(parts.join("/"))
}

/// `dir` joined with `relative`, with `.` and `..` folded away (no file system access).
fn join_normalized(dir: &Path, relative: &str) -> PathBuf {
    let mut out = dir.to_path_buf();
    for part in relative.split(['/', '\\']) {
        match part {
            "" | "." => {}
            ".." => {
                out.pop();
            }
            other => out.push(other),
        }
    }
    out
}

/// Where a scan really is: its recorded path if the file is there, else the same scan found through
/// its path relative to the project, else the recorded path (so the "missing" message names it).
pub fn resolve_scan(project_file: &Path, scan: &ScanRecord) -> PathBuf {
    let recorded = PathBuf::from(&scan.path);
    if recorded.exists() {
        return recorded;
    }
    if let (Some(relative), Some(folder)) = (&scan.rel_path, project_file.parent()) {
        let candidate = join_normalized(folder, relative);
        if candidate.exists() {
            return candidate;
        }
    }
    recorded
}

pub fn open(path: &Path) -> Result<OpenedProject, String> {
    let mut project = load(path)?;
    let mut missing = Vec::new();
    for scan in &mut project.scans {
        let found = resolve_scan(path, scan);
        if !found.exists() {
            missing.push(scan.id);
        }
        scan.path = found.to_string_lossy().into_owned();
    }
    Ok(OpenedProject {
        path: path.to_string_lossy().into_owned(),
        name: project_name(path),
        project,
        missing,
    })
}

/// A file name for a project called `name` in `dir` that does not exist yet: `name.oxicut`, then
/// `name (2).oxicut`, and so on.
pub fn unique_path(dir: &Path, name: &str) -> PathBuf {
    let name = name.trim();
    let stem = oxicut_core::export::sanitize_file_name(if name.is_empty() { "My scans" } else { name });
    let first = dir.join(format!("{stem}.{EXTENSION}"));
    if !first.exists() {
        return first;
    }
    (2..)
        .map(|i| dir.join(format!("{stem} ({i}).{EXTENSION}")))
        .find(|p| !p.exists())
        .expect("an unused name exists")
}

/// Make an empty project called `name` in `dir`.
pub fn create(dir: &Path, name: &str) -> Result<PathBuf, String> {
    let path = unique_path(dir, name);
    save(&path, &mut Project::default())?;
    Ok(path)
}

/// Rename the project file, keeping it in its folder. Refuses to take another project's name.
pub fn rename(path: &Path, new_name: &str) -> Result<PathBuf, String> {
    let new_name = new_name.trim();
    if new_name.is_empty() {
        return Err("Give the project a name.".into());
    }
    let stem = oxicut_core::export::sanitize_file_name(new_name);
    let target = path
        .parent()
        .unwrap_or(Path::new("."))
        .join(format!("{stem}.{EXTENSION}"));
    if target == path {
        return Ok(target);
    }
    if target.exists() {
        return Err(format!("There is already a project called {stem} in that folder."));
    }
    std::fs::rename(path, &target).map_err(|e| format!("Could not rename the project: {e}"))?;
    Ok(target)
}

/// Command-line arguments (or those of a second launch) split into the project to open, the first
/// `.oxicut` file named, and everything else, which are scans or folders to add.
pub fn split_startup(args: &[PathBuf]) -> (Option<PathBuf>, Vec<PathBuf>) {
    let is_project = |p: &PathBuf| p.extension().is_some_and(|e| e.eq_ignore_ascii_case(EXTENSION));
    let project = args.iter().find(|p| is_project(p)).cloned();
    let rest = args.iter().filter(|p| !is_project(p)).cloned().collect();
    (project, rest)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn startup_arguments_separate_the_project_from_the_scans() {
        let args: Vec<PathBuf> = ["D:\\scans\\a.tif", "C:\\Docs\\Trip.OXICUT", "D:\\more", "E:\\x.oxicut"]
            .iter()
            .map(PathBuf::from)
            .collect();
        let (project, rest) = split_startup(&args);
        assert_eq!(project, Some(PathBuf::from("C:\\Docs\\Trip.OXICUT")));
        assert_eq!(rest, vec![PathBuf::from("D:\\scans\\a.tif"), PathBuf::from("D:\\more")]);
        assert_eq!(split_startup(&[]), (None, vec![]));
    }

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("oxicut-projects-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn record(id: u32, path: &Path) -> ScanRecord {
        ScanRecord {
            id,
            path: path.to_string_lossy().into_owned(),
            name: file_label(path),
            width: 1000,
            height: 700,
            status: "approved".into(),
            boxes: vec![BoxDto {
                cx: 500.0,
                cy: 350.0,
                w: 300.0,
                h: 200.0,
                angle: 3.5,
            }],
            detected_key: "500,350,300,200,3.5".into(),
            exported_key: Some("500,350,300,200,3.5".into()),
            exported_files: vec!["D:/out/a-1.tif".into()],
            backs_on: Some(7),
            pairs: vec![[0, 0]],
            ..Default::default()
        }
    }

    #[test]
    fn a_saved_project_loads_back_the_same() {
        let dir = scratch("roundtrip");
        let file = dir.join("p.oxicut");
        let mut project = Project {
            scans: vec![record(1, &dir.join("a.tif")), record(7, &dir.join("b.jpg"))],
            ..Default::default()
        };
        save(&file, &mut project).unwrap();
        let loaded = load(&file).unwrap();
        assert_eq!(loaded, project);
        assert_eq!(loaded.format_version, 1);
        assert!(loaded.created > 0 && loaded.updated >= loaded.created);
        assert_eq!(loaded.scans[1].id, 7);
        assert_eq!(loaded.scans[0].rel_path.as_deref(), Some("a.tif"));
    }

    #[test]
    fn saving_leaves_no_temporary_file_and_keeps_the_creation_time() {
        let dir = scratch("atomic");
        let file = dir.join("p.oxicut");
        let mut project = Project::default();
        save(&file, &mut project).unwrap();
        let created = project.created;
        save(&file, &mut project).unwrap();
        assert_eq!(project.created, created);
        let names: Vec<String> = std::fs::read_dir(&dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, vec!["p.oxicut"]);
    }

    #[test]
    fn a_damaged_file_is_reported_by_name_and_left_alone() {
        let dir = scratch("damaged");
        let file = dir.join("broken.oxicut");
        std::fs::write(&file, "{ \"scans\": [ {").unwrap();
        let error = load(&file).unwrap_err();
        assert!(error.contains("broken.oxicut"), "{error}");
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "{ \"scans\": [ {");
    }

    #[test]
    fn missing_and_unknown_fields_still_load() {
        let dir = scratch("tolerant");
        let file = dir.join("p.oxicut");
        std::fs::write(
            &file,
            r#"{"scans":[{"id":3,"path":"C:\\x.tif","somethingNew":true}],"future":{"a":1}}"#,
        )
        .unwrap();
        let project = load(&file).unwrap();
        assert_eq!(project.scans.len(), 1);
        assert_eq!(project.scans[0].id, 3);
        assert!(project.scans[0].boxes.is_empty());
        assert_eq!((project.scans[0].backs_on, project.scans[0].pairs.len()), (None, 0));
        assert_eq!(project.format_version, 0);
    }

    #[test]
    fn a_file_saved_by_an_editor_with_a_byte_order_mark_still_opens() {
        let dir = scratch("bom");
        let file = dir.join("p.oxicut");
        std::fs::write(&file, "\u{feff}{\"scans\": []}").unwrap();
        assert!(load(&file).unwrap().scans.is_empty());
    }

    #[test]
    fn a_file_from_a_newer_oxicut_is_not_opened() {
        let dir = scratch("newer");
        let file = dir.join("p.oxicut");
        std::fs::write(&file, r#"{"formatVersion": 99}"#).unwrap();
        let error = load(&file).unwrap_err();
        assert!(error.contains("newer version"), "{error}");
    }

    #[test]
    fn relative_paths_climb_out_of_the_project_folder() {
        let base = Path::new("/home/me/projects");
        assert_eq!(relative_path(base, Path::new("/home/me/projects/a.tif")).as_deref(), Some("a.tif"));
        assert_eq!(
            relative_path(base, Path::new("/home/me/scans/2024/a.tif")).as_deref(),
            Some("../scans/2024/a.tif")
        );
        assert_eq!(join_normalized(Path::new("/home/me/projects"), "../scans/a.tif"), Path::new("/home/me/scans/a.tif"));
        #[cfg(windows)]
        assert_eq!(relative_path(Path::new("C:\\p"), Path::new("D:\\scans\\a.tif")), None);
    }

    #[test]
    fn a_moved_project_finds_its_scans_through_the_relative_path() {
        let dir = scratch("moved");
        // The project was made when the scans lived somewhere else; the folder was then copied here.
        let scans = dir.join("scans");
        std::fs::create_dir_all(&scans).unwrap();
        std::fs::write(scans.join("a.tif"), b"x").unwrap();
        let file = dir.join("p.oxicut");
        let mut gone = record(1, &dir.join("old place").join("scans").join("a.tif"));
        gone.rel_path = Some("scans/a.tif".into());
        let mut elsewhere = record(2, &dir.join("old place").join("scans").join("b.tif"));
        elsewhere.rel_path = Some("scans/b.tif".into());
        let json = serde_json::to_string(&Project {
            scans: vec![gone, elsewhere],
            ..Default::default()
        })
        .unwrap();
        std::fs::write(&file, json).unwrap();

        let opened = open(&file).unwrap();
        assert_eq!(PathBuf::from(&opened.project.scans[0].path), scans.join("a.tif"));
        assert_eq!(opened.missing, vec![2]);
        assert_eq!(opened.name, "p");
    }

    #[test]
    fn new_projects_never_take_an_existing_name() {
        let dir = scratch("unique");
        let first = create(&dir, "Grandma's postcards").unwrap();
        let second = create(&dir, "Grandma's postcards").unwrap();
        assert_eq!(file_label(&first), "Grandma's postcards.oxicut");
        assert_eq!(file_label(&second), "Grandma's postcards (2).oxicut");
        assert!(load(&first).unwrap().scans.is_empty());
        // Names Windows would refuse are cleaned, and an empty one gets a default.
        assert_eq!(file_label(&unique_path(&dir, "a/b:c")), "a_b_c.oxicut");
        assert_eq!(file_label(&unique_path(&dir, "  ")), "My scans.oxicut");
    }

    #[test]
    fn renaming_keeps_the_file_in_its_folder_and_refuses_clashes() {
        let dir = scratch("rename");
        let a = create(&dir, "One").unwrap();
        let b = create(&dir, "Two").unwrap();
        let renamed = rename(&a, "Holiday 1985").unwrap();
        assert_eq!(renamed, dir.join("Holiday 1985.oxicut"));
        assert!(!a.exists() && renamed.exists());
        assert!(rename(&renamed, "Two").unwrap_err().contains("already"));
        assert!(b.exists());
        assert_eq!(rename(&renamed, "Holiday 1985").unwrap(), renamed);
        assert!(rename(&renamed, "   ").is_err());
    }
}
