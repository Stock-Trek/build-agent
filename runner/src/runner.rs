use crate::{
    command::Command,
    error::{BaError, BaResult},
    exit_codes::SUCCESS,
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

pub fn run(command: Command) -> i32 {
    let result = select_algorithm(command.strategy_name()).and_then(|algorithm| match &command {
        Command::Algorithm { .. } => cmd_algorithm(algorithm.as_ref()),
        Command::Execute {
            strategy_context_path,
            portfolio_path,
            ..
        } => cmd_execute(algorithm.as_ref(), strategy_context_path, portfolio_path),
        Command::Metadata { .. } => cmd_metadata(algorithm.as_ref()),
        Command::Preferences { .. } => cmd_preferences(algorithm.as_ref()),
        Command::Signals {
            strategy_context_path,
            ..
        } => cmd_signals(algorithm.as_ref(), strategy_context_path),
    });
    match result {
        Ok(()) => SUCCESS,
        Err(error) => error.exit_code(),
    }
}

fn select_algorithm(strategy_name: &str) -> BaResult<Box<dyn Algorithm>> {
    let mut has_constructorless_registration = false;
    let mut has_instantiable_registration = false;
    let mut failed_to_instantiate = false;
    for registration in ALGORITHM_REGISTRY.iter() {
        if !registration.has_constructor() {
            has_constructorless_registration = true;
            continue;
        }
        has_instantiable_registration = true;
        match registration.instanciate() {
            Some(algorithm) if algorithm.name() == strategy_name => return Ok(algorithm),
            Some(_) => {}
            None => failed_to_instantiate = true,
        }
    }
    if failed_to_instantiate {
        return Err(BaError::StrategyCouldNotInstantiate);
    }
    if !has_instantiable_registration && has_constructorless_registration {
        return Err(BaError::StrategyHasNoConstructor);
    }
    Err(BaError::StrategyNotFound)
}

fn cmd_algorithm(algorithm: &dyn Algorithm) -> BaResult<()> {
    let command = algorithm.strategy(&StrategyContext::new());
    enqueue_strategy(command).map_err(|_| BaError::FailedToEnqueueJson)
}

fn cmd_execute(
    algorithm: &dyn Algorithm,
    strategy_context_path: &str,
    portfolio_path: &str,
) -> BaResult<()> {
    cmd_metadata(algorithm)?;
    let signal_context = load_signal_context(strategy_context_path)?;
    let portfolio_data =
        std::fs::read(portfolio_path).map_err(|_| BaError::PortfolioFileNotFound)?;
    let portfolio_dto =
        PortfolioDto::decode(&portfolio_data[..]).map_err(|_| BaError::PortfolioNotDecoded)?;
    let portfolio = portfolio_dto
        .try_into()
        .map_err(|_| BaError::PortfolioNotCreated)?;
    let signals = algorithm.signals(&signal_context);
    let strategy_context = StrategyContext::new();
    let command = algorithm.strategy(&strategy_context);
    let mut resolved_context = ResolvedContext {
        enqueue_action: Box::new(enqueue_action),
        portfolio,
        signals,
    };
    command
        .execute(&mut resolved_context)
        .map_err(|_| BaError::ResolvingAction)
}

fn cmd_metadata(algorithm: &dyn Algorithm) -> BaResult<()> {
    let name = algorithm.name();
    let description = algorithm.description();
    enqueue_metadata(name, description).map_err(|_| BaError::FailedToEnqueueJson)
}

fn cmd_preferences(algorithm: &dyn Algorithm) -> BaResult<()> {
    let preferences = algorithm.preferences();
    enqueue_preferences(preferences).map_err(|_| BaError::FailedToEnqueueJson)
}

fn cmd_signals(algorithm: &dyn Algorithm, strategy_context_path: &str) -> BaResult<()> {
    let signal_context = load_signal_context(strategy_context_path)?;
    let signals = algorithm.signals(&signal_context);
    enqueue_signals(signals).map_err(|_| BaError::FailedToEnqueueJson)
}

fn load_signal_context(strategy_context_path: &str) -> BaResult<SignalContext> {
    let strategy_context_data =
        std::fs::read(strategy_context_path).map_err(|_| BaError::StrategyContextFileNotFound)?;
    let strategy_context_dto = StrategyContextDto::decode(&strategy_context_data[..])
        .map_err(|_| BaError::StrategyContextNotDecoded)?;
    strategy_context_dto
        .try_into()
        .map_err(|_| BaError::StrategyContextNotCreated)
}
