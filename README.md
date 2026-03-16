# cursor-brain

[![Crates.io](https://img.shields.io/crates/v/cursor-brain)](https://crates.io/crates/cursor-brain)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-1.70%2B-orange)](https://www.rust-lang.org/)

**English** | [中文](README.zh-CN.md)

**OpenAI-compatible HTTP service** powered by **Cursor Agent**. Use it as a drop-in chat/completions endpoint for Openclaw, Ironclaw, Zeroclaw, or any OpenAI API client. Runs on **Windows**, **Linux**, and **macOS**.

## Features

- **OpenAI-compatible**: `POST /v1/chat/completions` (streaming + non-streaming), `GET /v1/models`, `GET /v1/health`, and related endpoints.
- **Cursor Agent backend**: Spawns the Cursor CLI as a subprocess; supports session resume and thinking output (`content` or `reasoning_content`).
- **Config file only**: All settings from `~/.cursor-brain/config.json`; default file is written on first run.
- **Observability**: Request ID header, JSON metrics (`/v1/metrics`), health with versions.
- **Provider-ready**: Register as an Openclaw/Ironclaw/Zeroclaw provider (see README examples below).

## Quick start

Recommended: install globally, then run:

```bash
cargo install cursor-brain
cursor-brain
```

Or from the repo: install [cursor-agent](#installing-cursor-agent) first, then `cargo run`.

Default: `http://localhost:3001`. Example request:

```bash
curl -X POST http://localhost:3001/v1/chat/completions \
  -H "Content-Type: application/json" \
  -d '{"model":"auto","messages":[{"role":"user","content":"Hello"}]}'
```

## Configuration

Configuration is read **only** from `~/.cursor-brain/config.json` (no environment variables). If the file is missing on first run, a default config is written there.

| Key                     | Description                              | Default                     |
| ----------------------- | ---------------------------------------- | --------------------------- |
| `port`                  | HTTP port                                | 3001                        |
| `bind_address`          | Listen address                           | `0.0.0.0`                   |
| `cursor_path`           | Path to cursor-agent                     | auto-detect                 |
| `request_timeout_sec`   | Per-request timeout (sec)                | 300                         |
| `session_cache_max`     | Session cache capacity                   | 1000                        |
| `session_header_name`   | Header for session id                    | `x-session-id`              |
| `default_model`         | Default model when request omits model   | (none)                      |
| `fallback_model`        | Fallback when no content                 | (none)                      |
| `minimal_workspace_dir` | Agent workspace (no MCP)                 | `~/.cursor-brain/workspace` |
| `sandbox`               | `enabled` or `disabled`                  | `enabled`                   |
| `forward_thinking`      | `off`, `content`, or `reasoning_content` | `content`                   |

Example `~/.cursor-brain/config.json`:

```json
{
  "port": 3001,
  "bind_address": "0.0.0.0",
  "request_timeout_sec": 300
}
```

Full key list and defaults: [Design & defaults](doc/DESIGN.md) and [Tutorial (EN)](doc/tutorial.en.md).

## Installing cursor-agent

cursor-brain does **not** install or upgrade cursor-agent. Install it yourself:

- **Linux / macOS**: `curl https://cursor.com/install -fsSL | bash`
- **Windows**: See [Cursor documentation](https://cursor.com). Ensure `agent` is on PATH or set `cursor_path` in config.

## Platform support

**Windows**, **Linux**, **macOS**. Config and PID use the user home directory (`%USERPROFILE%\.cursor-brain` on Windows, `~/.cursor-brain` on Unix). Cursor-agent detection and paths are OS-aware; see [Design & defaults](doc/DESIGN.md).

## PID file

On startup, the process writes its PID to `~/.cursor-brain/cursor-brain.pid` (create or truncate) and removes it on exit. Use for single-instance checks or monitoring.

## Documentation

| Doc                                                   | Description                                          |
| ----------------------------------------------------- | ---------------------------------------------------- |
| [Architecture (EN)](doc/architecture.en.md)           | Component layers and request flow (with diagrams)    |
| [Tutorial (EN)](doc/tutorial.en.md)                   | Quick start, config, API usage, deployment           |
| [Design & defaults](doc/DESIGN.md)                    | Design decisions, default values, platform notes     |
| [OpenAI alignment & thinking](doc/openai-protocol.md) | `content` vs `reasoning_content`, `forward_thinking` |
| [API spec](doc/openapi.yaml)                          | OpenAPI 3.0 definition                               |

**中文文档**：[README 中文](README.zh-CN.md) · [架构](doc/architecture.zh.md) · [教程](doc/tutorial.zh.md)

## Register as Openclaw / Ironclaw / Zeroclaw provider

1. Start cursor-brain (e.g. `cargo run` or `cursor-brain`).
2. Add the provider to your client's providers config (see examples below).
3. In your client’s LLM setup, choose **Cursor Brain** and pick a model.

### Example provider configuration

**Openclaw** — edit `~/.openclaw/openclaw.json` (JSON5: comments and trailing commas allowed). Add a provider under `models.providers` and set it as primary if desired:

```json5
{
  models: {
    mode: "merge",
    providers: {
      cursor_brain: {
        baseUrl: "http://127.0.0.1:3001/v1",
        api: "openai-completions",
        models: [
          { id: "auto", name: "Cursor (auto)" },
          { id: "cursor-default", name: "Cursor default" },
        ],
      },
    },
  },
  agents: {
    defaults: {
      model: { primary: "cursor_brain/auto" },
    },
  },
}
```

Use `cursor_brain/auto` or `cursor_brain/cursor-default` in the UI or CLI. Omit `agents.defaults.model` if you only want the provider available without changing the default. If cursor-brain is on another host, set `baseUrl` accordingly (e.g. `http://192.168.1.10:3001/v1`).

**Ironclaw** — add to `~/.ironclaw/providers.json` (merge into the `providers` array):

```json
[
  {
    "id": "cursor",
    "aliases": ["cursor_brain", "cursor-brain"],
    "protocol": "open_ai_completions",
    "default_base_url": "http://127.0.0.1:3001/v1",
    "base_url_env": "CURSOR_BRAIN_BASE_URL",
    "base_url_required": false,
    "api_key_required": false,
    "model_env": "CURSOR_BRAIN_MODEL",
    "default_model": "auto",
    "description": "Cursor Agent via cursor-brain (local OpenAI-compatible proxy)",
    "setup": {
      "kind": "open_ai_compatible",
      "secret_name": "llm_cursor_brain_api_key",
      "display_name": "Cursor Brain",
      "can_list_models": true
    }
  }
]
```

**Zeroclaw** — edit `~/.zeroclaw/config.toml` (create the file if missing). Use the `custom:` provider with cursor-brain’s base URL; no API key required:

```toml
default_provider = "custom:http://127.0.0.1:3001/v1"
default_model = "auto"
```

If cursor-brain runs on another host or port, change the URL (e.g. `custom:http://192.168.1.10:3001/v1`). Zeroclaw appends `/chat/completions` to this base URL automatically.

Full reference: [provider-definition.json](doc/provider-definition.json).

## License

[MIT](LICENSE). Code comments in this project are in English only.
