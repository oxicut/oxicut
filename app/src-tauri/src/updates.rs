//! The version check: two plain requests, nothing sent about the person or their scans.
//!
//! The list of releases (to see whether a newer version exists) and the changelog (to say what it
//! brings). Which release is newest and which changelog entries are new is worked out on the screen,
//! where it is tested without a window. This file only fetches, and never from an address the screen
//! hands it. Every failure is an error the caller turns into silence.

use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::settings;

const RELEASES_URL: &str = "https://api.github.com/repos/oxicut/oxicut/releases?per_page=20";
const CHANGELOG_URL: &str = "https://raw.githubusercontent.com/oxicut/oxicut/main/CHANGELOG.md";
/// For tests and for trying the feature without publishing anything: `<base>/releases.json` and
/// `<base>/CHANGELOG.md` are asked for instead of GitHub.
pub const URL_ENV: &str = "OXICUT_UPDATE_URL";
/// Set to `1` to never look for a newer version (scratch runs, offline demos).
pub const DISABLE_ENV: &str = "OXICUT_NO_UPDATE_CHECK";

/// One release, as the screen needs it.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Release {
    /// The tag, for example `v0.1.0`.
    pub version: String,
    pub name: String,
    /// The release notes.
    pub notes: String,
    /// The release page.
    pub url: String,
    pub published_at: String,
    pub prerelease: bool,
}

/// The fields of GitHub's answer that are used.
#[derive(Deserialize)]
struct GithubRelease {
    tag_name: String,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    body: Option<String>,
    html_url: String,
    #[serde(default)]
    published_at: Option<String>,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
}

/// Where the releases and the changelog are asked for, given the override (if any).
pub fn endpoints_for(base: Option<&str>) -> (String, String) {
    match base.map(|b| b.trim_end_matches('/')).filter(|b| !b.is_empty()) {
        Some(base) => (format!("{base}/releases.json"), format!("{base}/CHANGELOG.md")),
        None => (RELEASES_URL.to_string(), CHANGELOG_URL.to_string()),
    }
}

pub fn endpoints() -> (String, String) {
    endpoints_for(std::env::var(URL_ENV).ok().as_deref())
}

/// Whether the app may look for a newer version by itself: the setting is on, the environment does not
/// say no, and this is not a test run.
pub fn enabled() -> bool {
    !cfg!(test) && std::env::var(DISABLE_ENV).as_deref() != Ok("1") && settings::load().check_for_updates
}

fn client() -> Result<reqwest::blocking::Client, String> {
    reqwest::blocking::Client::builder()
        .connect_timeout(Duration::from_secs(5))
        .timeout(Duration::from_secs(10))
        .user_agent(format!("OxiCut/{}", oxicut_core::VERSION))
        .build()
        .map_err(|e| e.to_string())
}

/// The releases at `url` (drafts left out), newest first as GitHub lists them.
pub fn fetch_releases(url: &str) -> Result<Vec<Release>, String> {
    let response = client()?
        .get(url)
        .header("Accept", "application/vnd.github+json")
        .send()
        .map_err(|e| e.to_string())?;
    if !response.status().is_success() {
        return Err(format!("answered {}", response.status()));
    }
    let text = response.text().map_err(|e| e.to_string())?;
    parse_releases(&text)
}

pub fn parse_releases(json: &str) -> Result<Vec<Release>, String> {
    let list: Vec<GithubRelease> = serde_json::from_str(json).map_err(|e| e.to_string())?;
    Ok(list
        .into_iter()
        .filter(|r| !r.draft)
        .map(|r| Release {
            name: r.name.filter(|n| !n.is_empty()).unwrap_or_else(|| r.tag_name.clone()),
            version: r.tag_name,
            notes: r.body.unwrap_or_default(),
            url: r.html_url,
            published_at: r.published_at.unwrap_or_default(),
            prerelease: r.prerelease,
        })
        .collect())
}

