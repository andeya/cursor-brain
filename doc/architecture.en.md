# Architecture (English)

## Overview

cursor-brain is an **OpenAI-compatible HTTP service** backed by Cursor Agent. Stack: Rust, Axum, tokio.

- **Layers**: HTTP (server) → business (service) → cursor subprocess (cursor) + session (session) + OpenAI types (openai); config and metrics are shared.
- **Data flow**: Request → middleware (X-Request-Id, logging, metrics) → route → service (session resolve, spawn cursor-agent, stream or buffered) → response.

## Component layers

<img src="architecture.en-mermaid/Untitled-1.png" width="640" alt="Component layers diagram" />

<details>
<summary>Mermaid source</summary>

```mermaid
flowchart TB
    subgraph HTTP["HTTP layer (server)"]
        Routes["/v1/chat/completions, /v1/models, /v1/health, /v1/agent/*, ..."]
        Err["Unified error body"]
        Mid["Request-ID, logging, metrics"]
    end
    subgraph Business["Business layer (service)"]
        Input["Parse request → CompletionInput"]
        Session["Resolve session (external ↔ cursor id)"]
        Spawn["Spawn cursor-agent"]
        Response["Build OpenAI response / SSE"]
    end
    subgraph Backends["Backends"]
        Cursor["cursor: subprocess spawn, stream-json parse"]
        Store["session: PersistentSessionStore"]
        OpenAI["openai: request/response types, SSE format"]
    end
    Config["config: port, timeout, paths, options"]
    Metrics["metrics: counters"]
    Routes --> Mid
    Mid --> Input
    Input --> Session
    Session --> Spawn
    Spawn --> Cursor
    Cursor --> Response
    Response --> OpenAI
    Config --> Routes
    Config --> Input
    Config --> Spawn
    Metrics --> Mid
    Store --> Session
```

</details>

## Request flow (chat completion)

<img src="architecture.en-mermaid/Untitled-2.png" width="640" alt="Request flow sequence diagram" />

<details>
<summary>Mermaid source</summary>

```mermaid
sequenceDiagram
    participant Client
    participant Server
    participant Service
    participant Session
    participant Cursor
    participant Agent
    Client->>Server: POST /v1/chat/completions
    Server->>Server: X-Request-Id, inc metrics
    Server->>Service: CompletionInput (user_msg, model, stream, session_id)
    Service->>Session: get(external_id) or put
    Session-->>Service: cursor session_id or new
    Service->>Cursor: spawn_cursor_agent(user_msg, session_id, options)
    Cursor->>Agent: subprocess stdin
    Agent-->>Cursor: stdout stream-json
    Cursor-->>Service: CompletionOutput (content, thinking, reasoning_content)
    Service-->>Server: JSON or SSE stream
    Server-->>Client: 200 + body or stream
```

</details>

## Module boundaries

| Module      | Role                                                                                                      |
| ----------- | --------------------------------------------------------------------------------------------------------- |
| **main**    | Entry: load config, ensure workspace dir, write PID, start server.                                        |
| **config**  | Single source of defaults; load from `~/.cursor-brain/config.json` only; write default file on first run. |
| **server**  | HTTP: routes, error body, middleware. Uses service, session, cursor, config, metrics.                     |
| **service** | Business: build CompletionInput, resolve session, spawn via cursor, build OpenAI response.                |
| **cursor**  | Subprocess: spawn cursor-agent, stream-json parsing, list-models, version, agent subcommands.             |
| **session** | Storage: external session id ↔ cursor session_id; persisted to `~/.cursor-brain/sessions.json`.           |
| **openai**  | Types and formatting only: ChatCompletionRequest, build_completion_response, SSE chunks.                  |
| **metrics** | In-memory counters for GET /v1/metrics.                                                                   |

## See also

- [DESIGN.md](DESIGN.md) — design decisions, defaults, PID, platform support.
- [openai-protocol.md](openai-protocol.md) — API alignment, `content` vs `reasoning_content`.
- [tutorial.en.md](tutorial.en.md) — quick start, configuration, API usage, deployment.
