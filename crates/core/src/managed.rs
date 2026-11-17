//! Notchat's own llama.cpp: downloads a pinned `llama-server` build and a
//! model from the curated catalog (`models.json`, embedded at build time),
//! verifies both with SHA-256 and runs the server on a random loopback port.

use crate::error::{Error, Result};
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio::io::AsyncWriteExt;
use tokio::process::{Child, Command};

const CATALOG_JSON: &str = include_str!("../../../models.json");
/// Only these hosts may serve the server build and models (redirect targets
/// are their own CDNs, which `reqwest` follows over HTTPS).
const ALLOWED_HOSTS: [&str; 2] = ["github.com", "huggingface.co"];

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct Artifact {
    pub url: String,
    pub sha256: String,
    pub size: u64,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct ServerBuild {
    pub version: String,
    pub arm64: Artifact,
    pub x86_64: Artifact,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct ModelEntry {
    pub id: String,
    pub name: String,
    pub note_es: String,
    pub note_en: String,
    pub url: String,
    pub sha256: String,
    pub size: u64,
    pub min_ram_gb: u32,
    #[serde(default)]
    pub recommended: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct Catalog {
    pub llama_server: ServerBuild,
    pub models: Vec<ModelEntry>,
}

pub fn catalog() -> Result<Catalog> {
    let c: Catalog = serde_json::from_str(CATALOG_JSON)?;
    for a in [&c.llama_server.arm64, &c.llama_server.x86_64] {
        check_source(&a.url)?;
    }
    for m in &c.models {
        check_source(&m.url)?;
        if m.id.is_empty()
            || !m
                .id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"-._".contains(&b))
        {
            return Err(Error::Invalid(format!("model id `{}`", m.id)));
        }
    }
    Ok(c)
}

fn check_source(raw: &str) -> Result<()> {
    let url = url::Url::parse(raw).map_err(|_| Error::Invalid("download url".into()))?;
    let host = url.host_str().unwrap_or_default();
    if url.scheme() != "https" || !ALLOWED_HOSTS.contains(&host) {
        return Err(Error::Blocked(format!(
            "downloads from {host} are not allowed"
        )));
    }
    Ok(())
}

/// Where Notchat keeps its llama.cpp files.
#[derive(Debug, Clone)]
pub struct Paths {
    root: PathBuf,
}

impl Paths {
    pub fn new(data_dir: &Path) -> Self {
        Self {
            root: data_dir.join("llama"),
        }
    }
    fn bin_dir(&self) -> PathBuf {
        self.root.join("bin")
    }
    fn models_dir(&self) -> PathBuf {
        self.root.join("models")
    }
    fn marker(&self) -> PathBuf {
        self.root.join("server-version")
    }
    pub fn model_path(&self, m: &ModelEntry) -> PathBuf {
        self.models_dir().join(format!("{}.gguf", m.id))
    }
    pub fn server_binary(&self) -> Option<PathBuf> {
        find_file(&self.bin_dir(), "llama-server", 4)
    }
    pub fn server_installed(&self, build: &ServerBuild) -> bool {
        self.server_binary().is_some()
            && std::fs::read_to_string(self.marker()).is_ok_and(|v| v.trim() == build.version)
    }
    pub fn model_installed(&self, m: &ModelEntry) -> bool {
        std::fs::metadata(self.model_path(m)).is_ok_and(|md| md.len() == m.size)
    }
}

fn find_file(dir: &Path, name: &str, depth: u32) -> Option<PathBuf> {
    let entries = std::fs::read_dir(dir).ok()?;
    let mut dirs = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.file_name().is_some_and(|n| n == name) && path.is_file() {
            return Some(path);
        }
        if path.is_dir() {
            dirs.push(path);
        }
    }
    if depth == 0 {
        return None;
    }
    dirs.into_iter()
        .find_map(|d| find_file(&d, name, depth - 1))
}

pub fn host_artifact(build: &ServerBuild) -> &Artifact {
    if cfg!(target_arch = "x86_64") {
        &build.x86_64
    } else {
        &build.arm64
    }
}

pub type Progress<'a> = &'a (dyn Fn(u64, u64) + Send + Sync);

/// Downloads `artifact` to `dest`, resuming a previous partial download, and
/// verifies its size and SHA-256 before moving it into place.
pub async fn download(artifact: &Artifact, dest: &Path, progress: Progress<'_>) -> Result<()> {
    check_source(&artifact.url)?;
    if let Some(parent) = dest.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    let part = dest.with_extension("part");
    let mut have = tokio::fs::metadata(&part)
        .await
        .map(|m| m.len())
        .unwrap_or(0);
    if have > artifact.size {
        tokio::fs::remove_file(&part).await?;
        have = 0;
    }
    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(15))
        .read_timeout(Duration::from_secs(60))
        .https_only(true)
        .build()
        .map_err(|e| Error::Net(e.to_string()))?;
    if have < artifact.size {
        let mut req = client.get(&artifact.url);
        if have > 0 {
            req = req.header(reqwest::header::RANGE, format!("bytes={have}-"));
        }
        let resp = req.send().await.map_err(|e| Error::Net(e.to_string()))?;
        let status = resp.status();
        if !status.is_success() {
            return Err(Error::Net(format!(
                "download failed: http {}",
                status.as_u16()
            )));
        }
        // A server that ignores the range sends the whole file again.
        let resumed = status == reqwest::StatusCode::PARTIAL_CONTENT;
        if !resumed {
            have = 0;
        }
        let mut file = tokio::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .append(resumed)
            .truncate(!resumed)
            .open(&part)
            .await?;
        let mut stream = resp.bytes_stream();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|e| Error::Net(e.to_string()))?;
            have += chunk.len() as u64;
            if have > artifact.size {
                return Err(Error::Net("download is larger than expected".into()));
            }
            file.write_all(&chunk).await?;
            progress(have, artifact.size);
        }
        file.flush().await?;
    }
    if have != artifact.size {
        return Err(Error::Net(
            "download incomplete, try again to resume".into(),
        ));
    }
    let digest = sha256_file(&part).await?;
    if !digest.eq_ignore_ascii_case(&artifact.sha256) {
        let _ = tokio::fs::remove_file(&part).await;
        return Err(Error::Blocked(
            "checksum mismatch: the file was discarded".into(),
        ));
    }
    tokio::fs::rename(&part, dest).await?;
    Ok(())
}

