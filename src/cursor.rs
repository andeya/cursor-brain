//! Cursor-agent subprocess layer: spawn, stdin write, stdout stream-json parsing.
//! Options: workspace_dir, agent_mode (--mode), sandbox, allow_agent_write (--force).
//! Used by service for chat completion and by server for list-models, version, agent subcommands.

use std::io::{BufRead, BufReader, Write};
use std::process::{Child, Command, Stdio};
use std::time::Duration;

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub enum CursorEvent {
    SessionId(String),
    Thinking { text: String },
    Text(String),
    Result(String),
    ToolCall { subtype: String, tool: String },
}

#[derive(Debug, Default)]
pub struct CompletionOutput {
    pub content: String,
    pub thinking_text: String,
    /// When forward_thinking is "reasoning_content", thinking is stored here.
    pub reasoning_content: Option<String>,
    pub finish_reason: String,
}

/// Options for spawning cursor-agent (from config).
#[derive(Clone, Debug)]
pub struct SpawnOptions {
    pub workspace_dir: Option<String>,
    pub agent_mode: String, // "ask" | "agent"
    pub sandbox: String,    // "enabled" | "disabled"
    pub allow_agent_write: bool,
}

impl Default for SpawnOptions {
    fn default() -> Self {
        Self {
            workspace_dir: None,
            agent_mode: "agent".to_string(),
            sandbox: "enabled".to_string(),
            allow_agent_write: true,
        }
    }
}

pub fn spawn_cursor_agent(
    cursor_path: &str,
    user_msg: &str,
    model: Option<&str>,
    resume_session_id: Option<&str>,
    options: &SpawnOptions,
) -> std::io::Result<Child> {
    let mut args = vec![
        "-p".into(),
        "--output-format".into(),
        "stream-json".into(),
        "--stream-partial-output".into(),
        "--trust".into(),
        "--approve-mcps".into(),
    ];
    if options.allow_agent_write {
        args.push("--force".into());
    }
    args.push("--mode".into());
    args.push(options.agent_mode.clone());
    args.push("--sandbox".into());
    args.push(options.sandbox.clone());

    let model_for_agent = model.map(|m| {
        let m = m.trim();
        if m.is_empty()
            || m.eq_ignore_ascii_case("cursor")
            || m.eq_ignore_ascii_case("cursor-default")
            || m.eq_ignore_ascii_case("default")
        {
            "auto"
        } else {
            m
        }
    });
    if let Some(m) = model_for_agent {
        if !m.is_empty() && m != "auto" {
            args.push("--model".into());
            args.push(m.to_string());
        }
    }
    if let Some(r) = resume_session_id {
        if !r.is_empty() {
            args.push("--resume".into());
            args.push(r.to_string());
        }
    }

    #[cfg(windows)]
    let needs_shell = cursor_path.to_lowercase().ends_with(".cmd")
        || cursor_path.to_lowercase().ends_with(".bat");
    #[cfg(not(windows))]
    let _needs_shell = false;

    let mut cmd = Command::new(cursor_path);
    cmd.args(&args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(ref dir) = options.workspace_dir {
        if !dir.is_empty() {
            cmd.current_dir(dir);
        }
    }
    #[cfg(windows)]
    if needs_shell {
        cmd.creation_flags(0x08000000);
        let cmd_str = format!("\"{}\" {}", cursor_path, args.join(" "));
        cmd = Command::new("cmd");
        cmd.args(["/C", &cmd_str])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
    }

    let mut child = cmd.spawn()?;
    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(user_msg.as_bytes());
        let _ = stdin.write_all(b"\n");
        let _ = stdin.flush();
    }
    Ok(child)
}

pub fn list_models_via_agent(cursor_path: &str) -> Vec<String> {
    #[cfg(windows)]
    let needs_shell = cursor_path.to_lowercase().ends_with(".cmd")
        || cursor_path.to_lowercase().ends_with(".bat");
    #[cfg(not(windows))]
    let needs_shell = false;

    let output = if needs_shell {
        #[cfg(windows)]
        {
            let cmd_str = format!("\"{}\" --list-models", cursor_path);
            std::process::Command::new("cmd")
                .args(["/C", &cmd_str])
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::null())
                .output()
        }
        #[cfg(not(windows))]
        {
            unreachable!()
        }
    } else {
        std::process::Command::new(cursor_path)
            .arg("--list-models")
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .output()
    };

    let out = match output {
        Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout).into_owned(),
        _ => return Vec::new(),
    };
    parse_list_models_output(&out)
}

