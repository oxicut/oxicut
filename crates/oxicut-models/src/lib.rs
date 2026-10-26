//! Model registry and checksum-verified downloads for OxiCut.
//!
//! Nothing here is bundled: models are fetched on first use, hashed, and stored
//! in one directory shared by the command line and the desktop app
//! (`%APPDATA%\oxicut\models` on Windows). A file whose SHA-256 does not match
//! the registry is never returned; it is deleted and fetched again.
//!
//! URLs, revision and hashes are copied from ScanSplitter's `models.py`.

use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

use sha2::{Digest, Sha256};

/// One downloadable model file.
#[derive(Debug, Clone, Copy)]
pub struct ModelSpec<'a> {
    /// Stable short name used on the command line and in settings.
    pub id: &'a str,
    /// Name shown to people.
    pub label: &'a str,
    /// File name inside the models directory.
    pub filename: &'a str,
    /// Mirrors, tried in order.
    pub urls: &'a [&'a str],
    /// Lower-case hex SHA-256 of the file.
    pub sha256: &'a str,
    /// Approximate download size, for display before the real size is known.
    pub approx_bytes: u64,
}

/// MobileSAM revision ScanSplitter pins; the hashes below belong to it. The URLs spell it
/// out (statics cannot interpolate), and a test keeps the two in step.
#[cfg(test)]
const MOBILESAM_REVISION: &str = "0d3b403339b4674a82493d5e97964dd78089ddc8";

pub static MOBILESAM_ENCODER: ModelSpec<'static> = ModelSpec {
    id: "mobilesam-encoder",
    label: "MobileSAM image encoder",
    filename: "mobile_sam_image_encoder.onnx",
    urls: &[
        "https://huggingface.co/Acly/MobileSAM/resolve/0d3b403339b4674a82493d5e97964dd78089ddc8/mobile_sam_image_encoder.onnx",
    ],
    sha256: "580f5fb648ea1062c0aabc26217aed56921985f03f0cbbd852bba81d760cc749",
    approx_bytes: 27_000_000,
};

pub static MOBILESAM_DECODER: ModelSpec<'static> = ModelSpec {
    id: "mobilesam-decoder",
    label: "MobileSAM mask decoder",
    filename: "sam_mask_decoder_single.onnx",
    urls: &[
        "https://huggingface.co/Acly/MobileSAM/resolve/0d3b403339b4674a82493d5e97964dd78089ddc8/sam_mask_decoder_single.onnx",
    ],
    sha256: "93915fc7c993ab9d59ab8c9ccd3bce37f7509c81ab4150a74abd4d2abbd8570d",
    approx_bytes: 16_000_000,
};

/// Used by a later phase to tell which way is up; registered now so the
/// download machinery is the same for every model.
pub static ORIENTATION: ModelSpec<'static> = ModelSpec {
    id: "orientation",
    label: "Orientation model",
    filename: "orientation_model_v2.onnx",
    urls: &[
        "https://github.com/duartebarbosadev/deep-image-orientation-detection/releases/download/v2/orientation_model_v2_0.9882.onnx",
        "https://github.com/Madnex/ScanSplitter/releases/download/models-v1/orientation_model_v2.onnx",
    ],
    sha256: "cffe911c1dff47fbfbbd90110aaab9c07134645c460d35b3ae8832079bea91ba",
    approx_bytes: 80_000_000,
};

/// Every registered model.
pub fn all() -> [&'static ModelSpec<'static>; 3] {
    [&MOBILESAM_ENCODER, &MOBILESAM_DECODER, &ORIENTATION]
}

/// The models the MobileSAM-based detectors (v5, v7) need.
pub fn mobilesam() -> [&'static ModelSpec<'static>; 2] {
    [&MOBILESAM_ENCODER, &MOBILESAM_DECODER]
}

/// Look a model up by its id.
pub fn find(id: &str) -> Option<&'static ModelSpec<'static>> {
    all().into_iter().find(|spec| spec.id == id)
}

#[derive(Debug, thiserror::Error)]
pub enum ModelError {
    #[error("file error for {}: {source}", path.display())]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("could not download {label}: {}", errors.join("; "))]
    DownloadFailed { label: String, errors: Vec<String> },
    #[error("no model data directory could be determined on this system")]
    NoDataDirectory,
}

