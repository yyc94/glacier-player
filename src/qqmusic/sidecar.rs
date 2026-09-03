// SPDX-License-Identifier: GPL-3.0-only

//! Lifecycle management for the bundled QQMusicApi service.

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;

use reqwest::Url;
use serde::Deserialize;
use tokio::process::{Child, Command};
use tokio::sync::Mutex;

use super::client::{QqMusicError, QqResult};

const SIDECAR_BINARY: &str = "glacier-qqmusic-api";
const SIDECAR_OVERRIDE: &str = "GLACIER_QQMUSIC_API_BIN";
const START_ATTEMPTS: usize = 80;
const START_POLL_INTERVAL: Duration = Duration::from_millis(100);
const HEALTH_TIMEOUT: Duration = Duration::from_millis(500);

#[derive(Clone)]
pub(super) struct QqMusicSidecar {
    base_url: Url,
    managed: bool,
    state: Arc<Mutex<SidecarState>>,
}

#[derive(Default)]
struct SidecarState {
    child: Option<SidecarChild>,
    ready: bool,
}

struct SidecarChild {
    child: Child,
    process_group: i32,
}

impl SidecarChild {
    fn try_wait(&mut self) -> std::io::Result<Option<std::process::ExitStatus>> {
        self.child.try_wait()
    }

    fn start_kill(&mut self) -> std::io::Result<()> {
        self.child.start_kill()
    }
}

impl Drop for SidecarChild {
    fn drop(&mut self) {
        // PyInstaller's one-file launcher forks the actual application. Both
        // processes inherit this group, so terminating only `Child` would
        // leave the HTTP server running after Glacier exits.
        // SAFETY: `process_group` is the positive PID returned for a child
        // spawned with `process_group(0)`; negating it targets that group only.
        unsafe { libc::kill(-self.process_group, libc::SIGKILL) };
    }
}

#[derive(Deserialize)]
struct HealthResponse {
    code: i32,
    msg: String,
}

impl QqMusicSidecar {
    pub(super) fn new(base_url: &Url) -> Self {
        Self { base_url: base_url.clone(), managed: is_managed_url(base_url), state: Arc::default() }
    }

    pub(super) fn is_managed(&self) -> bool {
        self.managed
    }

    /// Ensure the bundled service is accepting requests. A service already
    /// listening on the default endpoint is reused and left running on exit.
    pub(super) async fn ensure_ready(&self, http: &reqwest::Client) -> QqResult<()> {
        if !self.managed {
            return Ok(());
        }

        let mut state = self.state.lock().await;
        if state.ready {
            return Ok(());
        }
        if endpoint_is_healthy(http, &self.base_url).await {
            state.ready = true;
            return Ok(());
        }

        if let Some(child) = state.child.as_mut() {
            match child.try_wait() {
                Ok(Some(status)) => {
                    tracing::warn!("bundled QQMusicApi exited before becoming ready: {status}");
                    state.child = None;
                }
                Ok(None) => {}
                Err(error) => {
                    tracing::warn!("could not inspect bundled QQMusicApi process: {error}");
                    state.child = None;
                }
            }
        }

        if state.child.is_none() {
            let executable = sidecar_executable();
            let data_dir = sidecar_data_dir()?;
            tokio::fs::create_dir_all(&data_dir)
                .await
                .map_err(|error| QqMusicError::Sidecar(format!("could not create {}: {error}", data_dir.display())))?;
            state.child = Some(spawn_sidecar(&executable, &self.base_url, &data_dir)?);
            tracing::info!(executable = %executable.display(), "started bundled QQ Music backend");
        }

        for _ in 0..START_ATTEMPTS {
            if endpoint_is_healthy(http, &self.base_url).await {
                state.ready = true;
                return Ok(());
            }
            if let Some(child) = state.child.as_mut()
                && let Some(status) = child
                    .try_wait()
                    .map_err(|error| QqMusicError::Sidecar(format!("could not inspect backend process: {error}")))?
            {
                state.child = None;
                return Err(QqMusicError::Sidecar(format!("bundled backend exited during startup ({status})")));
            }
            tokio::time::sleep(START_POLL_INTERVAL).await;
        }

        if let Some(child) = state.child.as_mut() {
            let _ = child.start_kill();
        }
        state.child = None;
        Err(QqMusicError::Sidecar(format!("bundled backend did not become ready at {}", self.base_url)))
    }

