//! Business layer: maps requests to completion input, resolves session, spawns cursor-agent via cursor,
//! builds OpenAI-format responses. Depends on config, session store, and openai types.

use crate::config::Config;
use crate::cursor::{
    run_to_completion, run_to_completion_stream, spawn_cursor_agent, CompletionOutput,
    SpawnOptions, StreamDelta,
};
use crate::openai::{extract_user_message, format_messages_as_prompt, ChatCompletionRequest};
use crate::session::SessionStore;
use axum::http::HeaderMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;

#[derive(Clone)]
pub struct CompletionInput {
    pub user_msg: String,
    pub model: String,
    pub stream: bool,
    pub external_session_id: Option<String>,
}

impl CompletionInput {
    pub fn from_request(
        body: &ChatCompletionRequest,
        headers: &HeaderMap,
        session_header_name: &str,
        default_model: &str,
    ) -> Result<Self, CompletionError> {
        let user_msg = if body.messages.len() > 1 {
            format_messages_as_prompt(&body.messages)
        } else {
            extract_user_message(&body.messages)
        };
        if user_msg.trim().is_empty() {
            return Err(CompletionError::InvalidRequest(
                "no user message in messages".to_string(),
            ));
        }
        let model = body
            .model
            .as_deref()
            .filter(|s| !s.is_empty())
            .unwrap_or(default_model)
            .to_string();
        let stream = body.stream.unwrap_or(false);
        let external_session_id = headers
            .get(session_header_name)
            .and_then(|v| v.to_str().ok())
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());
        Ok(CompletionInput {
            user_msg,
            model,
            stream,
            external_session_id,
        })
    }
}

#[derive(Debug)]
pub enum CompletionError {
    CursorNotFound,
    InvalidRequest(String),
    NoContent,
    SpawnFailed(std::io::Error),
    JoinFailed(String),
}

pub struct CompletionService {
    config: Arc<Config>,
    session_store: Arc<dyn SessionStore>,
    timeout: Duration,
}

impl CompletionService {
    pub fn new(config: Arc<Config>, session_store: Arc<dyn SessionStore>) -> Self {
        let timeout = Duration::from_secs(config.request_timeout_sec);
        Self {
            config,
            session_store,
            timeout,
        }
    }

    pub fn cursor_path(&self) -> Result<String, CompletionError> {
        self.config
            .resolve_cursor_path()
            .ok_or(CompletionError::CursorNotFound)
    }

    fn spawn_options(&self) -> SpawnOptions {
        SpawnOptions {
            workspace_dir: self.config.workspace_dir_for_spawn(),
            sandbox: self.config.sandbox.clone(),
        }
    }

