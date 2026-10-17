pub use crate::exit_codes::ExitCode as BaError;

pub type BaResult<T> = Result<T, BaError>;
