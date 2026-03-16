# Provider 兼容性

cursor-brain 提供 OpenAI 兼容 HTTP API，可作为 **ironclaw**、**zeroclaw**、**openclaw** 的 LLM provider 使用。

## 端点与契约

| 端点 | 方法 | 说明 |
|------|------|------|
| /v1/chat/completions | POST | 流式 / 非流式；支持 session（X-Session-Id） |
| /v1/models | GET | 模型列表（cursor-agent --list-models） |
| /v1/models/:id | GET | 单模型查询 |
| /v1/health | GET | 健康与版本（cursor_agent_version、cursor_brain_version） |
| /v1/version | GET | cursor-agent 版本 |
| /v1/agent/about, /v1/agent/status, /v1/agent/sessions | GET | agent 子命令 |
| /v1/agent/chats | POST | 创建空会话 |
| /v1/embeddings | POST | 501 Not Implemented |
| /v1/completions | POST | 501 Not Implemented |
| /v1/metrics | GET | JSON 指标 |

## Ironclaw

- **契约**：protocol `open_ai_completions`，需 POST /v1/chat/completions（stream 与非流式）、GET /v1/models、GET /v1/health。
- **满足情况**：✓ 已满足。cursor-brain 默认端口 3001，与 ironclaw 约定一致。
- **注册**：将 [provider-definition.json](provider-definition.json) 中的对象合并到 `~/.ironclaw/providers.json` 数组中；在 Ironclaw 向导中选择 **Cursor Brain** 并选择模型。

## Zeroclaw

- **契约**：若 zeroclaw 复用与 Ironclaw 相同的 OpenAI 兼容协议与端点，则与 ironclaw 一致。
- **满足情况**：✓ 可同样将 cursor-brain 注册为 provider，使用相同 provider 定义与 base_url（如 http://127.0.0.1:3001/v1）。

## Openclaw

- **契约**：通过 gateway 调用 proxy；proxy 需提供 chat/completions、models、health。
- **满足情况**：✓ cursor-brain 作为独立 HTTP 服务可被 gateway 配置为 upstream；端点与 openclaw-cursor-brain proxy 兼容。thinking 行为可通过 `forward_thinking`（off / content / reasoning_content）与 openclaw 的 forwardThinking 对齐。

## 已知缺口与改进建议

| 项目 | 说明 | 建议 |
|------|------|------|
| API Key | 当前不校验 API key | 可选：在配置中增加 api_key，请求头校验 |
| Zeroclaw 特定字段 | 若 zeroclaw 有额外请求/响应字段 | 按需在文档与实现中补充 |
| Openclaw gateway 配置 | 需在 openclaw 侧配置 cursor-brain 的 URL | 在 openclaw 文档中说明将 base_url 指向 cursor-brain |