pub fn parse_list_models_output(out: &str) -> Vec<String> {
    let trimmed = out.trim();
    if trimmed.is_empty() {
        return Vec::new();
    }
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(trimmed) {
        if let Some(arr) = v.as_array() {
            let ids: Vec<String> = arr
                .iter()
                .filter_map(|x| x.as_str().map(String::from))
                .filter(|s| !s.is_empty())
                .collect();
            if !ids.is_empty() {
                return ids;
            }
        }
    }
    let ids: Vec<String> = trimmed
        .lines()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty() && !l.starts_with('|') && !l.starts_with('-'))
        .filter_map(|l| {
            let s = l.split_whitespace().next().unwrap_or(l).to_string();
            if s.chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '.' || c == '_')
            {
                Some(s)
            } else {
                None
            }
        })
        .collect();
    if ids.is_empty() {
        Vec::new()
    } else {
        ids
    }
}

pub fn parse_stream_json_line(line: &str) -> Option<CursorEvent> {
    let line = line.trim();
    if line.is_empty() {
        return None;
    }
    let v: serde_json::Value = serde_json::from_str(line).ok()?;
    let ty = v.get("type")?.as_str()?;
    match ty {
        "session_id" => Some(CursorEvent::SessionId(
            v.get("session_id")?.as_str()?.to_string(),
        )),
        "thinking" => {
            let text = v
                .get("text")
                .and_then(|t| t.as_str())
                .unwrap_or("")
                .to_string();
            if text.is_empty() && v.get("subtype").and_then(|s| s.as_str()) != Some("completed") {
                None
            } else {
                Some(CursorEvent::Thinking { text })
            }
        }
        "text" => v
            .get("text")
            .and_then(|t| t.as_str())
            .map(|s| CursorEvent::Text(s.to_string())),
        "result" => v
            .get("result")
            .and_then(|r| r.as_str())
            .map(|s| CursorEvent::Result(s.to_string())),
        "tool_call" => {
            let subtype = v
                .get("subtype")
                .and_then(|s| s.as_str())
                .unwrap_or("")
                .to_string();
            let tool_call = v.get("tool_call").and_then(|t| t.as_object())?;
            let tool = tool_call.keys().next().cloned().unwrap_or_default();
            Some(CursorEvent::ToolCall { subtype, tool })
        }
        _ => None,
    }
}

/// forward_thinking: "off" | "content" | "reasoning_content"
pub fn run_to_completion(
    child: &mut Child,
    timeout: Duration,
    forward_thinking: &str,
    mut on_session_id: Option<&mut dyn FnMut(&str)>,
) -> std::io::Result<CompletionOutput> {
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| std::io::Error::other("no stdout"))?;
    let stderr_handle = child.stderr.take().map(|mut stderr| {
        std::thread::spawn(move || {
            let mut s = String::new();
            let _ = std::io::Read::read_to_string(&mut stderr, &mut s);
            s
        })
    });
    let reader = BufReader::new(stdout);
    let mut out = CompletionOutput {
        finish_reason: "stop".into(),
        ..Default::default()
    };
    let start = std::time::Instant::now();

    for line in reader.lines() {
        if start.elapsed() > timeout {
            let _ = child.kill();
            break;
        }
        let line = match line {
            Ok(l) => l,
            Err(_) => break,
        };
        if let Some(ev) = parse_stream_json_line(&line) {
            match ev {
                CursorEvent::Text(s) => out.content.push_str(&s),
                CursorEvent::Result(s) => out.content = s,
                CursorEvent::Thinking { text } => match forward_thinking {
                    "off" => {}
                    "reasoning_content" => {
                        out.reasoning_content
                            .get_or_insert_with(String::new)
                            .push_str(&text);
                    }
                    _ => out.thinking_text.push_str(&text),
                },
                CursorEvent::SessionId(s) => {
                    if let Some(f) = &mut on_session_id {
                        f(&s);
                    }
                }
                CursorEvent::ToolCall { subtype, tool } => {
                    tracing::debug!(subtype = %subtype, tool = %tool, "cursor tool_call");
                }
            }
        }
    }

    if out.content.is_empty()
        && out.thinking_text.is_empty()
        && out
            .reasoning_content
            .as_deref()
            .is_none_or(|s| s.is_empty())
    {
        let stderr = stderr_handle
            .and_then(|h| h.join().ok())
            .unwrap_or_default();
        let stderr = stderr.trim();
        if !stderr.is_empty() {
            tracing::warn!(
                cursor_agent_stderr = %stderr,
                "cursor-agent returned no content"
            );
        }
    }
    Ok(out)
}

