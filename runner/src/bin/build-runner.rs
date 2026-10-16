//! Native host for the `runner` algorithm.
//!
//! `build-agent` spawns this binary, writes the request body to its stdin and
//! forwards its stdout/stderr to the agent logs. The body is a JSON object
//! shaped like:
//!
//! ```json
//! {
//!   "command": "Algorithm",
//!   "strategyName": "AlgorithmStub",
//!   "strategyContext": "<base64 StrategyContextDto>",
//!   "portfolio": "<base64 PortfolioDto>"
//! }
//! ```
//!
//! The `runner` crate implements an algorithm that imports the `env::queue_*`
//! functions. This host provides native implementations of those imports and
//! invokes the algorithm in-process. Every value queued by the algorithm is
//! written to stdout as a single JSON line:
//!
//! ```text
//! {"type":"strategy","payload":"{...}"}
//! {"type":"metadata","payload":{"name":"...","description":"..."}}
//! ```

use runner::{
    args::Args,
    command::Command,
    entry::entry,
    exit_codes::{
        SERVER_ENTRY_FAILED_TO_PARSE_COMMAND, SERVER_ENTRY_FAILED_TO_PARSE_REQUEST,
        SERVER_ENTRY_INVALID_ARGUMENT,
    },
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    ffi::{CString, NulError},
    fs,
    io::{self, Read},
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

const DEFAULT_STRATEGY: &str = "AlgorithmStub";

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RunRequest {
    command: String,
    strategy_name: Option<String>,
    strategy_context: Option<String>,
    portfolio: Option<String>,
    strategy_context_path: Option<String>,
    portfolio_path: Option<String>,
}

impl RunRequest {
    fn strategy_name(&self) -> &str {
        match self.strategy_name.as_deref() {
            Some(name) if !name.is_empty() => name,
            _ => DEFAULT_STRATEGY,
        }
    }
}

struct TempFile {
    path: PathBuf,
}

impl Drop for TempFile {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

fn main() {
    std::process::exit(run());
}

fn run() -> i32 {
    let mut body = String::new();
    if let Err(error) = io::stdin().read_to_string(&mut body) {
        eprintln!("build-runner: failed to read stdin: {error}");
        return 1;
    }

    let request = match parse_request(&body) {
        Ok(request) => request,
        Err(error) => {
            eprintln!("build-runner: invalid request: {error}");
            return SERVER_ENTRY_FAILED_TO_PARSE_REQUEST;
        }
    };

    let command = match parse_command(&request.command) {
        Some(command) => command,
        None => {
            eprintln!(
                "build-runner: invalid request: unknown command '{}'",
                request.command
            );
            return SERVER_ENTRY_FAILED_TO_PARSE_COMMAND;
        }
    };

    let strategy_context = match materialize(
        request.strategy_context.as_deref(),
        request.strategy_context_path.as_deref(),
    ) {
        Ok(file) => file,
        Err(error) => {
            eprintln!("build-runner: failed to load strategy context: {error}");
            return 1;
        }
    };
    let portfolio = match materialize(
        request.portfolio.as_deref(),
        request.portfolio_path.as_deref(),
    ) {
        Ok(file) => file,
        Err(error) => {
            eprintln!("build-runner: failed to load portfolio: {error}");
            return 1;
        }
    };

    let args = Args {
        command,
        strategy_name: request.strategy_name().to_string(),
        strategy_context_path: strategy_context
            .as_ref()
            .map(|file| file.path.to_string_lossy().into_owned())
            .unwrap_or_default(),
        portfolio_path: portfolio
            .as_ref()
            .map(|file| file.path.to_string_lossy().into_owned())
            .unwrap_or_default(),
    };

    match invoke(&args) {
        Ok(exit_code) => exit_code,
        Err(error) => {
            eprintln!("build-runner: invalid argument: {error}");
            SERVER_ENTRY_INVALID_ARGUMENT
        }
    }
}

fn parse_request(body: &str) -> Result<RunRequest, String> {
    if body.trim().is_empty() {
        return Err("body is empty".to_string());
    }
    let value: Value =
        serde_json::from_str(body).map_err(|error| format!("body is not valid JSON: {error}"))?;
    parse_payload(value)
}

fn parse_payload(value: Value) -> Result<RunRequest, String> {
    match value {
        Value::String(text) => {
            let text = text.trim();
            if text.is_empty() {
                return Err("body is empty".to_string());
            }
            match serde_json::from_str::<Value>(text) {
                Ok(inner) if !matches!(inner, Value::String(_)) => parse_payload(inner),
                _ => Ok(RunRequest {
                    command: text.to_string(),
                    ..RunRequest::default()
                }),
            }
        }
        Value::Object(_) => serde_json::from_value(value)
            .map_err(|error| format!("body is not a valid run request: {error}")),
        other => Err(format!(
            "body must be a JSON object or command string, got {}",
            value_kind(&other)
        )),
    }
}

fn value_kind(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "a boolean",
        Value::Number(_) => "a number",
        Value::String(_) => "a string",
        Value::Array(_) => "an array",
        Value::Object(_) => "an object",
    }
}

fn parse_command(command: &str) -> Option<Command> {
    match command {
        "Algorithm" => Some(Command::Algorithm),
        "Execute" => Some(Command::Execute),
        "Metadata" => Some(Command::Metadata),
        "Preferences" => Some(Command::Preferences),
        "Signals" => Some(Command::Signals),
        _ => None,
    }
}

/// Materialise an inline base64 payload or a path into a temporary file. The
/// algorithm reads its protobuf inputs from disk, so both representations are
/// normalised to a file path. When neither is provided, the input is omitted.
fn materialize(inline: Option<&str>, path: Option<&str>) -> io::Result<Option<TempFile>> {
    let bytes = match (inline, path) {
        (_, Some(path)) if !path.is_empty() => fs::read(path)?,
        (Some(inline), _) if !inline.is_empty() => decode_base64(inline)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?,
        _ => return Ok(None),
    };

    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let id = COUNTER.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!("build-runner-{}-{id}.bin", std::process::id()));
    fs::write(&path, bytes)?;
    Ok(Some(TempFile { path }))
}

