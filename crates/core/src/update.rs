//! Checks GitHub Releases for a newer Notchat and installs it after
//! verifying the published SHA-256. The only hosts contacted are
//! api.github.com and github.com (plus GitHub's download CDN they redirect to).

use crate::error::{Error, Result};
use sha2::{Digest, Sha256};
use std::path::Path;
use std::time::Duration;

pub const REPO: &str = "Lautaro005/notchat";

#[derive(Debug, Clone, serde::Serialize, PartialEq, Default)]
pub struct Release {
    /// Version without the leading `v`.
    pub version: String,
    pub notes: String,
    pub url: String,
}

#[derive(serde::Deserialize)]
struct GhRelease {
    tag_name: String,
    #[serde(default)]
    body: String,
    html_url: String,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
}

fn client() -> Result<reqwest::Client> {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .user_agent("Notchat-updater")
        .https_only(true)
        .build()
        .map_err(|e| Error::Net(e.to_string()))
}

/// Parses "1.2.3" (optionally "v1.2.3") into numbers. Pre-release suffixes
/// are ignored.
pub fn parse_version(v: &str) -> Option<(u64, u64, u64)> {
    let v = v.trim().trim_start_matches('v');
    let core = v.split(['-', '+']).next()?;
    let mut parts = core.split('.').map(|p| p.parse::<u64>().ok());
    Some((
        parts.next()??,
        parts.next()??,
        parts.next().flatten().unwrap_or(0),
    ))
}

pub fn is_newer(candidate: &str, current: &str) -> bool {
    match (parse_version(candidate), parse_version(current)) {
        (Some(a), Some(b)) => a > b,
        _ => false,
    }
}

/// The latest published release, if it is newer than `current`.
pub async fn check(current: &str) -> Result<Option<Release>> {
    let release: GhRelease = client()?
        .get(format!(
            "https://api.github.com/repos/{REPO}/releases/latest"
        ))
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .map_err(|e| Error::Net(e.to_string()))?
        .error_for_status()
        .map_err(|e| Error::Net(e.to_string()))?
        .json()
        .await
        .map_err(|e| Error::Net(e.to_string()))?;
    if release.draft || release.prerelease || !is_newer(&release.tag_name, current) {
        return Ok(None);
    }
    Ok(Some(Release {
        version: release.tag_name.trim_start_matches('v').to_string(),
        notes: release.body.chars().take(4000).collect(),
        url: release.html_url,
    }))
}

pub fn asset_name(version: &str) -> String {
    format!("Notchat_{version}_universal.app.tar.gz")
}

/// Finds the checksum for `asset` in a `SHA256SUMS` file.
pub fn expected_sum(sums: &str, asset: &str) -> Option<String> {
    sums.lines().find_map(|line| {
        let mut parts = line.split_whitespace();
        let sum = parts.next()?;
        let name = parts.next()?.trim_start_matches('*');
        (name == asset && sum.len() == 64 && sum.bytes().all(|b| b.is_ascii_hexdigit()))
            .then(|| sum.to_ascii_lowercase())
    })
}

async fn get_bytes(url: &str, limit: usize) -> Result<Vec<u8>> {
    let bytes = client()?
        .get(url)
        .send()
        .await
        .map_err(|e| Error::Net(e.to_string()))?
        .error_for_status()
        .map_err(|e| Error::Net(e.to_string()))?
        .bytes()
        .await
        .map_err(|e| Error::Net(e.to_string()))?;
    if bytes.len() > limit {
        return Err(Error::Invalid("download too large".into()));
    }
    Ok(bytes.to_vec())
}

/// Downloads the release archive into `dir`, verifies its SHA-256 against the
/// release's `SHA256SUMS` and unpacks it. Returns the path of the new app.
pub async fn download(version: &str, dir: &Path) -> Result<std::path::PathBuf> {
    if parse_version(version).is_none() {
        return Err(Error::Invalid("version".into()));
    }
    let base = format!("https://github.com/{REPO}/releases/download/v{version}");
    let asset = asset_name(version);
    let sums = get_bytes(&format!("{base}/SHA256SUMS"), 64 * 1024).await?;
    let sums = String::from_utf8_lossy(&sums);
    let expected = expected_sum(&sums, &asset)
        .ok_or_else(|| Error::Blocked("no checksum published".into()))?;
    let archive = get_bytes(&format!("{base}/{asset}"), 400 * 1024 * 1024).await?;
    let actual = hex::encode(Sha256::digest(&archive));
    if actual != expected {
        return Err(Error::Blocked("checksum mismatch".into()));
    }
    std::fs::create_dir_all(dir)?;
    let path = dir.join(&asset);
    std::fs::write(&path, &archive)?;
    let status = tokio::process::Command::new("/usr/bin/tar")
        .arg("-xzf")
        .arg(&path)
        .arg("-C")
        .arg(dir)
        .status()
        .await?;
    let app = dir.join("Notchat.app");
    if !status.success() || !app.join("Contents/Info.plist").exists() {
        return Err(Error::Invalid("update archive".into()));
    }
    Ok(app)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_compare() {
        assert!(is_newer("v0.1.2", "0.1.1"));
        assert!(is_newer("0.2.0", "0.1.9"));
        assert!(is_newer("1.0", "0.9.9"));
        assert!(!is_newer("v0.1.1", "0.1.1"));
        assert!(!is_newer("0.1.0", "0.1.1"));
        assert!(!is_newer("garbage", "0.1.1"));
        assert_eq!(parse_version("v1.2.3-beta.1"), Some((1, 2, 3)));
    }

    #[test]
    fn checksum_lookup() {
        let sums = format!(
            "{}  Notchat_0.1.2_universal.app.tar.gz\n{}  install.sh\n",
            "a".repeat(64),
            "b".repeat(64)
        );
        assert_eq!(
            expected_sum(&sums, &asset_name("0.1.2")),
            Some("a".repeat(64))
        );
        assert_eq!(expected_sum(&sums, "other.tar.gz"), None);
        assert_eq!(
            expected_sum(
                "xyz  Notchat_0.1.2_universal.app.tar.gz",
                &asset_name("0.1.2")
            ),
            None
        );
    }
}
