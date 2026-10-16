use crate::exit_codes::*;

pub type BaResult<T> = Result<T, BaError>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BaError {
    StrategyNotFound,
    StrategyHasNoConstructor,
    StrategyCouldNotInstantiate,
    FailedToEnqueueJson,
    StrategyContextFileNotFound,
    StrategyContextNotDecoded,
    StrategyContextNotCreated,
    PortfolioFileNotFound,
    PortfolioNotDecoded,
    PortfolioNotCreated,
    ResolvingAction,
}

impl BaError {
    pub fn exit_code(self) -> i32 {
        match self {
            BaError::StrategyNotFound => STRATEGY_NOT_FOUND,
            BaError::StrategyHasNoConstructor => STRATEGY_HAS_NO_CONSTRUCTOR,
            BaError::StrategyCouldNotInstantiate => STRATEGY_COULD_NOT_INSTANTIATE,
            BaError::FailedToEnqueueJson => SERVER_ENTRY_FAILED_TO_ENQUEUE_JSON,
            BaError::StrategyContextFileNotFound => SERVER_RUNNER_STRATEGY_CONTEXT_FILE_NOT_FOUND,
            BaError::StrategyContextNotDecoded => SERVER_RUNNER_STRATEGY_CONTEXT_NOT_DECODED,
            BaError::StrategyContextNotCreated => SERVER_RUNNER_STRATEGY_CONTEXT_NOT_CREATED,
            BaError::PortfolioFileNotFound => SERVER_RUNNER_PORTFOLIO_FILE_NOT_FOUND,
            BaError::PortfolioNotDecoded => SERVER_RUNNER_PORTFOLIO_NOT_DECODED,
            BaError::PortfolioNotCreated => SERVER_RUNNER_PORTFOLIO_NOT_CREATED,
            BaError::ResolvingAction => BOT_ERROR_RESOLVING_ACTION,
        }
    }
}
