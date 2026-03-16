# cursor-brain 设计文档

## 文档索引

- **架构与请求流**：[architecture.en.md](architecture.en.md) / [architecture.zh.md](architecture.zh.md)（含 Mermaid 图）
- **使用教程**：[tutorial.en.md](tutorial.en.md) / [tutorial.zh.md](tutorial.zh.md)（快速开始、配置、API、部署）
- **OpenAI 协议与字段**：[openai-protocol.md](openai-protocol.md)（`content` vs `reasoning_content`）

---

## 目标与范围

cursor-brain 是**通用 OpenAI 兼容 HTTP 服务**，将 Cursor Agent 作为推理后端，对外提供稳定、可观测的 API。目标为「首版即终版」：API 契约稳定、错误格式统一、可观测与文档齐全。

- **OpenAI 兼容**：POST /v1/chat/completions（流式/非流式）、GET /v1/models、GET /v1/models/:id；POST /v1/embeddings 与 POST /v1/completions 以 501 明确返回并说明原因。
- **运维与可观测**：GET /v1/health（含 cursor_agent_version、cursor_brain_version）、GET /v1/metrics（JSON 指标）。
- **cursor-agent 衍生**：GET /v1/version；GET /v1/agent/about、/v1/agent/status、/v1/agent/sessions、POST /v1/agent/chats。
- **配置与数据**：`~/.cursor-brain/` 为配置与数据根目录；支持 bind_address、minimal_workspace_dir、sandbox。

## 架构概览

- **技术栈**：Rust、Axum、tokio；与 ironclaw-cursor-brain 对齐。
- **分层**：HTTP 路由 (server) → 业务 (service) → cursor 子进程 (cursor) + session 持久化 (session) + OpenAI 请求/响应 (openai)；配置 (config) 与指标 (metrics) 贯穿。

数据流：请求 → 统一错误体与 X-Request-Id 中间件 → 路由 → service 解析 session、spawn cursor-agent、流式/非流式 → 响应。

## 配置与部署

- **配置文件**：配置**仅**来自 `~/.cursor-brain/config.json`；不读取环境变量。若文件不存在或字段缺失，使用代码内默认值。请编辑 `~/.cursor-brain/config.json` 修改配置。
- **配置项**：port、bind_address（默认 0.0.0.0）、request_timeout_sec、session 相关、default_model、fallback_model、cursor_path、minimal_workspace_dir（默认 ~/.cursor-brain/workspace）、sandbox（enabled|disabled）、forward_thinking（off|content|reasoning_content，默认 content）。--mode 写死为 agent；写操作已写死禁止（不传 --force）。
- **Session 存储**：默认 `~/.cursor-brain/sessions.json`；外部 session id 通过请求头（默认 X-Session-Id）与 cursor session_id 映射。

## 错误与超时

- **统一错误体**：所有 4xx/5xx 使用 `{ "error": { "message", "code", "type" } }`；501 使用 `code: "not_implemented"`。不在响应中泄露内部路径或堆栈。
- **超时**：chat 使用 config.request_timeout_sec；models 列表 15s；version/agent 子命令 15s。超时返回 504，cursor 不可用返回 503。
- **状态码**：400 invalid_request、404 not_found、501 not_implemented、503 service_unavailable、504 gateway_timeout。

## 可观测与安全

- **请求 ID**：响应头 `X-Request-Id`（UUID）；日志记录 request_id、method、path、status、elapsed_ms；**不记录** API key、session id 或用户内容。
- **GET /v1/metrics**：JSON 格式，含 requests_total、cursor_calls_ok、cursor_calls_fail、cursor_calls_timeout。
- **bind_address**：可配置为 127.0.0.1 仅本机访问；默认 0.0.0.0 与 ironclaw 兼容。

## PID 文件

- **路径**：`~/.cursor-brain/cursor-brain.pid`（与 config 同基目录，按平台 home 解析）。
- **写入**：成功 bind 后写入当前进程 PID（单行十进制）。若文件不存在则创建，若已存在则先截断再写入（不先删除再创建）。
- **删除**：正常退出（SIGTERM/SIGINT）与 panic 时删除。
- **用途**：单实例检测、监控与进程管理（如 systemd/supervisor）。

## 版本与兼容性承诺

