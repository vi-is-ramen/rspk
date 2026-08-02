//! JSON-RPC 2.0 wire protocol types.

use serde::{Deserialize, Serialize};

/// JSON value alias.
pub type Value = serde_json::Value;

/// Request identifier — can be a number, string, or null.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Id
{
    /// Numeric ID.
    Number(i64),
    /// String ID.
    String(String),
    /// Null (rare, but spec-compliant).
    Null,
}

/// A JSON-RPC 2.0 request.
#[derive(Debug, Clone, Deserialize)]
pub struct Request
{
    /// Must be `"2.0"`.
    pub jsonrpc: String,
    /// Method name.
    pub method:  String,
    /// Optional parameters (object or array).
    #[serde(default)]
    pub params:  Option<Value>,
    /// Request ID. Absent for notifications.
    pub id:      Option<Id>,
}

/// A JSON-RPC 2.0 successful response.
#[derive(Debug, Clone, Serialize)]
pub struct Response
{
    /// Always `"2.0"`.
    pub jsonrpc: &'static str,
    /// Result payload (present on success).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result:  Option<Value>,
    /// Error payload (present on failure).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error:   Option<RpcError>,
    /// Echo of the request ID.
    pub id:      Id,
}

impl Response
{
    /// Builds a success response.
    pub fn success(id: Id, result: Value) -> Self
    {
        Self {
            jsonrpc: "2.0",
            result: Some(result),
            error: None,
            id,
        }
    }

    /// Builds an error response.
    pub fn error(id: Id, error: RpcError) -> Self
    {
        Self {
            jsonrpc: "2.0",
            result: None,
            error: Some(error),
            id,
        }
    }
}

/// A JSON-RPC 2.0 error object.
#[derive(Debug, Clone, Serialize, thiserror::Error)]
#[error("JSON-RPC error {code}: {message}")]
pub struct RpcError
{
    /// Numeric error code.
    pub code:    i32,
    /// Human-readable error message.
    pub message: String,
    /// Optional structured data.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data:    Option<Value>,
}

impl RpcError
{
    /// Creates an error with the given code and message.
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self
    {
        Self {
            code:    code as i32,
            message: message.into(),
            data:    None,
        }
    }

    /// Attaches structured data to the error.
    #[must_use]
    pub fn with_data(mut self, data: Value) -> Self
    {
        self.data = Some(data);
        self
    }

    /// `-32700`: Invalid JSON was received.
    pub fn parse_error(message: impl Into<String>) -> Self
    {
        Self::new(ErrorCode::ParseError, message)
    }

    /// `-32600`: The JSON sent is not a valid Request object.
    pub fn invalid_request(message: impl Into<String>) -> Self
    {
        Self::new(ErrorCode::InvalidRequest, message)
    }

    /// `-32601`: The method does not exist.
    pub fn method_not_found(method: &str) -> Self
    {
        Self::new(
            ErrorCode::MethodNotFound,
            format!("method not found: {method}"),
        )
    }

    /// `-32602`: Invalid method parameters.
    pub fn invalid_params(message: impl Into<String>) -> Self
    {
        Self::new(ErrorCode::InvalidParams, message)
    }

    /// `-32603`: Internal JSON-RPC error.
    pub fn internal(message: impl Into<String>) -> Self
    {
        Self::new(ErrorCode::InternalError, message)
    }
}

/// Standard JSON-RPC 2.0 error codes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum ErrorCode
{
    /// `-32700`: Invalid JSON was received by the server.
    ParseError     = -32700,
    /// `-32600`: The JSON sent is not a valid Request object.
    InvalidRequest = -32600,
    /// `-32601`: The method does not exist / is not available.
    MethodNotFound = -32601,
    /// `-32602`: Invalid method parameter(s).
    InvalidParams  = -32602,
    /// `-32603`: Internal JSON-RPC error.
    InternalError  = -32603,
}
