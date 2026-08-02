//! JSON-RPC 2.0 server loop over stdio.

use crate::methods::{MethodRegistry, RpcContext};
use crate::protocol::{Id, Request, Response, RpcError, Value};
use rspk_core::ExecutionContext;
use rspk_managers::ManagerRegistry;
use rspk_needsfile::EvalContext;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tracing::{debug, warn};

/// A JSON-RPC 2.0 server reading from stdin and writing to stdout.
#[derive(Debug)]
pub struct Server
{
    methods: MethodRegistry,
    ctx:     RpcContext,
}

impl Server
{
    /// Creates a new server with the given registry and context.
    ///
    /// Also builds an [`EvalContext`] from the current platform and
    /// discovered managers, so that Needsfile conditions like
    /// `if os = linux && present "apt"` work correctly.
    pub fn new(registry: ManagerRegistry, exec_ctx: ExecutionContext) -> Self
    {
        // Build EvalContext from the registry: current OS + all
        // discovered manager IDs.
        let mut eval_ctx = EvalContext::for_current_platform();
        for mgr in registry.available()
        {
            eval_ctx = eval_ctx.with_manager(mgr.id());
        }

        Self {
            methods: MethodRegistry::builtin(),
            ctx:     RpcContext {
                registry: Arc::new(registry),
                ctx: exec_ctx,
                eval_ctx,
            },
        }
    }

    /// Runs the server loop until stdin is closed.
    pub async fn run(self) -> std::io::Result<()>
    {
        let stdin = tokio::io::stdin();
        let stdout = tokio::io::stdout();
        let mut reader = BufReader::new(stdin);
        let mut writer = stdout;

        let mut line = String::new();
        debug!("JSON-RPC server listening on stdio");

        loop
        {
            line.clear();
            let n = reader.read_line(&mut line).await?;
            if n == 0
            {
                debug!("stdin closed, shutting down");
                break;
            }
            let trimmed = line.trim();
            if trimmed.is_empty()
            {
                continue;
            }
            let response_text = self.handle_line(trimmed).await;
            // Notifications produce no output — skip writing entirely.
            if response_text.is_empty()
            {
                continue;
            }
            writer.write_all(response_text.as_bytes()).await?;
            writer.write_all(b"\n").await?;
            writer.flush().await?;
        }
        Ok(())
    }

    /// Dispatches one line, which may be a single request or a batch.
    async fn handle_line(&self, line: &str) -> String
    {
        let parsed: Result<Value, _> = serde_json::from_str(line);
        match parsed
        {
            Ok(Value::Array(batch)) =>
            {
                if batch.is_empty()
                {
                    let err = Response::error(
                        Id::Null,
                        RpcError::invalid_request("empty batch"),
                    );
                    return serde_json::to_string(&err).unwrap_or_default();
                }
                let mut responses = Vec::with_capacity(batch.len());
                for item in batch
                {
                    if let Ok(req) =
                        serde_json::from_value::<Request>(item.clone())
                    {
                        if let Some(resp) = self.dispatch(req).await
                        {
                            responses.push(resp);
                        }
                    }
                    else
                    {
                        responses.push(Response::error(
                            Id::Null,
                            RpcError::invalid_request(
                                "invalid request in batch",
                            ),
                        ));
                    }
                }
                serde_json::to_string(&responses).unwrap_or_default()
            },
            Ok(_) =>
            {
                let req: Result<Request, _> = serde_json::from_str(line);
                if let Ok(r) = req
                {
                    match self.dispatch(r).await
                    {
                        Some(resp) =>
                        {
                            serde_json::to_string(&resp).unwrap_or_default()
                        },
                        None => String::new(),
                    }
                }
                else
                {
                    let err = Response::error(
                        Id::Null,
                        RpcError::invalid_request("invalid JSON-RPC request"),
                    );
                    serde_json::to_string(&err).unwrap_or_default()
                }
            },
            Err(e) =>
            {
                let err = Response::error(
                    Id::Null,
                    RpcError::parse_error(format!("invalid JSON: {e}")),
                );
                serde_json::to_string(&err).unwrap_or_default()
            },
        }
    }

    /// Dispatches a single request and returns an optional response.
    async fn dispatch(&self, req: Request) -> Option<Response>
    {
        if req.jsonrpc != "2.0"
        {
            return Some(Response::error(
                req.id.unwrap_or(Id::Null),
                RpcError::invalid_request("jsonrpc must be \"2.0\""),
            ));
        }
        let Some(id) = req.id
        else
        {
            let _ = self
                .methods
                .call(self.ctx.clone(), &req.method, req.params)
                .await;
            return None;
        };

        debug!(method = %req.method, "dispatch");
        match self
            .methods
            .call(self.ctx.clone(), &req.method, req.params)
            .await
        {
            Ok(value) => Some(Response::success(id, value)),
            Err(e) =>
            {
                warn!(method = %req.method, error = %e, "method failed");
                Some(Response::error(id, e))
            },
        }
    }
}
