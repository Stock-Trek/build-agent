use crate::command::Command;

pub struct Args {
    pub command: Command,
    pub strategy_name: String,
    pub strategy_context_path: String,
    pub portfolio_path: String,
}
