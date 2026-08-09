//! `ttctl` — drive a running Sterna window from a shell.
//!
//! The client `PLAN.md` promised when it chose JSON-RPC over DDE, and the
//! thing that makes the choice worth anything: `ttctl sendln 'show version'`
//! is a line in a shell script, where the DDE equivalent is a program.
//!
//! It is deliberately thin. Every command is one request, printed as its JSON
//! result, so a script that needs a field pipes it to `jq` and a script that
//! needs a terminal uses `ttctl screen`. The two exceptions are `ls`, which is
//! about the directory rather than about a window, and `macro`, which waits and
//! then exits with the macro's own exit code — because that is what a `.bat`
//! wrapper written for `ttpmacro.exe` expects of the process it started.
//!
//! There is no argument parser here and no dependency for one. The grammar is
//! `ttctl [--to NAME] COMMAND [ARGS]`, every option is a whole word, and the
//! error for anything else is the usage message.

use std::io::Write;
use std::process::ExitCode;

use serde_json::{json, Value};
use tt_ctl::{addr, Client};

const USAGE: &str = "\
usage: ttctl [--to NAME] COMMAND [ARGS]

  ls                       the windows that are listening
  ping                     is it there, and what is it called
  status                   connection, size, log, macro
  send TEXT                type at the host (`-` reads stdin)
  sendln TEXT              ...and a carriage return
  connect LINE             open what a Tera Term command line describes
  disconnect               hang up
  screen [--scrollback N]  the terminal as text (--json for the whole answer)
  macro FILE [PARAM...]    run a .ttl file and wait for it (--no-wait not to)
  stop                     end the running macro
  close                    close the window
  call METHOD [JSON]       anything else, by hand

  --to NAME    which window: a name, a path, or a $STERNA_CTL. With one
               window open it can be left out; with two it cannot.
  --json       print the raw JSON result even where a command has a
               friendlier form.
";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match run(&args) {
        Ok(code) => code,
        Err(e) => {
            eprintln!("ttctl: {e}");
            ExitCode::from(1)
        }
    }
}

fn run(args: &[String]) -> Result<ExitCode, String> {
    let mut to: Option<String> = None;
    let mut json_out = false;
    let mut scrollback = 0usize;
    let mut wait = true;
    let mut rest: Vec<String> = Vec::new();

    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--help" | "-h" => {
                print!("{USAGE}");
                return Ok(ExitCode::SUCCESS);
            }
            "--to" => to = Some(it.next().ok_or("--to wants a name")?.clone()),
            "--json" => json_out = true,
            "--no-wait" => wait = false,
            "--scrollback" => {
                scrollback = it
                    .next()
                    .ok_or("--scrollback wants a number")?
                    .parse()
                    .map_err(|_| "--scrollback wants a number")?
            }
            // Everything after the command word is the command's, so a macro
            // parameter that looks like an option is not eaten. That is the
            // rule `ttpmacro`'s own command line has, arrived at from the
            // other direction.
            _ => {
                rest.push(a.clone());
                rest.extend(it.cloned());
                break;
            }
        }
    }

    let (cmd, cmd_args) = match rest.split_first() {
        Some((c, a)) => (c.as_str(), a),
        None => {
            eprint!("{USAGE}");
            return Ok(ExitCode::from(2));
        }
    };

    // The one command that is about the directory rather than about a window,
    // so it opens no connection.
    if cmd == "ls" {
        return list(json_out).map(|_| ExitCode::SUCCESS);
    }

    let mut client = Client::open(to.as_deref()).map_err(|e| e.to_string())?;

    let (method, params) = match cmd {
        "ping" | "status" | "disconnect" | "close" => (cmd.to_string(), json!({})),
        "stop" => ("macro.stop".to_string(), json!({})),
        "send" | "sendln" => (cmd.to_string(), json!({ "text": one_arg(cmd, cmd_args)? })),
        "connect" => (
            "connect".to_string(),
            json!({ "line": one_arg(cmd, cmd_args)? }),
        ),
        "screen" => ("screen".to_string(), json!({ "scrollback": scrollback })),
        "macro" => {
            let (path, rest) = cmd_args.split_first().ok_or("macro wants a file")?;
            // Resolved here rather than in the window: a path typed in a shell
            // means what it says in *that* shell, and the window's working
            // directory is wherever it was started.
            let path = std::fs::canonicalize(path).map_err(|e| format!("{path}: {e}"))?;
            (
                "macro.run".to_string(),
                json!({
                    "path": path.to_string_lossy(),
                    "params": rest,
                    "wait": wait,
                }),
            )
        }
        "call" => {
            let (m, rest) = cmd_args.split_first().ok_or("call wants a method")?;
            let p: Value = match rest.first() {
                Some(s) => serde_json::from_str(s).map_err(|e| format!("params: {e}"))?,
                None => json!({}),
            };
            (m.clone(), p)
        }
        other => return Err(format!("no command {other:?}; try --help")),
    };

    let result = client.call(&method, params).map_err(|e| e.to_string())?;

    if cmd == "screen" && !json_out {
        let mut out = std::io::stdout().lock();
        for line in result["lines"].as_array().into_iter().flatten() {
            let _ = writeln!(out, "{}", line.as_str().unwrap_or_default());
        }
        return Ok(ExitCode::SUCCESS);
    }

    println!("{result}");

    // A `ttctl macro` that waited exits as the macro did, so that a script can
    // test it the way it would test any other command.
    if cmd == "macro" && wait {
        let exit = result["exit"].as_i64().unwrap_or(0);
        return Ok(ExitCode::from((exit & 0xff) as u8));
    }
    Ok(ExitCode::SUCCESS)
}

fn one_arg(cmd: &str, args: &[String]) -> Result<String, String> {
    match args {
        // `-` for stdin, so that a long string, or one with quoting a shell
        // would eat, can be piped in.
        [one] if one == "-" => {
            std::io::read_to_string(std::io::stdin()).map_err(|e| format!("stdin: {e}"))
        }
        [one] => Ok(one.clone()),
        _ => Err(format!("{cmd} wants exactly one argument (or `-`)")),
    }
}

/// Every window that answers, with what it calls itself.
///
/// This is what a person types when `--to` has complained that there are two.
/// A socket that will not answer is still listed rather than hidden — the
/// point is to show what is there, and a window that is wedged is exactly what
/// somebody looking at this list wants to see.
fn list(json_out: bool) -> Result<(), String> {
    let live = addr::live().map_err(|e| e.to_string())?;
    let mut rows = Vec::new();
    for path in &live {
        let name = path
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        let info = Client::connect(path)
            .and_then(|mut c| c.call("ping", json!({})))
            .ok();
        rows.push(json!({
            "name": name,
            "path": path.to_string_lossy(),
            "pid": info.as_ref().and_then(|i| i.get("pid").cloned()),
            "title": info.as_ref().and_then(|i| i.get("title").cloned()),
        }));
    }
    if json_out {
        println!("{}", Value::Array(rows));
        return Ok(());
    }
    for r in &rows {
        let title = r["title"].as_str().unwrap_or("");
        match r["pid"].as_u64() {
            Some(pid) => println!(
                "{:<12} pid {:<8} {}",
                r["name"].as_str().unwrap(),
                pid,
                title
            ),
            None => println!("{:<12} (no answer)", r["name"].as_str().unwrap()),
        }
    }
    Ok(())
}