/// Invoke the algorithm entry point with the resolved arguments.
fn invoke(args: &Args) -> Result<i32, NulError> {
    let program = cstring("build-runner")?;
    let command = cstring(command_name(args.command))?;
    let strategy_name = cstring(&args.strategy_name)?;
    let strategy_context = cstring(&args.strategy_context_path)?;
    let portfolio = cstring(&args.portfolio_path)?;
    let argv = [
        program.as_ptr(),
        command.as_ptr(),
        strategy_name.as_ptr(),
        strategy_context.as_ptr(),
        portfolio.as_ptr(),
    ];

    // Safety: `argv` holds valid, NUL-terminated C strings for the duration of
    // the call and `argv.len()` matches `EXPECTED_ARG_COUNT`.
    Ok(unsafe { entry(argv.len() as i32, argv.as_ptr()) })
}

fn command_name(command: Command) -> &'static str {
    match command {
        Command::Algorithm => "Algorithm",
        Command::Execute => "Execute",
        Command::Metadata => "Metadata",
        Command::Preferences => "Preferences",
        Command::Signals => "Signals",
        // `Command` is `#[non_exhaustive]`.
        _ => "Algorithm",
    }
}

fn cstring(value: &str) -> Result<CString, NulError> {
    CString::new(value)
}

/// Decode standard base64, ignoring padding and whitespace.
fn decode_base64(input: &str) -> Result<Vec<u8>, String> {
    let mut output = Vec::with_capacity(input.len() / 4 * 3);
    let mut accumulator: u32 = 0;
    let mut bits: u32 = 0;
    for byte in input.bytes() {
        if byte == b'=' || byte.is_ascii_whitespace() {
            continue;
        }
        let value = match byte {
            b'A'..=b'Z' => byte - b'A',
            b'a'..=b'z' => byte - b'a' + 26,
            b'0'..=b'9' => byte - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            other => return Err(format!("invalid base64 character: {other}")),
        } as u32;
        accumulator = (accumulator << 6) | value;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            output.push((accumulator >> bits) as u8);
        }
    }
    Ok(output)
}

/// Borrow a `(pointer, length)` pair as a byte slice.
///
/// # Safety
/// When `length` is non-zero, `pointer` must be valid for reads of `length`
/// bytes for the duration of the borrow.
unsafe fn bytes<'a>(pointer: *const u8, length: usize) -> &'a [u8] {
    if pointer.is_null() || length == 0 {
        &[]
    } else {
        // Safety: upheld by the caller.
        unsafe { std::slice::from_raw_parts(pointer, length) }
    }
}

/// Print a queued value as a JSON line.
fn emit(kind: &str, payload: Value) {
    println!("{}", json!({ "type": kind, "payload": payload }));
}

#[unsafe(no_mangle)]
unsafe extern "C" fn queue_metadata(
    name_ptr: *const u8,
    name_len: usize,
    description_ptr: *const u8,
    description_len: usize,
) {
    // Safety: the ABI guarantees the pointer/length pairs are valid.
    let name = unsafe { bytes(name_ptr, name_len) };
    let description = unsafe { bytes(description_ptr, description_len) };
    emit(
        "metadata",
        json!({
            "name": String::from_utf8_lossy(name),
            "description": String::from_utf8_lossy(description),
        }),
    );
}

#[unsafe(no_mangle)]
unsafe extern "C" fn queue_preferences(preferences_ptr: *const u8, preferences_len: usize) {
    // Safety: the ABI guarantees the pointer/length pair is valid.
    let preferences = unsafe { bytes(preferences_ptr, preferences_len) };
    emit("preferences", json!(String::from_utf8_lossy(preferences)));
}

#[unsafe(no_mangle)]
unsafe extern "C" fn queue_signals(signals_ptr: *const u8, signals_len: usize) {
    // Safety: the ABI guarantees the pointer/length pair is valid.
    let signals = unsafe { bytes(signals_ptr, signals_len) };
    emit("signals", json!(String::from_utf8_lossy(signals)));
}

#[unsafe(no_mangle)]
unsafe extern "C" fn queue_strategy(strategy_ptr: *const u8, strategy_len: usize) {
    // Safety: the ABI guarantees the pointer/length pair is valid.
    let strategy = unsafe { bytes(strategy_ptr, strategy_len) };
    emit("strategy", json!(String::from_utf8_lossy(strategy)));
}

#[unsafe(no_mangle)]
unsafe extern "C" fn queue_action(
    action_ptr: *const u8,
    action_len: usize,
    recovery_policy_ptr: *const u8,
    recovery_policy_len: usize,
) {
    // Safety: the ABI guarantees the pointer/length pairs are valid.
    let action = unsafe { bytes(action_ptr, action_len) };
    let recovery_policy = unsafe { bytes(recovery_policy_ptr, recovery_policy_len) };
    emit(
        "action",
        json!({
            "action": String::from_utf8_lossy(action),
            "recoveryPolicy": String::from_utf8_lossy(recovery_policy),
        }),
    );
}
