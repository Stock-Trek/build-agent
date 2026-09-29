use crate::{
    args::Args,
    command::Command,
    exit_codes::*,
    external::{
        enqueue_action, enqueue_metadata, enqueue_preferences, enqueue_signals, enqueue_strategy,
    },
    generated::dto::{PortfolioDto, StrategyContextDto},
};
use prost::Message;
use stock_trek::{
    Algorithm, ResolvedContext, StrategyContext, prelude::traitreg, signals::SignalContext,
};

#[traitreg::registry(Algorithm)]
static ALGORITHM_REGISTRY: () = ();

pub fn run(args: Args) -> i32 {
    let registered_algorithm_opt = ALGORITHM_REGISTRY
        .iter()
        .find(|registration| registration.name() == args.strategy_name);
    let registered_algorithm = match registered_algorithm_opt {
        Some(algorithm) => algorithm,
        None => return STRATEGY_NOT_FOUND,
    };
    if !registered_algorithm.has_constructor() {
        return STRATEGY_HAS_NO_CONSTRUCTOR;
    };
    let algorithm = match registered_algorithm.instanciate() {
        Some(algorithm) => algorithm,
        None => return STRATEGY_COULD_NOT_INSTANTIATE,
    };
    match args.command {
        Command::Algorithm => cmd_algorithm(algorithm.as_ref()),
        Command::Execute => cmd_execute(
            algorithm.as_ref(),
            &args.strategy_context_path,
            &args.portfolio_path,
        ),
        Command::Metadata => cmd_metadata(algorithm.as_ref()),
        Command::Preferences => cmd_preferences(algorithm.as_ref()),
        Command::Signals => cmd_signals(algorithm.as_ref(), &args.strategy_context_path),
    }
}

fn cmd_algorithm(algorithm: &dyn Algorithm) -> i32 {
    let command = algorithm.strategy(&StrategyContext::new());
    match enqueue_strategy(command) {
        Err(_) => SERVER_ENTRY_FAILED_TO_ENQUEUE_JSON,
        Ok(_) => SUCCESS,
    }
}

fn cmd_execute(
    algorithm: &dyn Algorithm,
    strategy_context_path: &String,
    portfolio_path: &String,
) -> i32 {
    let send_metadata_result = cmd_metadata(algorithm);
    if send_metadata_result != SUCCESS {
        return send_metadata_result;
    }
    let signal_context = match load_signal_context(strategy_context_path) {
        Ok(ctx) => ctx,
        Err(code) => return code,
    };
    let portfolio_data = match std::fs::read(portfolio_path) {
        Ok(data) => data,
        Err(_) => return SERVER_RUNNER_PORTFOLIO_FILE_NOT_FOUND,
    };
    let portfolio_dto = match PortfolioDto::decode(&portfolio_data[..]) {
        Ok(dto) => dto,
        Err(_) => return SERVER_RUNNER_PORTFOLIO_NOT_DECODED,
    };
    let portfolio = match portfolio_dto.try_into() {
        Ok(p) => p,
        Err(_) => return SERVER_RUNNER_PORTFOLIO_NOT_CREATED,
    };
    let signals = algorithm.signals(&signal_context);
    let strategy_context = StrategyContext::new();
    let command = algorithm.strategy(&strategy_context);
    let mut resolved_context = ResolvedContext {
        enqueue_action: Box::new(enqueue_action),
        portfolio,
        signals,
    };
    match command.execute(&mut resolved_context) {
        Err(_) => BOT_ERROR_RESOLVING_ACTION,
        Ok(_) => SUCCESS,
    }
}

fn cmd_metadata(algorithm: &dyn Algorithm) -> i32 {
    let name = algorithm.name();
    let description = algorithm.description();
    match enqueue_metadata(name, description) {
        Err(_) => SERVER_ENTRY_FAILED_TO_ENQUEUE_JSON,
        Ok(_) => SUCCESS,
    }
}

fn cmd_preferences(algorithm: &dyn Algorithm) -> i32 {
    let preferences = algorithm.preferences();
    match enqueue_preferences(preferences) {
        Err(_) => SERVER_ENTRY_FAILED_TO_ENQUEUE_JSON,
        Ok(_) => SUCCESS,
    }
}

fn cmd_signals(algorithm: &dyn Algorithm, strategy_context_path: &String) -> i32 {
    let signal_context = match load_signal_context(strategy_context_path) {
        Ok(ctx) => ctx,
        Err(code) => return code,
    };
    let signals = algorithm.signals(&signal_context);
    match enqueue_signals(signals) {
        Err(_) => SERVER_ENTRY_FAILED_TO_ENQUEUE_JSON,
        Ok(_) => SUCCESS,
    }
}

fn load_signal_context(strategy_context_path: &String) -> Result<SignalContext, i32> {
    let strategy_context_data = std::fs::read(strategy_context_path)
        .map_err(|_| SERVER_RUNNER_STRATEGY_CONTEXT_FILE_NOT_FOUND)?;
    let strategy_context_dto = StrategyContextDto::decode(&strategy_context_data[..])
        .map_err(|_| SERVER_RUNNER_STRATEGY_CONTEXT_NOT_DECODED)?;
    strategy_context_dto
        .try_into()
        .map_err(|_| SERVER_RUNNER_STRATEGY_CONTEXT_NOT_CREATED)
}
