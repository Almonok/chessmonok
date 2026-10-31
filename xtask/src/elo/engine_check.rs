//! A short UCI conversation with each engine before the match: the guard
//! against measuring a broken engine.
//!
//! fastchess would happily run a match where one engine failed to load its
//! net and fell back to something weaker, or silently ignored an option it
//! does not know: the result would look like a real Elo difference. So
//! before any game, each engine must get through:
//!
//! ```text
//! > uci                          < id ... / option name X type ... / uciok
//! > setoption name X value V     (each option, which the engine must list)
//! > isready                      < readyok   (no `info string ... error` before it)
//! > quit
//! ```

use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, anyhow, bail};

use crate::elo::engine_options::EngineOption;

/// How long the engine may take to answer.
#[derive(Debug)]
struct Timeouts {
    for_uciok: Duration,
    /// Longer: `isready` is when an engine finishes loading a net.
    for_readyok: Duration,
}

const DEFAULT_TIMEOUTS: Timeouts = Timeouts {
    for_uciok: Duration::from_secs(5),
    for_readyok: Duration::from_secs(30),
};

/// Check that `engine_binary` speaks UCI, supports and accepts every option,
/// and reports no error. Errors start with `side_name` (`new:` or
/// `base:`) and end with what the engine printed.
pub fn check_engine(engine_binary: &Path, options: &[EngineOption], side_name: &str) -> Result<()> {
    check_engine_with_timeouts(engine_binary, options, side_name, &DEFAULT_TIMEOUTS)
}

fn check_engine_with_timeouts(
    engine_binary: &Path,
    options: &[EngineOption],
    side_name: &str,
    timeouts: &Timeouts,
) -> Result<()> {
    let mut conversation = EngineConversation::start(engine_binary)
        .with_context(|| format!("{side_name}: cannot start {}", engine_binary.display()))?;
    let outcome = run_checks(&mut conversation, options, timeouts);
    outcome.map_err(|error| {
        anyhow!(
            "{side_name}: {error:#}\nengine output:\n{}",
            conversation.transcript.join("\n")
        )
    })
}

fn run_checks(
    conversation: &mut EngineConversation,
    options: &[EngineOption],
    timeouts: &Timeouts,
) -> Result<()> {
    conversation.send("uci")?;
    let introduction = conversation.read_until("uciok", timeouts.for_uciok)?;
    for option in options {
        if !declares_option(&introduction, &option.name) {
            bail!("does not support UCI option `{}`", option.name);
        }
        conversation.send(&format!(
            "setoption name {} value {}",
            option.name, option.value
        ))?;
    }
    conversation.send("isready")?;
    conversation.read_until("readyok", timeouts.for_readyok)?;
    let _ = conversation.send("quit");
    Ok(())
}

/// Whether the engine listed `name` among its `option name ...` lines. UCI
/// option names are case-insensitive.
fn declares_option(introduction: &[String], name: &str) -> bool {
    introduction
        .iter()
        .filter_map(|line| declared_option_name(line))
        .any(|declared| declared.eq_ignore_ascii_case(name))
}

/// The name in `option name <Name> type ...`, which may contain spaces.
fn declared_option_name(line: &str) -> Option<&str> {
    let rest = line.strip_prefix("option name ")?;
    Some(rest.split_once(" type ").map_or(rest, |(name, _)| name))
}

/// A running engine and everything it has said so far. Dropping it kills
/// the engine, so no exit path leaves one running.
struct EngineConversation {
    process: Child,
    input: ChildStdin,
    output_lines: Receiver<std::io::Result<String>>,
    transcript: Vec<String>,
}

impl EngineConversation {
    fn start(engine_binary: &Path) -> Result<Self> {
        let mut process = Command::new(engine_binary)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()?;
        let input = process.stdin.take().expect("stdin is piped");
        let output = process.stdout.take().expect("stdout is piped");
        Ok(EngineConversation {
            process,
            input,
            output_lines: read_lines_in_background(output),
            transcript: Vec::new(),
        })
    }

    fn send(&mut self, command: &str) -> Result<()> {
        writeln!(self.input, "{command}")
            .with_context(|| format!("engine stopped reading at `{command}`"))
    }

    /// Read lines until one equals `expected`; return the lines before it.
    /// Fails on timeout, on the engine exiting, or on an `info string`
    /// reporting an error.
    fn read_until(&mut self, expected: &str, timeout: Duration) -> Result<Vec<String>> {
        let deadline = Instant::now() + timeout;
        let mut lines_before = Vec::new();
        loop {
            let time_left = deadline.saturating_duration_since(Instant::now());
            let line = match self.output_lines.recv_timeout(time_left) {
                Ok(line) => line.context("cannot read engine output")?,
                Err(RecvTimeoutError::Timeout) => bail!("no `{expected}` within {timeout:?}"),
                Err(RecvTimeoutError::Disconnected) => {
                    bail!("engine exited before `{expected}`")
                }
            };
            self.transcript.push(line.clone());
            let line = line.trim();
            if reports_an_error(line) {
                bail!("engine reported `{line}`");
            }
            if line == expected {
                return Ok(lines_before);
            }
            lines_before.push(line.to_string());
        }
    }
}

