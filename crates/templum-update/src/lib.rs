//! Templum's updates: the newest release on GitHub, the file in it made for this system and for
//! the way Templum was installed, downloaded, checked against GitHub's checksum and put in place.
//! No UI: see docs/specs/updates.md.

use semver::Version;
use serde::Deserialize;

mod fetch;
mod install;

pub use fetch::{agent, download, latest};
pub use install::{Install, Why, detect_in};

/// Where releases are published.
pub const REPO: &str = "KingDasWinx/Templum";

/// The running version (every crate shares the workspace's).
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// A published release.
#[derive(Clone, Debug, PartialEq)]
pub struct Release {
    pub version: Version,
    /// Its page on GitHub: the notes and every file.
    pub page: String,
    pub assets: Vec<Asset>,
}

/// One of a release's files.
#[derive(Clone, Debug, PartialEq)]
pub struct Asset {
    pub name: String,
    pub url: String,
    pub size: u64,
    /// The SHA-256 GitHub computed on upload, in lowercase hex.
    pub sha256: Option<String>,
}

impl Release {
    /// Whether it is newer than `current` (a version in Cargo's form). A version that can't be
    /// read is never older.
    pub fn newer_than(&self, current: &str) -> bool {
        Version::parse(current).is_ok_and(|current| self.version > current)
    }

    pub(crate) fn asset(&self, name: &str) -> Result<&Asset, String> {
        self.assets.iter().find(|a| a.name == name).ok_or_else(|| {
            format!(
                "Templum {} has no file for this system ({name})",
                self.version
            )
        })
    }
}

/// GitHub's answer about a release (`GET /repos/{repo}/releases/latest`).
pub fn parse(json: &str) -> Result<Release, String> {
    #[derive(Deserialize)]
    struct Answer {
        tag_name: String,
        html_url: String,
        assets: Vec<File>,
    }
    #[derive(Deserialize)]
    struct File {
        name: String,
        browser_download_url: String,
        size: u64,
        #[serde(default)]
        digest: Option<String>,
        #[serde(default)]
        state: Option<String>,
    }
    let answer: Answer =
        serde_json::from_str(json).map_err(|e| format!("GitHub's answer is unexpected: {e}"))?;
    let tag = answer.tag_name.trim_start_matches('v');
    let version = Version::parse(tag)
        .map_err(|e| format!("The newest release's tag ({tag}) isn't a version: {e}"))?;
    let assets = answer
        .assets
        .into_iter()
        // A file still uploading is "starter".
        .filter(|f| f.state.as_deref().is_none_or(|s| s == "uploaded"))
        .map(|f| Asset {
            sha256: f
                .digest
                .and_then(|d| d.strip_prefix("sha256:").map(str::to_ascii_lowercase)),
            name: f.name,
            url: f.browser_download_url,
            size: f.size,
        })
        .collect();
    Ok(Release {
        version,
        page: answer.html_url,
        assets,
    })
}

#[cfg(test)]
pub(crate) fn fixture() -> Release {
    parse(include_str!("fixtures/release-v0.1.1.json")).unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_real_answer_parses() {
        let release = fixture();
        assert_eq!(release.version, Version::new(0, 1, 1));
        assert_eq!(
            release.page,
            "https://github.com/KingDasWinx/Templum/releases/tag/v0.1.1"
        );
        assert_eq!(release.assets.len(), 26);
        let deb = release.asset("templum_0.1.1_amd64.deb").unwrap();
        assert_eq!(deb.size, 19_450_910);
        assert_eq!(
            deb.url,
            "https://github.com/KingDasWinx/Templum/releases/download/v0.1.1/templum_0.1.1_amd64.deb"
        );
        assert_eq!(deb.sha256.as_deref().map(str::len), Some(64));
    }

    #[test]
    fn files_still_uploading_and_other_digests_are_left_out() {
        let release = parse(
            r#"{"tag_name": "v1.2.0-rc.1", "html_url": "page", "assets": [
                {"name": "a", "browser_download_url": "u", "size": 1, "state": "starter"},
                {"name": "b", "browser_download_url": "u", "size": 1, "digest": "sha512:AB"},
                {"name": "c", "browser_download_url": "u", "size": 1, "digest": "sha256:AB"}
            ]}"#,
        )
        .unwrap();
        assert_eq!(release.version, Version::parse("1.2.0-rc.1").unwrap());
        let names: Vec<_> = release.assets.iter().map(|a| a.name.as_str()).collect();
        assert_eq!(names, ["b", "c"]);
        assert_eq!(release.assets[0].sha256, None);
        assert_eq!(release.assets[1].sha256.as_deref(), Some("ab"));
        assert!(parse(r#"{"tag_name": "nightly", "html_url": "", "assets": []}"#).is_err());
    }

    #[test]
    fn newer_follows_semver() {
        let release = |v: &str| Release {
            version: Version::parse(v).unwrap(),
            page: String::new(),
            assets: Vec::new(),
        };
        assert!(release("0.2.0").newer_than("0.1.1"));
        assert!(release("0.2.0").newer_than("0.2.0-rc.1"));
        assert!(release("0.10.0").newer_than("0.9.9"));
        assert!(!release("0.1.1").newer_than("0.1.1"));
        assert!(!release("0.1.0").newer_than("0.1.1"));
        assert!(!release("9.0.0").newer_than("not a version"));
    }
}
