//! `ttpmacro` — the compatibility entry point, so existing shortcuts keep
//! working.
//!
//! `PLAN.md` has asked for this since Stage 0: "keep a `ttpmacro script.ttl`
//! CLI entry point so existing shortcuts and `.bat` wrappers keep working."
//! It is a *client*, and that is the whole difference between this port and
//! upstream. `ttpmacro.exe` is the interpreter — it runs the script in its own
//! process and reaches the terminal over DDE for every `send` and every
//! `wait`. Here the interpreter is inside the window, so this program parses
//! the same command line, finds the window, asks it to run the file, and waits.
//!
//! What survives:
//!
//! - the command line, character for character, because it is parsed by
//!   [`tt_ttl::cmdline`] — `ParseParam` (`ttmdlg.cpp:82`) and the four `.bat`
//!   lines in `macroparam.bat` that are its specification;
//! - `/D=`, which upstream uses to say *which window*, and which here names
//!   the socket — the same job through a different mechanism;
//! - the exit status, so a `.bat` file's `if errorlevel` still means what it
//!   meant.
//!
//! What does not, and is stated rather than hidden:
//!
//! - **`/V` and `/I` do nothing.** They describe `ttpmacro`'s own control
//!   window — hidden, or minimised — and there is no second process to have
//!   one. The window's Stop button is where a running macro is now visible.
//! - **`/S` does nothing.** It parks the macro until the terminal is ready,
//!   which is a race between two processes starting; here the window is
//!   already up by definition, since this program had to connect to it.
//! - **`params[0]` is not the command line as typed.** Upstream's is
//!   `GetCommandLineW()`; what the window can see is what was sent, which is
//!   the file and its parameters joined by spaces. The switches this program
//!   ate are not in it — which is the one thing `params[0]` was for.

use std::process::ExitCode;

use serde_json::json;
use tt_ctl::Client;
use tt_ttl::cmdline::CmdLine;

fn main() -> ExitCode {
    match run() {
        Ok(code) => code,
        Err(e) => {
            eprintln!("ttpmacro: {e}");
            ExitCode::from(1)
        }
    }
}

fn run() -> Result<ExitCode, String> {
    // `from_args` rather than `parse`: the shell has already tokenised and
    // unquoted, and running `GetParam` over a rejoined `argv` would
    // quote-process everything twice.
    let cmd = CmdLine::from_args(std::env::args_os().skip(1).map(|a| {
        use std::os::unix::ffi::OsStrExt;
        a.as_bytes().to_vec()
    }));

    if cmd.needs_prompt() {
        // Upstream puts a file-open dialog up. This has no window to put one
        // in — the window it would ask about belongs to another process — so
        // it says what it wanted instead.
        return Err("no macro file named".into());
    }

    let topic = String::from_utf8_lossy(&cmd.topic).into_owned();
    let mut client = Client::open(if topic.is_empty() {
        None
    } else {
        Some(topic.as_str())
    })
    .map_err(|e| e.to_string())?;

    // Fitted here rather than in the window, because `FitTTLFileName`
    // (`ttmmain.cpp:253`) is part of *this* command line's meaning: a name
    // with no dot in its last component gets `.TTL`, and it is the launcher
    // that decides so. Absolute, because the window's working directory is
    // wherever it was started and not where this was typed.
    let name = String::from_utf8_lossy(&cmd.fitted_file_name()).into_owned();
    let path = std::fs::canonicalize(&name).map_err(|e| format!("{name}: {e}"))?;
    let params: Vec<String> = cmd
        .args
        .iter()
        .map(|a| String::from_utf8_lossy(a).into_owned())
        .collect();

    let result = client
        .call(
            "macro.run",
            json!({
                "path": path.to_string_lossy(),
                "params": params,
                // A `.bat` wrapper waits on the process it started, so this
                // one has to last as long as the macro does. That is the whole
                // reason `macro.run` can block.
                "wait": true,
            }),
        )
        .map_err(|e| e.to_string())?;

    let exit = result["exit"].as_i64().unwrap_or(0);
    Ok(ExitCode::from((exit & 0xff) as u8))
}