- **GET /v1/health** 必含 `cursor_agent_version`（来自 cursor-agent --version）与 `cursor_brain_version`（服务版本）；cursor 不可用时 version 可为 "unknown"。
- **/v1/** 路径与现有请求/响应字段保持稳定；若将来必须破坏性变更，通过新版本路径（如 /v2/）而非修改 /v1/。

## output-format 选型（stream-json）

- **结论**：仅使用 `--output-format stream-json`，不提供 json 与 stream-json 的配置切换。
- **理由**：stream-json 支持逐行事件（thinking/text/result/session_id），便于流式与 thinking 策略；json 多为一次性输出，对当前流式优先架构价值有限。openclaw-cursor-brain 的 proxy 可通过 `outputFormat` 配置；本项目固定 stream-json 以简化实现并与流式/thinking 行为一致。
- **实现**：spawn cursor-agent 时命令行固定包含 `--output-format stream-json`。

## MCP 与效率控制

- **无官方「关闭 MCP」**：cursor-agent CLI 无 --no-mcp；通过 **minimal_workspace_dir**（空目录、无 .cursor/mcp.json）作为 spawn 的 --workspace，从源头避免项目级 MCP 加载。
- **--mode**：写死为 agent。
- **sandbox**：enabled（默认）/ disabled。
- **写操作**：写死禁止，不传 --force，cursor-agent 不执行写/命令。

## Default values (OS-aware)

All config keys have built-in defaults; the same values are used at runtime and when writing the default config file on first run.

| Key                                | Default                   | Notes                                                                    |
| ---------------------------------- | ------------------------- | ------------------------------------------------------------------------ |
| `port`                             | 3001                      | Same on all platforms.                                                   |
| `bind_address`                     | "0.0.0.0"                 | Same on all platforms.                                                   |
| `cursor_path`                      | (auto-detect)             | Omitted in written config (null); see "Cursor-agent search paths" below. |
| `request_timeout_sec`              | 300                       | Same on all platforms.                                                   |
| `session_cache_max`                | 1000                      | Same on all platforms.                                                   |
| `session_header_name`              | "x-session-id"            | Same on all platforms.                                                   |
| `default_model` / `fallback_model` | null                      | Optional.                                                                |
| `minimal_workspace_dir`            | ~/.cursor-brain/workspace | Resolved via home dir per platform.                                      |
| `sandbox`                          | "enabled"                 | "enabled" \| "disabled".                                                 |
| `forward_thinking`                 | "content"                 | "off" \| "content" \| "reasoning_content".                               |

## Cursor-agent search paths and detection

- **Windows**: Resolve via `where agent`; if not in PATH, search: `%LOCALAPPDATA%\Programs\cursor\resources\app\bin\agent.exe`, `%LOCALAPPDATA%\cursor-agent\agent.cmd`, `%USERPROFILE%\.cursor\bin\agent.exe`, `%USERPROFILE%\.local\bin\agent.exe`.
- **Linux / macOS**: Resolve via `which agent`; if not in PATH, search: `~/.local/bin/agent`, `/usr/local/bin/agent`, `~/.cursor/bin/agent`.

Config key `cursor_path` overrides auto-detection when set and the path exists.

## Cursor-agent install / upgrade / login (out of scope)

cursor-brain **does not** provide install, upgrade, or login for cursor-agent. Users must install and authenticate cursor-agent using official methods:

- **Linux / macOS**: `curl https://cursor.com/install -fsSL | bash` (or follow [Cursor install docs](https://cursor.com)).
- **Windows**: Follow the official Cursor / cursor-agent installation instructions for Windows.

After installation, ensure `agent` is on PATH or set `cursor_path` in `~/.cursor-brain/config.json`.

## Platform support

cursor-brain supports **Windows**, **Linux**, and **macOS**:

- Config path: `%USERPROFILE%\.cursor-brain\config.json` (Windows), `~/.cursor-brain/config.json` (Linux/macOS). Same for PID file and workspace dir under the same base.
- PID file: `~/.cursor-brain/cursor-brain.pid` (home-dir based on each platform).
- Cursor-agent detection: `where` on Windows, `which` on Unix; search paths differ as above. Use `PathBuf`/`join()` for paths; no hardcoded path separators.

## OpenAI protocol and response fields

See [doc/openai-protocol.md](openai-protocol.md) for:

- Extent of alignment with the official OpenAI Chat Completions API and known deviations (e.g. 501 for embeddings/completions).
- Meaning of `content` (standard assistant message body) and `reasoning_content` (extension for reasoning/thinking), and how `forward_thinking` maps cursor-agent thinking to these fields.

## 与 ironclaw / openclaw 迁移

- 从 ironclaw-cursor-brain 迁移：配置从 `~/.ironclaw/cursor-brain.json` 改为 `~/.cursor-brain/config.json`；session 存储路径改为 `~/.cursor-brain/sessions.json`；API 行为（chat、models、health）保持兼容。
- openclaw 的 streaming-proxy 仅暴露 chat/completions、models、health；本服务扩展了 models/:id、version、agent/\*、embeddings 501、completions 501、metrics，与现有端点兼容。
