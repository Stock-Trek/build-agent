use crate::{args::Args, command::Command, exit_codes::*, runner::run};
use std::{ffi::CStr, os::raw::c_char};

const EXPECTED_ARG_COUNT: i32 = 5;

/// # Safety
/// This is called from the host binary which must ensure correct args
#[unsafe(no_mangle)]
pub unsafe extern "C" fn entry(argc: i32, argv: *const *const c_char) -> i32 {
    if argc != EXPECTED_ARG_COUNT {
        eprintln!("Usage: program <context_path> <algorithm_name>");
        return SERVER_ENTRY_INCORRECT_ARG_COUNT;
    }
    let args: Vec<String> = (0..argc)
        .map(|i| {
            let arg_ptr = unsafe { *argv.offset(i as isize) };
            unsafe { CStr::from_ptr(arg_ptr).to_string_lossy().into_owned() }
        })
        .collect();
    let command = match args[1].as_str() {
        "Algorithm" => Command::Algorithm,
        "Execute" => Command::Execute,
        "Preferences" => Command::Preferences,
        "Signals" => Command::Signals,
        _ => return SERVER_ENTRY_FAILED_TO_PARSE_COMMAND,
    };
    let strategy_name = args[2].clone();
    let strategy_context_path = args[3].clone();
    let portfolio_path = args[4].clone();
    let args = Args {
        command,
        strategy_name,
        strategy_context_path,
        portfolio_path,
    };
    run(args)
}