    pub(super) async fn mark_unavailable(&self) {
        if self.managed {
            self.state.lock().await.ready = false;
        }
    }
}

fn is_managed_url(url: &Url) -> bool {
    if url.scheme() != "http" || url.port_or_known_default() != Some(8080) {
        return false;
    }
    matches!(url.host_str(), Some("127.0.0.1" | "localhost" | "::1" | "[::1]"))
}

fn sidecar_executable() -> PathBuf {
    if let Some(path) = std::env::var_os(SIDECAR_OVERRIDE) {
        return PathBuf::from(path);
    }
    if let Ok(current) = std::env::current_exe()
        && let Some(directory) = current.parent()
    {
        let sibling = directory.join(SIDECAR_BINARY);
        if sibling.is_file() {
            return sibling;
        }
    }
    PathBuf::from(SIDECAR_BINARY)
}

fn sidecar_data_dir() -> QqResult<PathBuf> {
    dirs::data_local_dir()
        .map(|directory| directory.join("glacier-player").join("qqmusic-api"))
        .ok_or_else(|| QqMusicError::Sidecar("could not determine the user data directory".into()))
}

fn spawn_sidecar(executable: &Path, base_url: &Url, data_dir: &Path) -> QqResult<SidecarChild> {
    let port = base_url.port_or_known_default().unwrap_or(8080);
    let device_path = data_dir.join("device.json").to_string_lossy().into_owned();
    let credential_path = data_dir.join("credentials.sqlite3").to_string_lossy().into_owned();
    let client_config = serde_json::json!({ "device_path": device_path }).to_string();
    let credential_config = serde_json::json!({ "store": { "path": credential_path } }).to_string();

    let mut command = Command::new(executable);
    command
        .kill_on_drop(true)
        .process_group(0)
        .env("QQMUSIC_SERVER_HOST", sidecar_host(base_url))
        .env("QQMUSIC_SERVER_PORT", port.to_string())
        .env("QQMUSIC_CLIENT", client_config)
        .env("QQMUSIC_CREDENTIAL", credential_config)
        .env("QQMUSIC_LOGGING", r#"{"mode":"console","level":"ERROR"}"#)
        .stdin(Stdio::null())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());
    let child = command.spawn().map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            QqMusicError::Sidecar("bundled backend is missing; reinstall Glacier Player".into())
        } else {
            QqMusicError::Sidecar(format!("could not start {}: {error}", executable.display()))
        }
    })?;
    let process_group = child
        .id()
        .and_then(|pid| i32::try_from(pid).ok())
        .ok_or_else(|| QqMusicError::Sidecar("could not determine the backend process ID".into()))?;
    Ok(SidecarChild { child, process_group })
}

fn sidecar_host(base_url: &Url) -> &'static str {
    match base_url.host_str() {
        Some("::1" | "[::1]") => "::1",
        _ => "127.0.0.1",
    }
}

async fn endpoint_is_healthy(http: &reqwest::Client, base_url: &Url) -> bool {
    let request = async {
        let response = http.get(base_url.clone()).send().await.ok()?;
        if !response.status().is_success() {
            return None;
        }
        response.json::<HealthResponse>().await.ok()
    };
    matches!(tokio::time::timeout(HEALTH_TIMEOUT, request).await, Ok(Some(HealthResponse { code: 0, msg })) if msg == "ok")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_default_loopback_endpoint_is_managed() {
        for managed in ["http://127.0.0.1:8080", "http://localhost:8080", "http://[::1]:8080/"] {
            assert!(is_managed_url(&Url::parse(managed).unwrap()), "{managed}");
        }
        for external in ["https://example.com", "http://127.0.0.1:9000", "https://127.0.0.1:8080"] {
            assert!(!is_managed_url(&Url::parse(external).unwrap()), "{external}");
        }
    }

    #[test]
    fn sidecar_listens_on_the_requested_ip_family() {
        assert_eq!(sidecar_host(&Url::parse("http://127.0.0.1:8080").unwrap()), "127.0.0.1");
        assert_eq!(sidecar_host(&Url::parse("http://localhost:8080").unwrap()), "127.0.0.1");
        assert_eq!(sidecar_host(&Url::parse("http://[::1]:8080").unwrap()), "::1");
    }
}
