# cursor-brain

[![Crates.io](https://img.shields.io/crates/v/cursor-brain)](https://crates.io/crates/cursor-brain)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-1.70%2B-orange)](https://www.rust-lang.org/)

[English](README.md) | **中文**

基于 **Cursor Agent** 的 **OpenAI 兼容 HTTP 服务**。可作为 Openclaw、Ironclaw、Zeroclaw 或任意 OpenAI API 客户端的即插即用聊天/补全端点。支持 **Windows**、**Linux**、**macOS** 平台。

## 特性

- **OpenAI 兼容**：`POST /v1/chat/completions`（流式与非流式）、`GET /v1/models`、`GET /v1/health` 及相关端点。
- **Cursor Agent 后端**：以子进程方式调用 Cursor CLI；支持会话恢复与思考过程输出（`content` 或 `reasoning_content`）。
- **仅配置文件**：所有配置来自 `~/.cursor-brain/config.json`；首次运行会写入默认文件。
- **可观测**：请求 ID 头、JSON 指标（`/v1/metrics`）、带版本信息的健康检查。
- **Provider 就绪**：可注册为 Openclaw/Ironclaw/Zeroclaw 提供方；见 [Provider 兼容](doc/provider-compat.md)。

## 快速开始

推荐：全局安装后直接运行：

```bash
cargo install cursor-brain
cursor-brain
```

或从源码运行：先安装 [cursor-agent](#安装-cursor-agent)，再执行 `cargo run`。

默认地址：`http://localhost:3001`。请求示例：

```bash
curl -X POST http://localhost:3001/v1/chat/completions \
  -H "Content-Type: application/json" \
  -d '{"model":"auto","messages":[{"role":"user","content":"你好"}]}'
```

## 配置

配置**仅**从 `~/.cursor-brain/config.json` 读取（不使用环境变量）。首次运行时若文件不存在，会在此写入默认配置。

| 键                      | 说明                                    | 默认值                      |
| ----------------------- | --------------------------------------- | --------------------------- |
| `port`                  | HTTP 端口                               | 3001                        |
| `bind_address`          | 监听地址                                | `0.0.0.0`                   |
| `cursor_path`           | cursor-agent 路径                       | 自动检测                    |
| `request_timeout_sec`   | 单次请求超时（秒）                      | 300                         |
| `session_header_name`   | 会话 id 请求头名                        | `x-session-id`              |
| `minimal_workspace_dir` | Agent 工作目录（无 MCP）                | `~/.cursor-brain/workspace` |
| `agent_mode`            | `ask` 或 `agent`                        | `agent`                     |
| `forward_thinking`      | `off`、`content` 或 `reasoning_content` | `content`                   |

示例 `~/.cursor-brain/config.json`：

```json
{
  "port": 3001,
  "bind_address": "0.0.0.0",
  "request_timeout_sec": 300
}
```

完整配置项与默认值见 [设计与默认值](doc/DESIGN.md) 与 [教程（中文）](doc/tutorial.zh.md)。

## 安装 cursor-agent

cursor-brain **不**负责安装或升级 cursor-agent。请自行安装：

- **Linux / macOS**：`curl https://cursor.com/install -fsSL | bash`
- **Windows**：参见 [Cursor 文档](https://cursor.com)。确保 `agent` 在 PATH 中或在配置中设置 `cursor_path`。

## 平台支持

**Windows**、**Linux**、**macOS**。配置与 PID 使用用户主目录（Windows 为 `%USERPROFILE%\.cursor-brain`，Unix 为 `~/.cursor-brain`）。cursor-agent 检测与路径按系统区分，见 [设计与默认值](doc/DESIGN.md)。

## PID 文件

启动后进程会将 PID 写入 `~/.cursor-brain/cursor-brain.pid`（创建或截断），退出时删除。可用于单实例检测或监控。

## 文档

| 文档                                            | 说明                                                 |
| ----------------------------------------------- | ---------------------------------------------------- |
| [架构（中文）](doc/architecture.zh.md)          | 组件分层与请求流（含图示）                           |
| [教程（中文）](doc/tutorial.zh.md)              | 快速开始、配置、API 用法、部署                       |
| [设计与默认值](doc/DESIGN.md)                   | 设计决策、默认值、平台说明                           |
| [OpenAI 对齐与思考过程](doc/openai-protocol.md) | `content` 与 `reasoning_content`、`forward_thinking` |
| [Provider 兼容](doc/provider-compat.md)         | Openclaw、Ironclaw、Zeroclaw                         |
| [API 规范](doc/openapi.yaml)                    | OpenAPI 3.0 定义                                     |

**English**: [README](README.md) · [Architecture](doc/architecture.en.md) · [Tutorial](doc/tutorial.en.md)

## 注册为 Openclaw / Ironclaw / Zeroclaw 提供方

1. 启动 cursor-brain（如 `cargo run` 或 `cursor-brain`）。
2. 在 `~/.ironclaw/providers.json` 中添加提供方：将 [provider-definition.json](doc/provider-definition.json) 中的对象合并到数组中。
3. 在客户端的 LLM 设置中选择 **Cursor Brain** 并选择模型。

## 许可证

[MIT](LICENSE)。本项目代码注释仅使用英文。
