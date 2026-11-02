//! Which evaluation the engine searches with: material, or an NNUE net.
//!
//! The choice is an enum, not a `Box<dyn Evaluator>`. The search is generic
//! over its evaluator, so matching on the enum once per `go` and calling
//! `Searcher::new(MaterialEval)` or `Searcher::new(nnue)` makes the compiler
//! build one specialised copy of the search per evaluator
//! (monomorphisation): the evaluation call inside the search, made millions
//! of times per second, is then a direct call the compiler can inline, not a
//! lookup through a table of function pointers.

use std::sync::Arc;

use nnue::{Network, Nnue};

#[derive(Clone, Debug)]
pub enum Evaluation {
    /// Material count: the evaluation when no net is set.
    Material,
    /// The net loaded from `file`. Cloning shares the net (an `Arc`).
    Nnue { evaluator: Nnue, file: String },
    /// `EvalFile` was set but could not be loaded. The engine refuses to
    /// search rather than quietly playing on material: a match measuring the
    /// net must not measure something else without anyone noticing.
    Broken { reason: String },
}

impl Evaluation {
    /// The evaluation an `EvalFile` value asks for: empty, or the advertised
    /// default `<empty>` (which a GUI may send back), means material.
    pub fn from_eval_file(path: &str) -> Self {
        if path.is_empty() || path == "<empty>" {
            return Evaluation::Material;
        }
        match Network::load(path) {
            Ok(network) => Evaluation::Nnue {
                evaluator: Nnue::new(Arc::new(network)),
                file: path.to_string(),
            },
            Err(error) => Evaluation::Broken {
                reason: error.to_string(),
            },
        }
    }

    /// What `go` announces before searching, after `info string`.
    pub fn describe(&self) -> String {
        match self {
            Evaluation::Material => "eval material".to_string(),
            Evaluation::Nnue { file, .. } => format!("eval nnue {file}"),
            Evaluation::Broken { reason } => format!("EvalFile error: {reason}"),
        }
    }
}
