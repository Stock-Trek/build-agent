use std::fmt::Debug;

#[derive(Debug, thiserror::Error)]
pub enum BAError {
    #[error("Config: {0}")]
    Config(String),
}
