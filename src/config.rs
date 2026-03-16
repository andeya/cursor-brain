//! Configuration: single source of defaults; load from ~/.cursor-brain/config.json only (no env vars).
//! Keys: port, bind_address, request_timeout_sec, session, default_model, fallback_model,
//! cursor_path, minimal_workspace_dir, sandbox, forward_thinking.
//! On first run (no config file), the default config is written to disk.

use std::path::PathBuf;

/// Serializable view of config for writing to config.json (all keys included for defaults).
#[derive(serde::Serialize)]
struct ConfigFileExport {
    cursor_path: Option<String>,
    port: u16,
    bind_address: String,
    request_timeout_sec: u64,
    session_cache_max: u32,
    session_header_name: String,
    default_model: Option<String>,
    fallback_model: Option<String>,
    minimal_workspace_dir: Option<String>,
    sandbox: String,
    forward_thinking: String,
}

impl From<&Config> for ConfigFileExport {
    fn from(c: &Config) -> Self {
        ConfigFileExport {
            cursor_path: c.cursor_path.clone(),
            port: c.port,
            bind_address: c.bind_address.clone(),
            request_timeout_sec: c.request_timeout_sec,
            session_cache_max: c.session_cache_max,
            session_header_name: c.session_header_name.clone(),
            default_model: c.default_model.clone(),
            fallback_model: c.fallback_model.clone(),
            minimal_workspace_dir: c.minimal_workspace_dir.clone(),
            sandbox: c.sandbox.clone(),
            forward_thinking: c.forward_thinking.clone(),
        }
    }
}

/// Deserializable config file shape; missing keys yield None / default in load_config.
#[derive(Debug, serde::Deserialize)]
#[serde(default)]
struct ConfigFileLoad {
    cursor_path: Option<String>,
    port: Option<u16>,
    bind_address: Option<String>,
    request_timeout_sec: Option<u64>,
    session_cache_max: Option<u32>,
    session_header_name: Option<String>,
    default_model: Option<String>,
    fallback_model: Option<String>,
    minimal_workspace_dir: Option<String>,
    sandbox: Option<String>,
    forward_thinking: Option<String>,
}

impl Default for ConfigFileLoad {
    fn default() -> Self {
        Self {
            cursor_path: None,
            port: None,
            bind_address: None,
            request_timeout_sec: None,
            session_cache_max: None,
            session_header_name: None,
            default_model: None,
            fallback_model: None,
            minimal_workspace_dir: None,
            sandbox: None,
            forward_thinking: None,
        }
    }
}

/// Default port for the HTTP server.
pub const DEFAULT_PORT: u16 = 3001;
/// Default request timeout in seconds.
pub const DEFAULT_REQUEST_TIMEOUT_SEC: u64 = 300;
/// Default session cache capacity (LRU).
pub const DEFAULT_SESSION_CACHE_MAX: u32 = 1000;
/// Default HTTP header name for external session id.
pub const DEFAULT_SESSION_HEADER_NAME: &str = "x-session-id";
/// Default model ids when cursor-agent --list-models fails.
pub const DEFAULT_MODELS_LIST: &[&str] = &["auto", "cursor-default"];

/// Config directory and data root (not configurable).
pub fn cursor_brain_home_dir() -> PathBuf {
    dirs::home_dir().unwrap_or_else(|| PathBuf::from("."))
}

/// Default session persistence file under ~/.cursor-brain/.
pub fn default_session_file_path() -> String {
    cursor_brain_home_dir()
        .join(".cursor-brain")
        .join("sessions.json")
        .to_string_lossy()
        .into_owned()
}

/// Default minimal workspace dir (no .cursor/mcp.json) for spawn.
pub fn default_minimal_workspace_dir() -> String {
    cursor_brain_home_dir()
        .join(".cursor-brain")
        .join("workspace")
        .to_string_lossy()
        .into_owned()
}

/// PID file path for single-instance detection and monitoring.
pub fn pid_file_path() -> std::path::PathBuf {
    cursor_brain_home_dir()
        .join(".cursor-brain")
        .join("cursor-brain.pid")
}

#[derive(Clone, Debug)]
pub struct Config {
    pub cursor_path: Option<String>,
    pub port: u16,
    /// Bind address, e.g. "0.0.0.0" or "127.0.0.1".
    pub bind_address: String,
    pub request_timeout_sec: u64,
    pub session_cache_max: u32,
    pub session_header_name: String,
    pub default_model: Option<String>,
    pub fallback_model: Option<String>,
    /// Workspace dir for cursor-agent (e.g. ~/.cursor-brain/workspace); no project MCP.
    pub minimal_workspace_dir: Option<String>,
    /// "enabled" | "disabled"
    pub sandbox: String,
    /// How to return thinking: "off" | "content" | "reasoning_content". Default "content".
    pub forward_thinking: String,
}

