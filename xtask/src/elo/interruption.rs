//! Noticing Ctrl-C without dying from it.
//!
//! Ctrl-C reaches every process in the terminal's foreground group: cargo
//! while building, fastchess during the match, and each stops on its own.
//! By default it would also kill the harness on the spot, before worktrees
//! are cleaned up and before the games played so far are reported. So the
//! harness replaces that default with a handler that only raises a flag.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use anyhow::{Context, Result};

/// Whether Ctrl-C has been pressed since [`Interruption::watch`].
#[derive(Debug)]
pub struct Interruption {
    happened: Arc<AtomicBool>,
}

impl Interruption {
    /// Start watching for Ctrl-C. Call once per process.
    pub fn watch() -> Result<Self> {
        // The handler runs on another thread, so the flag must be shared
        // (`Arc`) and safe to set from there (`AtomicBool`).
        let happened = Arc::new(AtomicBool::new(false));
        let flag_for_handler = Arc::clone(&happened);
        ctrlc::set_handler(move || flag_for_handler.store(true, Ordering::SeqCst))
            .context("cannot install the Ctrl-C handler")?;
        Ok(Interruption { happened })
    }

    pub fn happened(&self) -> bool {
        self.happened.load(Ordering::SeqCst)
    }
}