/// The changelog at `url`, or `None` when there is none (not found).
pub fn fetch_changelog(url: &str) -> Result<Option<String>, String> {
    let response = client()?.get(url).send().map_err(|e| e.to_string())?;
    if response.status() == reqwest::StatusCode::NOT_FOUND {
        return Ok(None);
    }
    if !response.status().is_success() {
        return Err(format!("answered {}", response.status()));
    }
    response.text().map(Some).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;

    /// A server on a free local port that answers `routes` (path, status, body) once each, in any order.
    fn serve(routes: Vec<(&'static str, u16, String)>) -> (String, std::thread::JoinHandle<Vec<String>>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let handle = std::thread::spawn(move || {
            let mut asked = Vec::new();
            for _ in 0..routes.len() {
                let (mut stream, _) = listener.accept().unwrap();
                let mut buffer = [0u8; 4096];
                let n = stream.read(&mut buffer).unwrap();
                let request = String::from_utf8_lossy(&buffer[..n]).into_owned();
                let path = request.split_whitespace().nth(1).unwrap_or("").to_string();
                asked.push(request.clone());
                let (status, body) = routes
                    .iter()
                    .find(|(p, _, _)| *p == path)
                    .map(|(_, s, b)| (*s, b.clone()))
                    .unwrap_or((404, String::new()));
                let reply = format!(
                    "HTTP/1.1 {status} X\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                stream.write_all(reply.as_bytes()).unwrap();
            }
            asked
        });
        (base, handle)
    }

    const LIST: &str = r#"[
        {"tag_name":"v0.2.0","name":"OxiCut 0.2.0","body":"- Newer","html_url":"https://github.com/oxicut/oxicut/releases/tag/v0.2.0","published_at":"2026-12-01T10:00:00Z","draft":false,"prerelease":true},
        {"tag_name":"v0.3.0","name":"Secret","body":"x","html_url":"https://example.invalid","draft":true,"prerelease":false},
        {"tag_name":"v0.1.0","name":null,"body":null,"html_url":"https://github.com/oxicut/oxicut/releases/tag/v0.1.0","published_at":null,"draft":false,"prerelease":false}
    ]"#;

    #[test]
    fn the_release_list_is_read_and_drafts_are_left_out() {
        let releases = parse_releases(LIST).unwrap();
        assert_eq!(releases.len(), 2);
        assert_eq!(releases[0].version, "v0.2.0");
        assert_eq!(releases[0].notes, "- Newer");
        assert!(releases[0].prerelease);
        assert_eq!(releases[1].name, "v0.1.0", "no name: the tag stands in");
        assert_eq!(releases[1].notes, "");
        assert!(parse_releases("{ nope").is_err());
        assert!(parse_releases("{}").is_err(), "an object is not a list");
    }

    #[test]
    fn the_override_replaces_both_addresses_and_nothing_else_does() {
        let (releases, changelog) = endpoints_for(Some("http://127.0.0.1:8000/"));
        assert_eq!(releases, "http://127.0.0.1:8000/releases.json");
        assert_eq!(changelog, "http://127.0.0.1:8000/CHANGELOG.md");
        for none in [None, Some(""), Some("/")] {
            let (releases, changelog) = endpoints_for(none);
            assert!(releases.starts_with("https://api.github.com/repos/oxicut/oxicut/releases"));
            assert!(changelog.starts_with("https://raw.githubusercontent.com/oxicut/oxicut/"));
        }
    }

    #[test]
    fn releases_and_changelog_are_fetched_with_only_a_user_agent_and_an_accept() {
        let (base, server) = serve(vec![
            ("/releases.json", 200, LIST.to_string()),
            ("/CHANGELOG.md", 200, "## [0.2.0]\n- Newer\n".to_string()),
        ]);
        let (releases_url, changelog_url) = endpoints_for(Some(&base));
        let releases = fetch_releases(&releases_url).unwrap();
        assert_eq!(releases.len(), 2);
        let changelog = fetch_changelog(&changelog_url).unwrap();
        assert_eq!(changelog.as_deref(), Some("## [0.2.0]\n- Newer\n"));

        let asked = server.join().unwrap();
        for request in &asked {
            let lower = request.to_lowercase();
            assert!(lower.contains(&format!("user-agent: oxicut/{}", oxicut_core::VERSION).to_lowercase()));
            // Nothing that identifies the person: no cookies, no authorisation, no custom headers.
            for header in ["cookie", "authorization", "x-"] {
                assert!(!lower.contains(&format!("\r\n{header}")), "{header} was sent: {request}");
            }
        }
    }

    #[test]
    fn a_missing_changelog_is_none_and_a_failing_server_is_an_error() {
        let (base, server) = serve(vec![
            ("/CHANGELOG.md", 404, "not found".to_string()),
            ("/releases.json", 500, "oops".to_string()),
        ]);
        let (releases_url, changelog_url) = endpoints_for(Some(&base));
        assert_eq!(fetch_changelog(&changelog_url).unwrap(), None);
        assert!(fetch_releases(&releases_url).is_err());
        server.join().unwrap();
    }

    #[test]
    fn nobody_listening_is_an_error_not_a_hang() {
        // A port that was just free and has nobody on it.
        let port = TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
        let started = std::time::Instant::now();
        assert!(fetch_releases(&format!("http://127.0.0.1:{port}/releases.json")).is_err());
        assert!(started.elapsed() < Duration::from_secs(12));
    }

    #[test]
    fn tests_never_look_for_updates_by_themselves() {
        assert!(!enabled());
    }
}