#[derive(Debug, Clone)]
pub enum StreamDelta {
    Content(String),
    /// Thinking in separate field for reasoning_content mode.
    ReasoningContent(String),
    Done {
        finish_reason: String,
    },
}

/// forward_thinking: "off" | "content" | "reasoning_content"
pub fn run_to_completion_stream<F>(
    child: &mut Child,
    timeout: Duration,
    forward_thinking: &str,
    mut on_event: F,
    mut on_session_id: Option<&mut dyn FnMut(&str)>,
) -> std::io::Result<()>
where
    F: FnMut(StreamDelta),
{
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| std::io::Error::other("no stdout"))?;
    let reader = BufReader::new(stdout);
    let start = std::time::Instant::now();

    for line in reader.lines() {
        if start.elapsed() > timeout {
            let _ = child.kill();
            on_event(StreamDelta::Done {
                finish_reason: "timeout".to_string(),
            });
            return Ok(());
        }
        let line = match line {
            Ok(l) => l,
            Err(_) => break,
        };
        if let Some(ev) = parse_stream_json_line(&line) {
            match ev {
                CursorEvent::Text(s) => on_event(StreamDelta::Content(s)),
                CursorEvent::Result(s) => on_event(StreamDelta::Content(s)),
                CursorEvent::Thinking { text } => {
                    if !text.is_empty() {
                        match forward_thinking {
                            "off" => {}
                            "reasoning_content" => on_event(StreamDelta::ReasoningContent(text)),
                            _ => on_event(StreamDelta::Content(format!(
                                "\n\n> 💭 {}\n\n",
                                text.trim()
                            ))),
                        }
                    }
                }
                CursorEvent::SessionId(s) => {
                    if let Some(f) = &mut on_session_id {
                        f(&s);
                    }
                }
                CursorEvent::ToolCall { subtype, tool } => {
                    tracing::debug!(subtype = %subtype, tool = %tool, "cursor tool_call");
                }
            }
        }
    }
    on_event(StreamDelta::Done {
        finish_reason: "stop".to_string(),
    });
    Ok(())
}

/// Run cursor-agent with subcommand (e.g. "about", "status") and args (e.g. [] or ["ls"]).
/// Returns (stdout, stderr) as strings; empty on failure.
pub fn run_agent_subcommand(
    cursor_path: &str,
    subcommand: &str,
    args: &[&str],
) -> (String, String) {
    #[cfg(windows)]
    let needs_shell = cursor_path.to_lowercase().ends_with(".cmd")
        || cursor_path.to_lowercase().ends_with(".bat");
    #[cfg(not(windows))]
    let needs_shell = false;

    let output = if needs_shell {
        #[cfg(windows)]
        {
            let all_args = std::iter::once(subcommand).chain(args.iter().copied());
            let cmd_str = format!(
                "\"{}\" agent {}",
                cursor_path,
                all_args.collect::<Vec<_>>().join(" ")
            );
            std::process::Command::new("cmd")
                .args(["/C", &cmd_str])
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                .output()
        }
        #[cfg(not(windows))]
        {
            unreachable!()
        }
    } else {
        let mut cmd = std::process::Command::new(cursor_path);
        cmd.arg("agent")
            .arg(subcommand)
            .args(args)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .output()
    };
    match output {
        Ok(o) => (
            String::from_utf8_lossy(&o.stdout).trim().to_string(),
            String::from_utf8_lossy(&o.stderr).trim().to_string(),
        ),
        _ => (String::new(), String::new()),
    }
}

/// Run cursor-agent --version, return trimmed stdout or None.
pub fn cursor_agent_version(cursor_path: &str) -> Option<String> {
    #[cfg(windows)]
    let needs_shell = cursor_path.to_lowercase().ends_with(".cmd")
        || cursor_path.to_lowercase().ends_with(".bat");
    #[cfg(not(windows))]
    let needs_shell = false;

    let output = if needs_shell {
        #[cfg(windows)]
        {
            let cmd_str = format!("\"{}\" --version", cursor_path);
            std::process::Command::new("cmd")
                .args(["/C", &cmd_str])
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::null())
                .output()
        }
        #[cfg(not(windows))]
        {
            unreachable!()
        }
    } else {
        std::process::Command::new(cursor_path)
            .arg("--version")
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .output()
    };
    output
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
}