pub async fn sha256_file(path: &Path) -> Result<String> {
    let path = path.to_path_buf();
    tokio::task::spawn_blocking(move || -> Result<String> {
        use std::io::Read;
        let mut file = std::fs::File::open(path)?;
        let mut hasher = Sha256::new();
        let mut buf = vec![0u8; 1 << 20];
        loop {
            let n = file.read(&mut buf)?;
            if n == 0 {
                break;
            }
            hasher.update(&buf[..n]);
        }
        Ok(hex::encode(hasher.finalize()))
    })
    .await
    .map_err(|e| Error::Io(std::io::Error::other(e)))?
}

/// Downloads, verifies and unpacks the pinned `llama-server` build.
pub async fn install_server(
    paths: &Paths,
    build: &ServerBuild,
    progress: Progress<'_>,
) -> Result<()> {
    if paths.server_installed(build) {
        return Ok(());
    }
    let artifact = host_artifact(build);
    let archive = paths.root.join(format!("llama-{}.tar.gz", build.version));
    download(artifact, &archive, progress).await?;
    let bin = paths.bin_dir();
    if bin.exists() {
        tokio::fs::remove_dir_all(&bin).await?;
    }
    tokio::fs::create_dir_all(&bin).await?;
    let status = Command::new("/usr/bin/tar")
        .arg("-xzf")
        .arg(&archive)
        .arg("-C")
        .arg(&bin)
        .status()
        .await?;
    let _ = tokio::fs::remove_file(&archive).await;
    if !status.success() {
        return Err(Error::Io(std::io::Error::other(
            "could not unpack llama-server",
        )));
    }
    if paths.server_binary().is_none() {
        return Err(Error::Io(std::io::Error::other(
            "llama-server missing from the archive",
        )));
    }
    tokio::fs::write(paths.marker(), &build.version).await?;
    Ok(())
}

pub async fn install_model(
    paths: &Paths,
    model: &ModelEntry,
    progress: Progress<'_>,
) -> Result<()> {
    if paths.model_installed(model) {
        return Ok(());
    }
    let artifact = Artifact {
        url: model.url.clone(),
        sha256: model.sha256.clone(),
        size: model.size,
    };
    download(&artifact, &paths.model_path(model), progress).await
}

/// A running `llama-server`. Killed when dropped.
pub struct Server {
    child: Child,
    pub base_url: String,
    pub model_id: String,
}

impl Server {
    pub async fn stop(mut self) {
        let _ = self.child.kill().await;
    }