impl Config {
    pub fn resolve_cursor_path(&self) -> Option<String> {
        if let Some(ref p) = self.cursor_path {
            if !p.is_empty() && std::path::Path::new(p).exists() {
                return Some(p.clone());
            }
        }
        detect_cursor_path()
    }

    /// Resolved workspace dir for spawn (minimal_workspace_dir or default).
    pub fn workspace_dir_for_spawn(&self) -> Option<String> {
        self.minimal_workspace_dir
            .as_deref()
            .filter(|s| !s.is_empty())
            .map(String::from)
            .or_else(|| Some(default_minimal_workspace_dir()))
    }
}

fn cursor_search_paths() -> Vec<PathBuf> {
    let home = cursor_brain_home_dir();
    #[cfg(windows)]
    {
        let local: PathBuf = std::env::var("LOCALAPPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(|_| home.join("AppData").join("Local"));
        vec![
            local
                .join("Programs")
                .join("cursor")
                .join("resources")
                .join("app")
                .join("bin")
                .join("agent.exe"),
            local.join("cursor-agent").join("agent.cmd"),
            home.join(".cursor").join("bin").join("agent.exe"),
            home.join(".cursor").join("bin").join("agent.cmd"),
            home.join(".local").join("bin").join("agent.exe"),
        ]
    }
    #[cfg(not(windows))]
    {
        vec![
            home.join(".local").join("bin").join("agent"),
            PathBuf::from("/usr/local/bin/agent"),
            home.join(".cursor").join("bin").join("agent"),
        ]
    }
}

fn detect_cursor_path() -> Option<String> {
    #[cfg(windows)]
    let out = std::process::Command::new("where")
        .args(["agent"])
        .output()
        .ok();
    #[cfg(not(windows))]
    let out = std::process::Command::new("which")
        .arg("agent")
        .output()
        .ok();

    if let Some(ref o) = out {
        if o.status.success() {
            let s = String::from_utf8_lossy(&o.stdout);
            let first = s.lines().next()?.trim();
            if !first.is_empty() && std::path::Path::new(first).exists() {
                return Some(first.to_string());
            }
        }
    }
    for p in cursor_search_paths() {
        if p.exists() {
            return Some(p.to_string_lossy().into_owned());
        }
    }
    None
}

/// Writes the given config to ~/.cursor-brain/config.json. Creates parent directory if needed.
/// Used when the config file was missing on first run so the user gets an editable template.
pub fn write_default_config_file(config: &Config) {
    let path = cursor_brain_home_dir()
        .join(".cursor-brain")
        .join("config.json");
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let export = ConfigFileExport::from(config);
    if let Ok(json) = serde_json::to_string_pretty(&export) {
        if let Err(e) = std::fs::write(&path, json) {
            tracing::warn!(
                "could not write default config to {}: {}",
                path.display(),
                e
            );
        }
    }
}

/// Load config from ~/.cursor-brain/config.json only. Missing keys use built-in defaults.
/// If the config file did not exist, it is created with the effective defaults after loading.
pub fn load_config() -> Config {
    let home = cursor_brain_home_dir();
    let path = home.join(".cursor-brain").join("config.json");
    let file_existed = path.exists();
    let load: ConfigFileLoad = if file_existed {
        std::fs::read_to_string(&path)
            .ok()
            .and_then(|data| serde_json::from_str(&data).ok())
            .unwrap_or_default()
    } else {
        ConfigFileLoad::default()
    };

    let port = load.port.unwrap_or(DEFAULT_PORT).clamp(1, 65535);
    let request_timeout_sec = load
        .request_timeout_sec
        .unwrap_or(DEFAULT_REQUEST_TIMEOUT_SEC)
        .max(1);
    let session_cache_max = load
        .session_cache_max
        .unwrap_or(DEFAULT_SESSION_CACHE_MAX)
        .clamp(1, 1_000_000);
    let bind_address = load
        .bind_address
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| "0.0.0.0".to_string());
    let session_header_name = load
        .session_header_name
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| DEFAULT_SESSION_HEADER_NAME.to_string());
    let forward_thinking = load
        .forward_thinking
        .as_deref()
        .map(|s| s.to_lowercase())
        .filter(|s| s == "off" || s == "content" || s == "reasoning_content")
        .unwrap_or_else(|| "content".to_string());
    let cursor_path = load.cursor_path.filter(|s| !s.trim().is_empty());
    let default_model = load.default_model.filter(|s| !s.trim().is_empty());
    let fallback_model = load.fallback_model.filter(|s| !s.trim().is_empty());
    let minimal_workspace_dir = load.minimal_workspace_dir.filter(|s| !s.trim().is_empty());
    let sandbox = load
        .sandbox
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| "enabled".to_string());

    let config = Config {
        cursor_path,
        port,
        bind_address,
        request_timeout_sec,
        session_cache_max,
        session_header_name,
        default_model,
        fallback_model,
        minimal_workspace_dir,
        sandbox,
        forward_thinking,
    };
    if !file_existed {
        write_default_config_file(&config);
    }
    config
}