    pub async fn complete(
        &self,
        input: CompletionInput,
    ) -> Result<(CompletionOutput, String, String), CompletionError> {
        let cursor_path = self.cursor_path()?;
        let resume_session_id = if let Some(ref ext) = input.external_session_id {
            self.session_store.get(ext).await
        } else {
            None
        };

        let (session_tx, mut session_rx) = mpsc::channel::<(String, String)>(4);
        let store = self.session_store.clone();
        tokio::spawn(async move {
            while let Some((ext, cur)) = session_rx.recv().await {
                store.put(ext, cur).await;
            }
        });

        // One completion id per request; must not change across retries (no_content, fallback_model).
        let id = format!("chatcmpl-{}", uuid::Uuid::new_v4().to_simple());
        let mut resume = resume_session_id;
        let mut no_content_retried = false;
        let mut fallback_retried = false;
        let mut current_model = input.model.clone();
        let fallback_model = self.config.fallback_model.clone();
        let options = self.spawn_options();
        let forward_thinking = self.config.forward_thinking.clone();

        let out = loop {
            let resume_this = resume.clone();
            let external_session_this = input.external_session_id.clone();
            let session_tx_this = session_tx.clone();
            let cursor_path_this = cursor_path.clone();
            let user_msg_this = input.user_msg.clone();
            let model_this = current_model.clone();
            let timeout = self.timeout;
            let options_this = options.clone();
            let forward_thinking_this = forward_thinking.clone();

            let result = tokio::task::spawn_blocking(move || {
                let mut on_session_id = |cursor_id: &str| {
                    if let Some(ref ext) = external_session_this {
                        let _ = session_tx_this.blocking_send((ext.clone(), cursor_id.to_string()));
                    }
                };
                let mut child = spawn_cursor_agent(
                    &cursor_path_this,
                    &user_msg_this,
                    Some(&model_this),
                    resume_this.as_deref(),
                    &options_this,
                )
                .map_err(CompletionError::SpawnFailed)?;
                run_to_completion(
                    &mut child,
                    timeout,
                    &forward_thinking_this,
                    Some(&mut on_session_id),
                )
                .map_err(CompletionError::SpawnFailed)
            })
            .await
            .map_err(|e| CompletionError::JoinFailed(e.to_string()))?;

            let out = result?;
            let empty = out.content.is_empty()
                && out.thinking_text.is_empty()
                && out
                    .reasoning_content
                    .as_deref()
                    .is_none_or(|s| s.is_empty());
            if empty && input.external_session_id.is_some() && resume.is_some() {
                if let Some(ref ext) = input.external_session_id {
                    self.session_store.remove(ext).await;
                }
                resume = None;
                continue;
            }
            if empty && !no_content_retried {
                no_content_retried = true;
                tracing::info!("cursor-agent returned no content; retrying once without resume");
                continue;
            }
            if empty
                && !fallback_retried
                && fallback_model
                    .as_deref()
                    .is_some_and(|fb| fb != current_model)
            {
                fallback_retried = true;
                current_model = fallback_model.clone().unwrap_or_default();
                tracing::info!(
                    "cursor-agent returned no content; retrying with fallback_model {}",
                    current_model
                );
                continue;
            }
            if empty {
                return Err(CompletionError::NoContent);
            }
            break out;
        };
        Ok((out, current_model, id))
    }

    pub async fn complete_stream(
        &self,
        input: CompletionInput,
    ) -> Result<(String, String, mpsc::Receiver<StreamDelta>), CompletionError> {
        let cursor_path = self.cursor_path()?;
        let resume_session_id = if let Some(ref ext) = input.external_session_id {
            self.session_store.get(ext).await
        } else {
            None
        };

        let (session_tx, mut session_rx) = mpsc::channel::<(String, String)>(4);
        let store = self.session_store.clone();
        tokio::spawn(async move {
            while let Some((ext, cur)) = session_rx.recv().await {
                store.put(ext, cur).await;
            }
        });

        let (tx, rx) = mpsc::channel::<StreamDelta>(32);
        let id = format!("chatcmpl-{}", uuid::Uuid::new_v4().to_simple());
        let cursor_path_clone = cursor_path.clone();
        let model_owned = input.model.clone();
        let user_msg = input.user_msg.clone();
        let external_session_for_spawn = input.external_session_id.clone();
        let timeout = self.timeout;
        let options = self.spawn_options();
        let forward_thinking = self.config.forward_thinking.clone();

        // Spawn runs in background; spawn/run errors are delivered via StreamDelta::Done
        // (finish_reason e.g. "spawn_error: ...") so the client can see them on the stream.
        tokio::task::spawn_blocking(move || {
            let mut on_session_id = |cursor_id: &str| {
                if let Some(ref ext) = external_session_for_spawn {
                    let _ = session_tx.blocking_send((ext.clone(), cursor_id.to_string()));
                }
            };
            let mut child = match spawn_cursor_agent(
                &cursor_path_clone,
                &user_msg,
                Some(&model_owned),
                resume_session_id.as_deref(),
                &options,
            ) {
                Ok(c) => c,
                Err(e) => {
                    let _ = tx.blocking_send(StreamDelta::Done {
                        finish_reason: format!("spawn_error: {}", e),
                    });
                    return;
                }
            };
            let _ = run_to_completion_stream(
                &mut child,
                timeout,
                &forward_thinking,
                |delta| {
                    let _ = tx.blocking_send(delta);
                },
                Some(&mut on_session_id),
            );
        });
        Ok((id, input.model, rx))
    }
}
