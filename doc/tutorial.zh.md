# 使用教程（中文）

面向用户的说明：快速开始、配置、API 用法、部署。架构与请求流见 [architecture.zh.md](architecture.zh.md)。

## 快速开始

1. 安装 cursor-agent（见 [安装 cursor-agent](#安装-cursor-agent)）。
2. 构建：`cargo build --release`。
3. 运行：`cargo run`（或直接运行二进制）。默认端口：3001。
4. 可选：创建或编辑 `~/.cursor-brain/config.json`（见 [配置](#配置)）。
5. 调用 API：例如 `curl -X POST http://localhost:3001/v1/chat/completions -H "Content-Type: application/json" -d '{"model":"auto","messages":[{"role":"user","content":"你好"}]}'`。

## 配置

- **来源**：仅从 `~/.cursor-brain/config.json` 读取（不使用环境变量）。首次运行时若文件不存在，会在此处写入默认配置。
- **路径**：Windows `%USERPROFILE%\.cursor-brain\config.json`；Linux/macOS `~/.cursor-brain/config.json`。

### 主要配置项

| 键                      | 说明                                    | 默认值                      |
| ----------------------- | --------------------------------------- | --------------------------- |
| `port`                  | HTTP 服务端口                           | 3001                        |
| `bind_address`          | 监听地址（如 `0.0.0.0`、`127.0.0.1`）   | `0.0.0.0`                   |
| `cursor_path`           | cursor-agent 可执行文件路径             | 自动检测                    |
| `request_timeout_sec`   | 单次请求超时（秒）                      | 300                         |
| `session_cache_max`     | 会话缓存容量                            | 1000                        |
| `session_header_name`   | 外部会话 id 的请求头名                  | `x-session-id`              |
| `default_model`         | 请求未指定 model 时的默认模型           | （无）                      |
| `fallback_model`        | 无内容时的回退模型                      | （无）                      |
| `minimal_workspace_dir` | agent 工作目录（无项目 MCP）            | `~/.cursor-brain/workspace` |
| `sandbox`               | `enabled` 或 `disabled`                 | `enabled`                   |
| `forward_thinking`      | `off`、`content` 或 `reasoning_content` | `content`                   |

### 配置示例

```json
{
  "port": 3001,
  "bind_address": "0.0.0.0",
  "request_timeout_sec": 300
}
```

## API 用法

### 端点

| 方法 | 路径                                                  | 说明                                                       |
| ---- | ----------------------------------------------------- | ---------------------------------------------------------- |
| POST | /v1/chat/completions                                  | 聊天补全（流式或非流式）。                                 |
| GET  | /v1/models                                            | 模型列表（来自 cursor-agent）。                            |
| GET  | /v1/models/:id                                        | 按 id 查询模型。                                           |
| GET  | /v1/health                                            | 健康与版本（cursor_agent_version、cursor_brain_version）。 |
| GET  | /v1/version                                           | cursor-agent 版本。                                        |
| GET  | /v1/agent/about、/v1/agent/status、/v1/agent/sessions | agent 子命令。                                             |
| POST | /v1/agent/chats                                       | 创建空会话。                                               |
| GET  | /v1/metrics                                           | JSON 指标（requests_total、cursor_calls_ok 等）。          |
| POST | /v1/embeddings                                        | 501 Not Implemented。                                      |
| POST | /v1/completions                                       | 501 Not Implemented。                                      |

### 聊天补全

- **会话**：通过请求头 `X-Session-Id`（或配置中的 header 名）复用或创建 cursor 会话。
- **流式**：请求体中设置 `"stream": true`。
- **思考过程**：见 [openai-protocol.md](openai-protocol.md) 中 `content` 与 `reasoning_content` 及 `forward_thinking`。

### 错误格式

所有 4xx/5xx 响应格式：`{ "error": { "message", "code", "type" } }`。响应头 `X-Request-Id`（UUID）用于关联。

## 部署

### PID 文件

- **路径**：`~/.cursor-brain/cursor-brain.pid`。
- **行为**：在 bind 成功后写入（创建或截断）；正常退出或 panic 时删除。
- **用途**：单实例检测、监控、进程管理（如 systemd）。

### 工作目录

- **默认**：`~/.cursor-brain/workspace`。启动时若不存在会创建。
- **用途**：作为 cursor-agent 的 `--workspace`；空目录可避免项目级 MCP 加载。

### 注册为 Ironclaw（或 Zeroclaw）提供方

1. 启动 cursor-brain（如 `cargo run`）。
2. 在 `~/.ironclaw/providers.json` 中添加提供方：将 [provider-definition.json](provider-definition.json) 中的对象合并到数组中。
3. 在 Ironclaw 的 LLM 设置中选择 **Cursor Brain** 并选择模型。

## 安装 cursor-agent

cursor-brain **不**负责安装或升级 cursor-agent。请自行安装：

- **Linux / macOS**：`curl https://cursor.com/install -fsSL | bash`
- **Windows**：按 [Cursor 文档](https://cursor.com) 操作。确保 `agent` 在 PATH 中或在配置中设置 `cursor_path`。

## 参见

- [DESIGN.md](DESIGN.md) — 设计决策、默认值、平台支持。
- [openai-protocol.md](openai-protocol.md) — OpenAI 对齐、`content` 与 `reasoning_content`。
- [architecture.zh.md](architecture.zh.md) — 组件分层与请求流。
