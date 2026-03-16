//! Configuration: single source of defaults; load from ~/.cursor-brain/config.json only (no env vars).
//! Keys: port, bind_address, request_timeout_sec, session, default_model, fallback_model,
//! cursor_path, minimal_workspace_dir, agent_mode, sandbox, allow_agent_write, forward_thinking.
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
    agent_mode: String,
    sandbox: String,
    allow_agent_write: bool,
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
            agent_mode: c.agent_mode.clone(),
            sandbox: c.sandbox.clone(),
            allow_agent_write: c.allow_agent_write,
            forward_thinking: c.forward_thinking.clone(),
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
    /// "ask" | "agent"
    pub agent_mode: String,
    /// "enabled" | "disabled"
    pub sandbox: String,
    /// If false, do not pass --force to cursor-agent.
    pub allow_agent_write: bool,
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
        let local = std::env::var("LOCALAPPDATA").unwrap_or_else(|_| {
            home.join("AppData")
                .join("Local")
                .to_string_lossy()
                .into_owned()
        });
        let local = PathBuf::from(local);
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
    let mut cursor_path: Option<String> = None;
    let mut port = DEFAULT_PORT;
    let mut bind_address = "0.0.0.0".to_string();
    let mut request_timeout_sec = DEFAULT_REQUEST_TIMEOUT_SEC;
    let mut session_cache_max = DEFAULT_SESSION_CACHE_MAX;
    let mut session_header_name = DEFAULT_SESSION_HEADER_NAME.to_string();
    let mut default_model: Option<String> = None;
    let mut fallback_model: Option<String> = None;
    let mut minimal_workspace_dir: Option<String> = None;
    let mut agent_mode = "agent".to_string();
    let mut sandbox = "enabled".to_string();
    let mut allow_agent_write = true;
    let mut forward_thinking = "content".to_string();

    let home = cursor_brain_home_dir();
    let path = home.join(".cursor-brain").join("config.json");
    let file_existed = path.exists();
    if file_existed {
        if let Ok(data) = std::fs::read_to_string(&path) {
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(&data) {
                cursor_path = v
                    .get("cursor_path")
                    .and_then(|c| c.as_str())
                    .map(String::from);
                if let Some(p) = v.get("port").and_then(|p| p.as_u64()) {
                    port = p.clamp(1, 65535) as u16;
                }
                if let Some(s) = v.get("bind_address").and_then(|s| s.as_str()) {
                    if !s.is_empty() {
                        bind_address = s.to_string();
                    }
                }
                if let Some(t) = v.get("request_timeout_sec").and_then(|t| t.as_u64()) {
                    request_timeout_sec = t.max(1);
                }
                if let Some(n) = v.get("session_cache_max").and_then(|n| n.as_u64()) {
                    session_cache_max = n.clamp(1, 1_000_000) as u32;
                }
                if let Some(s) = v.get("session_header_name").and_then(|s| s.as_str()) {
                    if !s.is_empty() {
                        session_header_name = s.to_string();
                    }
                }
                if let Some(m) = v.get("default_model").and_then(|m| m.as_str()) {
                    if !m.is_empty() {
                        default_model = Some(m.to_string());
                    }
                }
                if let Some(m) = v.get("fallback_model").and_then(|m| m.as_str()) {
                    if !m.is_empty() {
                        fallback_model = Some(m.to_string());
                    }
                }
                if let Some(d) = v.get("minimal_workspace_dir").and_then(|d| d.as_str()) {
                    if !d.is_empty() {
                        minimal_workspace_dir = Some(d.to_string());
                    }
                }
                if let Some(m) = v.get("agent_mode").and_then(|m| m.as_str()) {
                    if !m.is_empty() {
                        agent_mode = m.to_string();
                    }
                }
                if let Some(s) = v.get("sandbox").and_then(|s| s.as_str()) {
                    if !s.is_empty() {
                        sandbox = s.to_string();
                    }
                }
                if let Some(b) = v.get("allow_agent_write").and_then(|b| b.as_bool()) {
                    allow_agent_write = b;
                }
                if let Some(s) = v.get("forward_thinking").and_then(|s| s.as_str()) {
                    let s = s.to_lowercase();
                    if s == "off" || s == "content" || s == "reasoning_content" {
                        forward_thinking = s;
                    }
                }
            }
        }
    }

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
        agent_mode,
        sandbox,
        allow_agent_write,
        forward_thinking,
    };
    if !file_existed {
        write_default_config_file(&config);
    }
    config
}
