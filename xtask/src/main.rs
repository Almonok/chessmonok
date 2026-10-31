//! Repo tooling for chessmonok, run as `cargo xtask <command>`.
//!
//! # The xtask pattern
//!
//! Cargo has no built-in task runner (no `npm run`). The convention the Rust
//! ecosystem settled on is a plain binary crate called `xtask` inside the
//! workspace, plus a cargo alias in `.cargo/config.toml`:
//!
//! ```toml
//! [alias]
//! xtask = "run --quiet --package xtask --"
//! ```
//!
//! so `cargo xtask elo --help` compiles and runs this binary. The tooling is
//! then ordinary Rust: type-checked, tested with `cargo test`, portable, and
//! needing nothing installed beyond the toolchain the repo already pins.
//!
//! # Commands
//!
//! - `elo`: is one version of the engine stronger than another? See [`elo`].

mod elo;

use std::process::ExitCode;

use clap::{Parser, Subcommand};

/// `#[derive(Parser)]` makes clap generate the whole argument parser, the
/// `--help` text and the error messages from this type at compile time: the
/// struct *is* the command-line interface.
#[derive(Debug, Parser)]
#[command(name = "cargo xtask", about = "Repo tooling for chessmonok")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Play two versions of the engine against each other until an SPRT
    /// decides whether the new one is stronger.
    Elo(elo::EloArguments),
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let result = match cli.command {
        Command::Elo(arguments) => elo::run(&arguments),
    };
    // `anyhow::Error`'s alternate Display (`{:#}`) prints the whole chain of
    // context ("building base: cargo failed: ..."), not just the outermost
    // message.
    result.unwrap_or_else(|error| {
        eprintln!("error: {error:#}");
        ExitCode::from(elo::EXIT_CODE_FOR_ERRORS)
    })
}