    pub fn is_running(&mut self) -> bool {
        matches!(self.child.try_wait(), Ok(None))
    }
}

fn free_port() -> Result<u16> {
    let listener = std::net::TcpListener::bind("127.0.0.1:0")?;
    Ok(listener.local_addr()?.port())
}

/// Starts the server for `model` on a free loopback port and waits until it
/// answers its health check (model loaded).
pub async fn start(paths: &Paths, model: &ModelEntry) -> Result<Server> {
    let binary = paths
        .server_binary()
        .ok_or_else(|| Error::Ai("llama.cpp is not installed".into()))?;
    if !paths.model_installed(model) {
        return Err(Error::Ai("the model is not downloaded".into()));
    }
    let port = free_port()?;
    let mut cmd = Command::new(&binary);
    cmd.arg("--host")
        .arg("127.0.0.1")
        .arg("--port")
        .arg(port.to_string())
        .arg("-m")
        .arg(paths.model_path(model))
        .arg("--alias")
        .arg(&model.id)
        .arg("--jinja")
        .arg("-c")
        .arg("16384")
        .arg("--no-webui")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(true);
    if let Some(dir) = binary.parent() {
        // The build ships its dylibs next to the binary.
        cmd.current_dir(dir).env("DYLD_LIBRARY_PATH", dir);
    }
    let mut child = cmd.spawn()?;
    let base_url = format!("http://127.0.0.1:{port}");
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(2))
        .no_proxy()
        .build()
        .map_err(|e| Error::Ai(e.to_string()))?;
    for _ in 0..240 {
        if let Ok(Some(status)) = child.try_wait() {
            return Err(Error::Ai(format!("llama-server exited ({status})")));
        }
        let healthy = client
            .get(format!("{base_url}/health"))
            .send()
            .await
            .is_ok_and(|r| r.status().is_success());
        if healthy {
            return Ok(Server {
                child,
                base_url,
                model_id: model.id.clone(),
            });
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
    let _ = child.kill().await;
    Err(Error::Ai(
        "llama-server did not become ready in time".into(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_catalog_is_valid() {
        let c = catalog().unwrap();
        assert!(!c.models.is_empty());
        assert_eq!(c.models.iter().filter(|m| m.recommended).count(), 1);
        for m in &c.models {
            assert_eq!(m.sha256.len(), 64, "{}", m.id);
            assert!(m.size > 0);
        }
        assert_eq!(c.llama_server.arm64.sha256.len(), 64);
    }

    #[test]
    fn only_trusted_hosts() {
        assert!(check_source("https://huggingface.co/a/b/resolve/main/x.gguf").is_ok());
        assert!(check_source("https://github.com/ggml-org/llama.cpp/releases/x").is_ok());
        assert!(check_source("http://huggingface.co/x").is_err());
        assert!(check_source("https://evil.example/x.gguf").is_err());
        assert!(check_source("https://huggingface.co.evil.example/x").is_err());
    }

    #[tokio::test]
    async fn checksum_and_lookup() {
        let dir = tempfile::tempdir().unwrap();
        let f = dir.path().join("a");
        std::fs::write(&f, b"abc").unwrap();
        assert_eq!(
            sha256_file(&f).await.unwrap(),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        let nested = dir.path().join("x/y");
        std::fs::create_dir_all(&nested).unwrap();
        std::fs::write(nested.join("llama-server"), b"").unwrap();
        assert!(find_file(dir.path(), "llama-server", 3).is_some());
        assert!(find_file(dir.path(), "llama-server", 0).is_none());
    }
}

#[cfg(test)]
mod network_tests {
    use super::*;

    /// Downloads the real pinned build. Run with `cargo test -- --ignored`.
    #[tokio::test]
    #[ignore]
    async fn installs_and_runs_pinned_server() {
        let dir = tempfile::tempdir().unwrap();
        let paths = Paths::new(dir.path());
        let build = catalog().unwrap().llama_server;
        install_server(&paths, &build, &|_, _| {}).await.unwrap();
        assert!(paths.server_installed(&build));
        let bin = paths.server_binary().unwrap();
        let out = Command::new(&bin)
            .arg("--version")
            .current_dir(bin.parent().unwrap())
            .env("DYLD_LIBRARY_PATH", bin.parent().unwrap())
            .output()
            .await
            .unwrap();
        let text = String::from_utf8_lossy(&out.stderr).to_string()
            + &String::from_utf8_lossy(&out.stdout);
        assert!(text.contains("12441"), "{text}");
    }
}
