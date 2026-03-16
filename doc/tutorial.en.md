# Tutorial (English)

User-facing guide: quick start, configuration, API usage, deployment. See [architecture.en.md](architecture.en.md) for layers and request flow.

## Quick start

1. Install cursor-agent (see [Installing cursor-agent](#installing-cursor-agent)).
2. Build: `cargo build --release`.
3. Run: `cargo run` (or run the binary). Default port: 3001.
4. Optional: create or edit `~/.cursor-brain/config.json` (see [Configuration](#configuration)).
5. Call the API: e.g. `curl -X POST http://localhost:3001/v1/chat/completions -H "Content-Type: application/json" -d '{"model":"auto","messages":[{"role":"user","content":"Hello"}]}'`.

## Configuration

- **Source**: `~/.cursor-brain/config.json` only (no environment variables). On first run, if the file is missing, a default config file is written there.
- **Location**: Windows `%USERPROFILE%\.cursor-brain\config.json`; Linux/macOS `~/.cursor-brain/config.json`.

### Main keys

| Key                     | Description                                  | Default                     |
| ----------------------- | -------------------------------------------- | --------------------------- |
| `port`                  | HTTP server port                             | 3001                        |
| `bind_address`          | Listen address (e.g. `0.0.0.0`, `127.0.0.1`) | `0.0.0.0`                   |
| `cursor_path`           | Path to cursor-agent executable              | auto-detect                 |
| `request_timeout_sec`   | Per-request timeout (seconds)                | 300                         |
| `session_cache_max`     | Session cache capacity                       | 1000                        |
| `session_header_name`   | Header for external session id               | `x-session-id`              |
| `default_model`         | Default model when request omits model       | (none)                      |
| `fallback_model`        | Fallback when no content                     | (none)                      |
| `minimal_workspace_dir` | Workspace for agent (no project MCP)         | `~/.cursor-brain/workspace` |
| `agent_mode`            | `ask` or `agent`                             | `agent`                     |
| `sandbox`               | `enabled` or `disabled`                      | `enabled`                   |
| `allow_agent_write`     | If false, do not pass `--force`              | true                        |
| `forward_thinking`      | `off`, `content`, or `reasoning_content`     | `content`                   |

### Example config

```json
{
  "port": 3001,
  "bind_address": "0.0.0.0",
  "request_timeout_sec": 300
}
```

## API usage

### Endpoints

| Method | Path                                                  | Description                                                       |
| ------ | ----------------------------------------------------- | ----------------------------------------------------------------- |
| POST   | /v1/chat/completions                                  | Chat completion (streaming or non-streaming).                     |
| GET    | /v1/models                                            | List models (from cursor-agent).                                  |
| GET    | /v1/models/:id                                        | Get model by id.                                                  |
| GET    | /v1/health                                            | Health and versions (cursor_agent_version, cursor_brain_version). |
| GET    | /v1/version                                           | cursor-agent version.                                             |
| GET    | /v1/agent/about, /v1/agent/status, /v1/agent/sessions | Agent subcommands.                                                |
| POST   | /v1/agent/chats                                       | Create empty chat.                                                |
| GET    | /v1/metrics                                           | JSON metrics (requests_total, cursor_calls_ok, etc.).             |
| POST   | /v1/embeddings                                        | 501 Not Implemented.                                              |
| POST   | /v1/completions                                       | 501 Not Implemented.                                              |

### Chat completion

- **Session**: send `X-Session-Id` (or header name from config) to reuse or create a cursor session.
- **Streaming**: set `"stream": true` in the request body.
- **Thinking**: see [openai-protocol.md](openai-protocol.md) for `content` vs `reasoning_content` and `forward_thinking`.

### Error format

All 4xx/5xx responses use: `{ "error": { "message", "code", "type" } }`. Response header `X-Request-Id` (UUID) for correlation.

## Deployment

### PID file

- **Path**: `~/.cursor-brain/cursor-brain.pid`.
- **Behavior**: Written after bind (create or truncate); removed on normal exit and on panic.
- **Use**: Single-instance check, monitoring, process managers (e.g. systemd).

### Workspace directory

- **Default**: `~/.cursor-brain/workspace`. Created on startup if missing.
- **Purpose**: Used as `--workspace` for cursor-agent; empty directory avoids project-level MCP loading.

### Register as Ironclaw (or Zeroclaw) provider

1. Start cursor-brain (e.g. `cargo run`).
2. Add provider to `~/.ironclaw/providers.json`: merge the object from [provider-definition.json](provider-definition.json) into the array.
3. In Ironclaw LLM setup, choose **Cursor Brain** and pick a model.

See [provider-compat.md](provider-compat.md) for ironclaw, zeroclaw, openclaw.

## Installing cursor-agent

cursor-brain does **not** install or upgrade cursor-agent. Install it yourself:

- **Linux / macOS**: `curl https://cursor.com/install -fsSL | bash`
- **Windows**: Follow [Cursor documentation](https://cursor.com). Ensure `agent` is on PATH or set `cursor_path` in config.

## See also

- [DESIGN.md](DESIGN.md) — design decisions, defaults, platform support.
- [openai-protocol.md](openai-protocol.md) — OpenAI alignment, `content` vs `reasoning_content`.
- [architecture.en.md](architecture.en.md) — component layers and request flow.
