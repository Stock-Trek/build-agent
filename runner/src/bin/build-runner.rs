//! Native host for the `runner` algorithm.
//!
//! `build-agent` spawns this binary, writes the MicroVM run-hook request body to
//! its stdin and forwards its stdout/stderr to the agent logs. The run-hook body
//! is JSON shaped like `{ "microvmId": "...", "runHookPayload": "..." }`, where
//! `runHookPayload` carries the algorithm invocation.
//!
//! The `runner` crate implements an algorithm as a WebAssembly module that
//! imports the `env::queue_*` functions. This host provides native
//! implementations of those imports and invokes the algorithm in-process, so the
//! algorithm can be executed directly inside the MicroVM. Every value queued by
//! the algorithm is written to stdout as a single JSON line:
//!
//! ```text
//! {"type":"strategy","payload":"{...}"}
//! {"type":"metadata","payload":{"name":"...","description":"..."}}
//! ```

use runner::{
    args::Args, command::Command, entry::entry, exit_codes::SERVER_ENTRY_FAILED_TO_PARSE_COMMAND,
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    ffi::CString,
    fs,
    io::{self, Read},
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

/// The default algorithm shipped with the image.
const DEFAULT_STRATEGY: &str = "AlgorithmStub";

/// Request extracted from the run-hook body. Every field is optional so a
/// payload containing only `command` (or nothing at all) still runs.
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct RunRequest {
    command: Option<String>,
    strategy_name: Option<String>,
    /// Base64 encoded `StrategyContextDto` protobuf.
    strategy_context: Option<String>,
    /// Base64 encoded `PortfolioDto` protobuf.
    portfolio: Option<String>,
    /// Path to a `StrategyContextDto` protobuf, used instead of `strategyContext`.
    strategy_context_path: Option<String>,
    /// Path to a `PortfolioDto` protobuf, used instead of `portfolio`.
    portfolio_path: Option<String>,
}

impl RunRequest {
    fn command(&self) -> &str {
        self.command.as_deref().unwrap_or("Algorithm")
    }

    fn strategy_name(&self) -> &str {
        match self.strategy_name.as_deref() {
            Some(name) if !name.is_empty() => name,
            _ => DEFAULT_STRATEGY,
        }
    }
}

/// A temporary file that is removed when it goes out of scope.
struct TempFile {
    path: PathBuf,
}

impl Drop for TempFile {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

fn main() {
    let mut body = String::new();
    if let Err(error) = io::stdin().read_to_string(&mut body) {
        eprintln!("build-runner: failed to read stdin: {error}");
        std::process::exit(1);
    }

    let request = parse_request(&body);

    let command = match parse_command(request.command()) {
        Some(command) => command,
        None => {
            eprintln!("build-runner: unknown command '{}'", request.command());
            std::process::exit(SERVER_ENTRY_FAILED_TO_PARSE_COMMAND);
        }
    };

    let strategy_context = match materialize(
        request.strategy_context.as_deref(),
        request.strategy_context_path.as_deref(),
    ) {
        Ok(file) => file,
        Err(error) => {
            eprintln!("build-runner: failed to load strategy context: {error}");
            std::process::exit(1);
        }
    };
    let portfolio = match materialize(
        request.portfolio.as_deref(),
        request.portfolio_path.as_deref(),
    ) {
        Ok(file) => file,
        Err(error) => {
            eprintln!("build-runner: failed to load portfolio: {error}");
            std::process::exit(1);
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

    let exit_code = invoke(&args);
    std::process::exit(exit_code);
}

/// Parse the run-hook body into a [`RunRequest`].
///
/// `runHookPayload` may be a JSON object, a JSON string containing an object, or
/// a bare command name. If the body itself is a request (no envelope), it is used
/// directly. Unparseable bodies fall back to the default `Algorithm` invocation.
fn parse_request(body: &str) -> RunRequest {
    let value = match serde_json::from_str::<Value>(body) {
        Ok(Value::Object(mut map)) => match map.remove("runHookPayload") {
            Some(payload) => payload,
            None => Value::Object(map),
        },
        Ok(value) => value,
        Err(_) => return RunRequest::default(),
    };

    parse_payload(value)
}

fn parse_payload(value: Value) -> RunRequest {
    match value {
        // The payload may be a JSON document encoded as a string.
        Value::String(text) => match serde_json::from_str::<Value>(&text) {
            Ok(inner) if !matches!(inner, Value::String(_)) => parse_payload(inner),
            _ => RunRequest {
                command: Some(text),
                ..RunRequest::default()
            },
        },
        value => serde_json::from_value(value).unwrap_or_default(),
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
fn invoke(args: &Args) -> i32 {
    let program = cstring("build-runner");
    let command = cstring(command_name(args.command));
    let strategy_name = cstring(&args.strategy_name);
    let strategy_context = cstring(&args.strategy_context_path);
    let portfolio = cstring(&args.portfolio_path);
    let argv = [
        program.as_ptr(),
        command.as_ptr(),
        strategy_name.as_ptr(),
        strategy_context.as_ptr(),
        portfolio.as_ptr(),
    ];

    // Safety: `argv` holds valid, NUL-terminated C strings for the duration of
    // the call and `argv.len()` matches `EXPECTED_ARG_COUNT`.
    unsafe { entry(argv.len() as i32, argv.as_ptr()) }
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

fn cstring(value: &str) -> CString {
    CString::new(value).expect("arguments must not contain interior NUL bytes")
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
    // Safety: the wasm ABI guarantees the pointer/length pairs are valid.
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
    // Safety: the wasm ABI guarantees the pointer/length pair is valid.
    let preferences = unsafe { bytes(preferences_ptr, preferences_len) };
    emit("preferences", json!(String::from_utf8_lossy(preferences)));
}

#[unsafe(no_mangle)]
unsafe extern "C" fn queue_signals(signals_ptr: *const u8, signals_len: usize) {
    // Safety: the wasm ABI guarantees the pointer/length pair is valid.
    let signals = unsafe { bytes(signals_ptr, signals_len) };
    emit("signals", json!(String::from_utf8_lossy(signals)));
}

#[unsafe(no_mangle)]
unsafe extern "C" fn queue_strategy(strategy_ptr: *const u8, strategy_len: usize) {
    // Safety: the wasm ABI guarantees the pointer/length pair is valid.
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
    // Safety: the wasm ABI guarantees the pointer/length pairs are valid.
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_base64() {
        assert_eq!(decode_base64("").unwrap(), Vec::<u8>::new());
        assert_eq!(decode_base64("aGVsbG8=").unwrap(), b"hello");
        assert_eq!(decode_base64("aGVsbG8").unwrap(), b"hello");
        assert!(decode_base64("!!!").is_err());
    }

    #[test]
    fn parses_commands() {
        assert_eq!(parse_command("Algorithm"), Some(Command::Algorithm));
        assert_eq!(parse_command("Execute"), Some(Command::Execute));
        assert_eq!(parse_command("Metadata"), Some(Command::Metadata));
        assert_eq!(parse_command("nope"), None);
    }

    #[test]
    fn parses_envelope_payload() {
        let request =
            parse_request(r#"{"microvmId":"m-1","runHookPayload":"{\"command\":\"Signals\"}"}"#);
        assert_eq!(request.command(), "Signals");
    }

    #[test]
    fn parses_direct_request() {
        let request = parse_request(r#"{"command":"Preferences"}"#);
        assert_eq!(request.command(), "Preferences");
    }

    #[test]
    fn parses_bare_command() {
        let request = parse_request(r#""Execute""#);
        assert_eq!(request.command(), "Execute");
    }

    #[test]
    fn defaults_to_algorithm() {
        let request = parse_request("not json");
        assert_eq!(request.command(), "Algorithm");
        assert_eq!(request.strategy_name(), "AlgorithmStub");
    }
}