fn io_error(path: &Path) -> impl FnOnce(std::io::Error) -> ModelError + '_ {
    move |source| ModelError::Io {
        path: path.to_path_buf(),
        source,
    }
}

/// Where models live: `OXICUT_MODELS_DIR` if set, else `<user data>/oxicut/models`
/// (`%APPDATA%\oxicut\models` on Windows).
pub fn models_dir() -> Result<PathBuf, ModelError> {
    if let Some(dir) = std::env::var_os("OXICUT_MODELS_DIR") {
        return Ok(PathBuf::from(dir));
    }
    let base = directories::BaseDirs::new().ok_or(ModelError::NoDataDirectory)?;
    Ok(base.data_dir().join("oxicut").join("models"))
}

pub fn model_path(dir: &Path, spec: &ModelSpec) -> PathBuf {
    dir.join(spec.filename)
}

/// State of a model file on disk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Status {
    Missing,
    /// Present and the checksum matches.
    Ready,
    /// Present but the checksum does not match; carries what the file hashes to.
    Corrupt {
        actual_sha256: String,
    },
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// SHA-256 of a file, streamed.
pub fn sha256_file(path: &Path) -> std::io::Result<String> {
    let mut file = fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; 256 * 1024];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hex(&hasher.finalize()))
}

pub fn status(dir: &Path, spec: &ModelSpec) -> Result<Status, ModelError> {
    let path = model_path(dir, spec);
    if !path.is_file() {
        return Ok(Status::Missing);
    }
    let actual = sha256_file(&path).map_err(io_error(&path))?;
    Ok(if actual == spec.sha256 {
        Status::Ready
    } else {
        Status::Corrupt {
            actual_sha256: actual,
        }
    })
}

/// Download progress; `total` is `None` when the server does not say.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Progress {
    pub downloaded: u64,
    pub total: Option<u64>,
}

/// Make sure `spec` is on disk and verified, downloading it if needed, and
/// return its path. `progress` is called as bytes arrive.
///
/// A file that fails its checksum is deleted first. Downloads go to a `.part`
/// file that is renamed into place only after the hash matches, so a failed or
/// tampered download never leaves a bad file at the final path.
pub fn ensure_model(
    dir: &Path,
    spec: &ModelSpec,
    mut progress: impl FnMut(Progress),
) -> Result<PathBuf, ModelError> {
    let path = model_path(dir, spec);
    match status(dir, spec)? {
        Status::Ready => return Ok(path),
        Status::Corrupt { .. } => fs::remove_file(&path).map_err(io_error(&path))?,
        Status::Missing => {}
    }
    fs::create_dir_all(dir).map_err(io_error(dir))?;

    let mut errors = Vec::new();
    for url in spec.urls {
        match fetch(url, &path, spec.sha256, &mut progress) {
            Ok(()) => return Ok(path),
            Err(message) => errors.push(format!("{url}: {message}")),
        }
    }
    Err(ModelError::DownloadFailed {
        label: spec.label.to_string(),
        errors,
    })
}

/// Download one URL to `dest` via `dest.part`, checking the hash before the rename.
fn fetch(
    url: &str,
    dest: &Path,
    sha256: &str,
    progress: &mut impl FnMut(Progress),
) -> Result<(), String> {
    let part = {
        let mut name = dest.as_os_str().to_owned();
        name.push(".part");
        PathBuf::from(name)
    };
    let result = fetch_into(url, &part, sha256, progress).and_then(|()| {
        fs::rename(&part, dest).map_err(|e| format!("could not move the download into place: {e}"))
    });
    if result.is_err() {
        let _ = fs::remove_file(&part);
    }
    result
}

