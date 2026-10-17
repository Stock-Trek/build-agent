pub const SUCCESS: i32 = 0;

// strategy errors
pub const STRATEGY_NOT_FOUND: i32 = 1;
pub const STRATEGY_HAS_NO_CONSTRUCTOR: i32 = 2;
pub const STRATEGY_COULD_NOT_INSTANTIATE: i32 = 3;

// server errors
pub const SERVER_ENTRY_INCORRECT_ARG_COUNT: i32 = 101;
pub const SERVER_ENTRY_FAILED_TO_PARSE_COMMAND: i32 = 102;
pub const SERVER_ENTRY_FAILED_TO_CONVERT_TO_JSON: i32 = 103;
pub const SERVER_ENTRY_FAILED_TO_SET_LINKED_FUNCTION: i32 = 104;
pub const SERVER_ENTRY_FAILED_TO_FIND_LINKED_FUNCTION: i32 = 105;
pub const SERVER_ENTRY_FAILED_TO_ENQUEUE_JSON: i32 = 106;
pub const SERVER_ENTRY_FAILED_TO_PARSE_REQUEST: i32 = 107;
pub const SERVER_ENTRY_INVALID_ARGUMENT: i32 = 108;

pub const SERVER_RUNNER_STRATEGY_CONTEXT_FILE_NOT_FOUND: i32 = 111;
pub const SERVER_RUNNER_STRATEGY_CONTEXT_NOT_DECODED: i32 = 112;
pub const SERVER_RUNNER_STRATEGY_CONTEXT_NOT_CREATED: i32 = 113;

pub const SERVER_RUNNER_PORTFOLIO_FILE_NOT_FOUND: i32 = 121;
pub const SERVER_RUNNER_PORTFOLIO_NOT_DECODED: i32 = 122;
pub const SERVER_RUNNER_PORTFOLIO_NOT_CREATED: i32 = 123;

pub const SERVER_RUNNER_CONNECTION_FAILED: i32 = 131;

// bot errors
pub const BOT_ERROR_CREATING_RESOLVER: i32 = 201;
pub const BOT_ERROR_CREATING_SCRATCH_PAD: i32 = 202;
pub const BOT_ERROR_RESOLVING_ACTION: i32 = 203;
pub const BOT_ERROR_PANIC: i32 = 204;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClientError {
    InvalidRequest,
    StrategyNotFound,
    StrategyContextNotFound,
    StrategyContextNotDecoded,
    StrategyContextNotCreated,
    PortfolioNotFound,
    PortfolioNotDecoded,
    PortfolioNotCreated,
}

impl ClientError {
    pub fn from_code(code: i32) -> Option<Self> {
        Some(match code {
            SERVER_ENTRY_FAILED_TO_PARSE_REQUEST
            | SERVER_ENTRY_FAILED_TO_PARSE_COMMAND
            | SERVER_ENTRY_INVALID_ARGUMENT => Self::InvalidRequest,
            STRATEGY_NOT_FOUND => Self::StrategyNotFound,
            SERVER_RUNNER_STRATEGY_CONTEXT_FILE_NOT_FOUND => Self::StrategyContextNotFound,
            SERVER_RUNNER_STRATEGY_CONTEXT_NOT_DECODED => Self::StrategyContextNotDecoded,
            SERVER_RUNNER_STRATEGY_CONTEXT_NOT_CREATED => Self::StrategyContextNotCreated,
            SERVER_RUNNER_PORTFOLIO_FILE_NOT_FOUND => Self::PortfolioNotFound,
            SERVER_RUNNER_PORTFOLIO_NOT_DECODED => Self::PortfolioNotDecoded,
            SERVER_RUNNER_PORTFOLIO_NOT_CREATED => Self::PortfolioNotCreated,
            _ => return None,
        })
    }
}

pub fn is_client_error(code: i32) -> bool {
    ClientError::from_code(code).is_some()
}

pub fn is_invalid_request(code: i32) -> bool {
    is_client_error(code)
}
