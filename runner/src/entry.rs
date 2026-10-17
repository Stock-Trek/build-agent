use crate::{command::Command, exit_codes::ExitCode, runner::Runner};
use std::{ffi::CStr, os::raw::c_char};

const EXPECTED_ARG_COUNT: i32 = 5;

pub struct Entry;

impl Entry {
    /// # Safety
    /// This is called from the host binary which must ensure correct args
    pub unsafe fn run(argc: i32, argv: *const *const c_char) -> i32 {
        if argc != EXPECTED_ARG_COUNT {
            eprintln!(
                "Usage: program <command> <strategy_name> <strategy_context_path> <portfolio_path>"
            );
            return ExitCode::ServerEntryIncorrectArgCount.code();
        }
        let args: Vec<String> = (0..argc)
            .map(|i| {
                let arg_ptr = unsafe { *argv.offset(i as isize) };
                unsafe { CStr::from_ptr(arg_ptr).to_string_lossy().into_owned() }
            })
            .collect();
        let command = match args[1].as_str() {
            "Algorithm" => Command::Algorithm {
                strategy_name: args[2].clone(),
            },
            "Execute" => Command::Execute {
                strategy_name: args[2].clone(),
                strategy_context_path: args[3].clone(),
                portfolio_path: args[4].clone(),
            },
            "Metadata" => Command::Metadata {
                strategy_name: args[2].clone(),
            },
            "Preferences" => Command::Preferences {
                strategy_name: args[2].clone(),
            },
            "Signals" => Command::Signals {
                strategy_name: args[2].clone(),
                strategy_context_path: args[3].clone(),
            },
            _ => return ExitCode::ServerEntryFailedToParseCommand.code(),
        };
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| Runner::run(command)))
            .unwrap_or(ExitCode::BotErrorPanic.code())
    }
}

/// # Safety
/// This is called from the host binary which must ensure correct args
#[unsafe(no_mangle)]
pub unsafe extern "C" fn entry(argc: i32, argv: *const *const c_char) -> i32 {
    unsafe { Entry::run(argc, argv) }
}
