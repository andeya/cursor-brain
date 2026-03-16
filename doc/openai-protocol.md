# OpenAI API alignment and response fields

## Alignment with official API

The `openai` module aims to be **compatible** with the OpenAI Chat Completions API for request/response shapes and streaming SSE:

- **Implemented**: `POST /v1/chat/completions` (streaming and non-streaming), `GET /v1/models`, `GET /v1/models/:id`. Request body (e.g. `model`, `messages`, `stream`) and response structures follow the usual OpenAI schema where applicable.
- **Deviations**:
  - `POST /v1/embeddings` and `POST /v1/completions` return **501 Not Implemented** with `code: "not_implemented"` and a short message; they are not implemented by this service.
  - The service may add **extension fields** (e.g. `reasoning_content` on messages) that are not in the base OpenAI spec; see below.

Integrators can rely on the documented behavior above; any other divergence from the official API is considered a bug unless documented here.

## `content` vs `reasoning_content`

- **`content`**  
  The standard assistant message body. It is the main reply text (or array of content parts) as in the OpenAI API. It is always present in the response message when there is reply text. When `forward_thinking` is `"content"`, cursor-agent “thinking” output is merged into this field (e.g. prefixed with a “thinking” block).

- **`reasoning_content`**  
  An **optional extension** field for reasoning/thinking output (similar in spirit to extended reasoning in models like o1). When `forward_thinking` is `"reasoning_content"`, cursor-agent thinking is mapped into this field instead of being merged into `content`. Clients that support it can display reasoning separately from the main answer.

**`forward_thinking` config** (in `~/.cursor-brain/config.json`):

| Value               | Behavior                                                                            |
| ------------------- | ----------------------------------------------------------------------------------- |
| `off`               | Thinking is not returned.                                                           |
| `content` (default) | Thinking is merged into the assistant message `content` (e.g. with a block prefix). |
| `reasoning_content` | Thinking is returned in `reasoning_content`; `content` holds only the main reply.   |

Streaming: when `forward_thinking` is `reasoning_content`, SSE chunks may include `delta.reasoning_content` in addition to `delta.content`.