impl Drop for EngineConversation {
    fn drop(&mut self) {
        // Killing the engine also closes its output, which ends the reading
        // thread.
        let _ = self.process.kill();
        let _ = self.process.wait();
    }
}

fn reports_an_error(line: &str) -> bool {
    line.starts_with("info string") && line.to_ascii_lowercase().contains("error")
}

/// Forward each line the engine prints to a channel, from another thread.
///
/// Reading a pipe blocks until a line arrives, and std has no read with a
/// timeout on pipes, so a hung engine would hang the harness. A thread does
/// the blocking reads instead, and the caller waits on the channel with
/// `recv_timeout`, which does take a deadline.
fn read_lines_in_background(output: ChildStdout) -> Receiver<std::io::Result<String>> {
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        for line in BufReader::new(output).lines() {
            if sender.send(line).is_err() {
                break;
            }
        }
    });
    receiver
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::fs;
    use std::os::unix::fs::PermissionsExt;
    use std::path::PathBuf;
    use std::sync::{Mutex, MutexGuard, PoisonError};

    /// Held by every test that writes and runs a fake engine. Writing an
    /// executable while another test thread forks can make the exec fail
    /// with "text file busy" (the child briefly inherits the write handle),
    /// so these tests take turns.
    static FAKE_ENGINES: Mutex<()> = Mutex::new(());

    fn take_turn() -> MutexGuard<'static, ()> {
        FAKE_ENGINES.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Write a fake engine: a `/bin/sh` loop answering UCI commands with the
    /// given `case` branches.
    fn fake_engine(name: &str, answers: &str) -> PathBuf {
        let path =
            std::env::temp_dir().join(format!("xtask-fake-engine-{name}-{}", std::process::id()));
        let script = format!(
            "#!/bin/sh\nwhile read -r line; do\n  case \"$line\" in\n{answers}\n    quit) exit 0;;\n  esac\ndone\n"
        );
        fs::write(&path, script).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
        path
    }

    const INTRODUCES_ITSELF_WITH_HASH: &str = "    uci) echo 'id name fake'; echo 'option name Hash type spin default 16 min 1 max 1024'; echo uciok;;";
    const ANSWERS_READY: &str = "    isready) echo readyok;;";

    fn short_timeouts() -> Timeouts {
        Timeouts {
            for_uciok: Duration::from_millis(500),
            for_readyok: Duration::from_millis(500),
        }
    }

    fn option(name: &str, value: &str) -> EngineOption {
        EngineOption {
            name: name.into(),
            value: value.into(),
        }
    }

    fn check(engine: &Path, options: &[EngineOption], name: &str) -> String {
        match check_engine_with_timeouts(engine, options, name, &short_timeouts()) {
            Ok(()) => String::new(),
            Err(error) => format!("{error:#}"),
        }
    }

    #[test]
    fn accepts_a_well_behaved_engine() {
        let _turn = take_turn();
        let engine = fake_engine(
            "good",
            &format!("{INTRODUCES_ITSELF_WITH_HASH}\n{ANSWERS_READY}"),
        );
        assert_eq!(check(&engine, &[option("Hash", "32")], "new"), "");
    }

    #[test]
    fn rejects_an_unsupported_option() {
        let _turn = take_turn();
        let engine = fake_engine(
            "no-evalfile",
            &format!("{INTRODUCES_ITSELF_WITH_HASH}\n{ANSWERS_READY}"),
        );
        let error = check(&engine, &[option("EvalFile", "x.nnue")], "base");
        assert!(
            error.starts_with("base:") && error.contains("EvalFile"),
            "{error}"
        );
    }

    #[test]
    fn rejects_an_engine_reporting_an_error() {
        let _turn = take_turn();
        let engine = fake_engine(
            "eval-error",
            &format!(
                "{INTRODUCES_ITSELF_WITH_HASH}\n    isready) echo 'info string EvalFile error: missing'; echo readyok;;"
            ),
        );
        let error = check(&engine, &[], "new");
        assert!(
            error.starts_with("new:") && error.contains("EvalFile error"),
            "{error}"
        );
    }

    #[test]
    fn gives_up_on_an_engine_that_never_answers() {
        let _turn = take_turn();
        let engine = fake_engine("mute", "    never) ;;");
        assert!(check(&engine, &[], "new").contains("uciok"));
    }

    /// The real engine, built from `main` by the harness's own build code.
    #[test]
    #[ignore = "builds the engine; run with --ignored"]
    fn checks_the_real_engine() {
        use crate::elo::build::build_engine;
        use crate::elo::versions::{CodeVersion, commit_hash_of, repository_root};
        let _turn = crate::elo::build::tests::REAL_BUILDS
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let main = CodeVersion::Commit {
            hash: commit_hash_of("main").unwrap(),
        };
        let binary = build_engine(&main, &repository_root().unwrap()).unwrap();
        check_engine(&binary, &[option("RandomMover", "false")], "base").unwrap();
        assert!(check_engine(&binary, &[option("NoSuchOption", "1")], "base").is_err());
    }
}