fn fetch_into(
    url: &str,
    part: &Path,
    sha256: &str,
    progress: &mut impl FnMut(Progress),
) -> Result<(), String> {
    let client = reqwest::blocking::Client::builder()
        .connect_timeout(Duration::from_secs(30))
        // Overall limit: a stalled connection must not hang the app for ever.
        .timeout(Duration::from_secs(60 * 15))
        .build()
        .map_err(|e| e.to_string())?;
    let mut response = client.get(url).send().map_err(|e| e.to_string())?;
    if !response.status().is_success() {
        return Err(format!("server answered {}", response.status()));
    }
    let total = response.content_length().filter(|&n| n > 0);

    let mut file =
        fs::File::create(part).map_err(|e| format!("could not create {}: {e}", part.display()))?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; 256 * 1024];
    let mut downloaded = 0u64;
    loop {
        let read = response.read(&mut buffer).map_err(|e| e.to_string())?;
        if read == 0 {
            break;
        }
        file.write_all(&buffer[..read]).map_err(|e| e.to_string())?;
        hasher.update(&buffer[..read]);
        downloaded += read as u64;
        progress(Progress { downloaded, total });
    }
    file.flush().map_err(|e| e.to_string())?;
    drop(file);

    let actual = hex(&hasher.finalize());
    if actual != sha256 {
        return Err(format!(
            "checksum mismatch: expected {sha256}, got {actual}"
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufRead, BufReader};
    use std::net::TcpListener;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn sha256_bytes(bytes: &[u8]) -> String {
        hex(&Sha256::digest(bytes))
    }

    /// A tiny HTTP server: `/good` serves GOOD, `/bad` serves tampered bytes,
    /// anything else is a 404. Counts requests.
    struct Server {
        base: String,
        requests: Arc<AtomicUsize>,
    }

    const GOOD: &[u8] =
        b"pretend this is an onnx model, long enough to need more than one read ...";

    fn serve() -> Server {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let requests = Arc::new(AtomicUsize::new(0));
        let counter = requests.clone();
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { continue };
                counter.fetch_add(1, Ordering::SeqCst);
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                let mut request_line = String::new();
                reader.read_line(&mut request_line).unwrap();
                loop {
                    let mut line = String::new();
                    reader.read_line(&mut line).unwrap();
                    if line == "\r\n" || line.is_empty() {
                        break;
                    }
                }
                let path = request_line
                    .split_whitespace()
                    .nth(1)
                    .unwrap_or("/")
                    .to_string();
                let (status, body): (&str, Vec<u8>) = match path.as_str() {
                    "/good" => ("200 OK", GOOD.to_vec()),
                    "/bad" => (
                        "200 OK",
                        b"tampered bytes of the same sort of length........................"
                            .to_vec(),
                    ),
                    _ => ("404 Not Found", b"nope".to_vec()),
                };
                let head = format!(
                    "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                );
                let _ = stream.write_all(head.as_bytes());
                let _ = stream.write_all(&body);
            }
        });
        Server { base, requests }
    }

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("oxicut-models-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        dir
    }

    fn spec<'a>(urls: &'a [&'a str], sha256: &'a str) -> ModelSpec<'a> {
        ModelSpec {
            id: "test",
            label: "Test model",
            filename: "test.onnx",
            urls,
            sha256,
            approx_bytes: 0,
        }
    }

    #[test]
    fn registry_hashes_look_like_sha256_and_ids_are_unique() {
        for spec in all() {
            assert_eq!(spec.sha256.len(), 64, "{}", spec.id);
            assert!(
                spec.sha256
                    .chars()
                    .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()),
                "{}",
                spec.id
            );
            assert!(
                spec.urls.iter().all(|u| u.starts_with("https://")),
                "{}",
                spec.id
            );
            assert!(find(spec.id).is_some());
        }
        let mut ids: Vec<_> = all().iter().map(|s| s.id).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), all().len());
        assert!(MOBILESAM_ENCODER.urls[0].contains(MOBILESAM_REVISION));
        assert!(MOBILESAM_DECODER.urls[0].contains(MOBILESAM_REVISION));
    }

    #[test]
    fn missing_model_is_downloaded_verified_and_reported() {
        let server = serve();
        let dir = scratch("download");
        let url = format!("{}/good", server.base);
        let urls = [url.as_str()];
        let hash = sha256_bytes(GOOD);
        let spec = spec(&urls, &hash);

        assert_eq!(status(&dir, &spec).unwrap(), Status::Missing);
        let mut seen = Vec::new();
        let path = ensure_model(&dir, &spec, |p| seen.push(p)).unwrap();
        assert_eq!(fs::read(&path).unwrap(), GOOD);
        assert_eq!(status(&dir, &spec).unwrap(), Status::Ready);
        assert!(
            !dir.join("test.onnx.part").exists(),
            "partial file must not linger"
        );
        let last = seen.last().expect("progress reported");
        assert_eq!(last.downloaded, GOOD.len() as u64);
        assert_eq!(last.total, Some(GOOD.len() as u64));
    }

    #[test]
    fn a_verified_model_is_not_downloaded_again() {
        let server = serve();
        let dir = scratch("cached");
        let url = format!("{}/good", server.base);
        let urls = [url.as_str()];
        let hash = sha256_bytes(GOOD);
        let spec = spec(&urls, &hash);
        ensure_model(&dir, &spec, |_| {}).unwrap();
        let before = server.requests.load(Ordering::SeqCst);
        ensure_model(&dir, &spec, |_| {}).unwrap();
        assert_eq!(server.requests.load(Ordering::SeqCst), before);
    }

    #[test]
    fn a_corrupted_file_is_detected_deleted_and_fetched_again() {
        let server = serve();
        let dir = scratch("corrupt");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("test.onnx"), b"half a model").unwrap();
        let url = format!("{}/good", server.base);
        let urls = [url.as_str()];
        let hash = sha256_bytes(GOOD);
        let spec = spec(&urls, &hash);

        assert!(matches!(
            status(&dir, &spec).unwrap(),
            Status::Corrupt { .. }
        ));
        ensure_model(&dir, &spec, |_| {}).unwrap();
        assert_eq!(fs::read(dir.join("test.onnx")).unwrap(), GOOD);
        assert_eq!(status(&dir, &spec).unwrap(), Status::Ready);
    }

    #[test]
    fn a_download_with_the_wrong_hash_never_reaches_the_final_path() {
        let server = serve();
        let dir = scratch("tampered");
        let url = format!("{}/bad", server.base);
        let urls = [url.as_str()];
        let hash = sha256_bytes(GOOD);
        let spec = spec(&urls, &hash);

        let error = ensure_model(&dir, &spec, |_| {}).unwrap_err();
        assert!(error.to_string().contains("checksum mismatch"), "{error}");
        assert!(!dir.join("test.onnx").exists());
        assert!(!dir.join("test.onnx.part").exists());
    }

    #[test]
    fn the_next_mirror_is_tried_when_the_first_fails() {
        let server = serve();
        let dir = scratch("mirror");
        let (missing, tampered, good) = (
            format!("{}/missing", server.base),
            format!("{}/bad", server.base),
            format!("{}/good", server.base),
        );
        let urls = [missing.as_str(), tampered.as_str(), good.as_str()];
        let hash = sha256_bytes(GOOD);
        let spec = spec(&urls, &hash);
        ensure_model(&dir, &spec, |_| {}).unwrap();
        assert_eq!(status(&dir, &spec).unwrap(), Status::Ready);
        assert_eq!(server.requests.load(Ordering::SeqCst), 3);
    }

    #[test]
    fn failing_everywhere_reports_every_attempt_and_leaves_nothing_behind() {
        let server = serve();
        let dir = scratch("allfail");
        let (a, b) = (
            format!("{}/missing", server.base),
            "http://127.0.0.1:1/unreachable".to_string(),
        );
        let urls = [a.as_str(), b.as_str()];
        let spec = spec(&urls, "00");
        let error = ensure_model(&dir, &spec, |_| {}).unwrap_err();
        let message = error.to_string();
        assert!(
            message.contains("Test model")
                && message.contains("404")
                && message.contains("unreachable"),
            "{message}"
        );
        assert_eq!(status(&dir, &spec).unwrap(), Status::Missing);
    }

    #[test]
    fn models_dir_honours_the_override_and_defaults_under_user_data() {
        // SAFETY of env mutation: only this test touches the variable.
        unsafe { std::env::set_var("OXICUT_MODELS_DIR", "somewhere/else") };
        assert_eq!(models_dir().unwrap(), PathBuf::from("somewhere/else"));
        unsafe { std::env::remove_var("OXICUT_MODELS_DIR") };
        let default = models_dir().unwrap();
        assert!(
            default.ends_with("oxicut/models") || default.ends_with("oxicut\\models"),
            "{default:?}"
        );
    }
}
