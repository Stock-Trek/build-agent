use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CompileStatus {
    Success,
    Failure,
    Error,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CompileResult {
    pub result: CompileStatus,
    pub errors: Vec<String>,
    pub compile_messages: Vec<CompileMessage>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CompileMessage {
    pub file: String,
    pub level: String,
    pub start: CodeLocation,
    pub end: CodeLocation,
    pub message: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CodeLocation {
    pub line: i64,
    pub column: i64,
}
