//! `elo/history.jsonl`: the evidence trail.
//!
//! Every Mac run whose result means something appends one line here, a JSON
//! object (the "JSON Lines" format: one object per line, so appending never
//! rewrites the file and git diffs stay one line per run). The line is
//! committed on the feature branch together with the change it justifies,
//! and `.gitattributes` marks the file `merge=union` so appends from two
//! branches merge without conflicts.
//!
//! The file is public once the repository is, so it holds no hostname, no
//! user name and no directory paths.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::Path;
use std::process::Command;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::elo::engine_options::EngineOption;
use crate::elo::verdict::Verdict;
use crate::elo::versions::CodeVersion;

/// One recorded run.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct HistoryEntry {
    /// UTC, e.g. `2026-10-31T13:45:07Z`.
    pub date: String,
    pub new_commit: String,
    pub base_commit: String,
    pub mode: String,
    pub profile: String,
    pub time_control: String,
    pub lower_elo: f64,
    pub upper_elo: f64,
    pub verdict: String,
    pub elo: Option<f64>,
    pub elo_error_margin: Option<f64>,
    pub log_likelihood_ratio: Option<f64>,
    pub games: u32,
    pub wins: u32,
    pub losses: u32,
    pub draws: u32,
    pub pair_score_counts: [u32; 5],
    /// `new:NAME=VALUE`, with a file (a net) shown as `<file name>#<hash>`.
    pub engine_options: Vec<String>,
    pub opening_book: Option<String>,
    pub seed: u64,
    pub machine: String,
}

/// Whether a run goes into the history: only finished, valid runs between
/// exact commits. Uncommitted changes have no commit to point at.
pub fn should_record(
    verdict: &Verdict,
    new: &CodeVersion,
    base: &CodeVersion,
    recording_disabled: bool,
) -> bool {
    let finished_and_valid = !matches!(verdict, Verdict::Aborted | Verdict::Invalid);
    !recording_disabled && finished_and_valid && new.is_exact() && base.is_exact()
}

/// Append `entry` to the history file as one JSON line.
pub fn append_entry(history_file: &Path, entry: &HistoryEntry) -> Result<()> {
    if let Some(directory) = history_file.parent() {
        fs::create_dir_all(directory)?;
    }
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(history_file)
        .with_context(|| format!("cannot open {}", history_file.display()))?;
    let line = serde_json::to_string(entry)?;
    writeln!(file, "{line}").with_context(|| format!("cannot write {}", history_file.display()))
}

/// How an engine option is recorded. A value naming an existing file (a net)
/// becomes its file name plus a fingerprint of its contents: two runs with
/// the same net are recognisable, while the path (which may name a home
/// directory) is not kept.
pub fn describe_engine_option(option: &EngineOption) -> String {
    let path = Path::new(&option.value);
    match (path.file_name(), fs::read(path)) {
        (Some(file_name), Ok(contents)) if path.is_file() => format!(
            "{}={}#{:016x}",
            option.name,
            file_name.to_string_lossy(),
            content_fingerprint(&contents)
        ),
        _ => format!("{}={}", option.name, option.value),
    }
}

/// A 64-bit fingerprint of `bytes` (the FNV-1a hash). Not cryptographic, but
/// stable across Rust versions and platforms, unlike `std`'s
/// `DefaultHasher`, which is exactly what a value stored in a file needs.
pub fn content_fingerprint(bytes: &[u8]) -> u64 {
    const OFFSET_BASIS: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIME: u64 = 0x0100_0000_01b3;
    bytes.iter().fold(OFFSET_BASIS, |hash, &byte| {
        (hash ^ u64::from(byte)).wrapping_mul(PRIME)
    })
}

/// The processor and its core count, e.g. `Apple M4, 10 cores`. Never the
/// hostname: this ends up in a public file.
pub fn machine_description() -> String {
    let processor = processor_name().unwrap_or_else(|| std::env::consts::ARCH.to_string());
    let cores = std::thread::available_parallelism().map_or(0, std::num::NonZero::get);
    format!("{processor}, {cores} cores")
}

fn processor_name() -> Option<String> {
    if cfg!(target_os = "macos") {
        let output = Command::new("sysctl")
            .args(["-n", "machdep.cpu.brand_string"])
            .output()
            .ok()?;
        let name = String::from_utf8_lossy(&output.stdout).trim().to_string();
        (!name.is_empty()).then_some(name)
    } else {
        let processor_info = fs::read_to_string("/proc/cpuinfo").ok()?;
        processor_info
            .lines()
            .find_map(|line| line.strip_prefix("model name")?.split_once(':'))
            .map(|(_, name)| name.trim().to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn commit() -> CodeVersion {
        CodeVersion::Commit { hash: "abc".into() }
    }

    fn working_tree(has_uncommitted_changes: bool) -> CodeVersion {
        CodeVersion::WorkingTree {
            head_commit: "def".into(),
            has_uncommitted_changes,
        }
    }

    fn temporary_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("xtask-{name}-{}", std::process::id()))
    }

    fn sample_entry() -> HistoryEntry {
        HistoryEntry {
            date: "2026-10-31T13:45:07Z".into(),
            new_commit: "abc".into(),
            base_commit: "def".into(),
            mode: "gain".into(),
            profile: "quick".into(),
            time_control: "5+0.05".into(),
            lower_elo: 0.0,
            upper_elo: 10.0,
            verdict: "PASS".into(),
            elo: Some(25.0),
            elo_error_margin: Some(12.0),
            log_likelihood_ratio: Some(2.95),
            games: 600,
            wins: 200,
            losses: 160,
            draws: 240,
            pair_score_counts: [10, 60, 120, 80, 30],
            engine_options: vec![],
            opening_book: Some("8moves_v3.pgn".into()),
            seed: 42,
            machine: "Apple M4, 10 cores".into(),
        }
    }

    #[test]
    fn uncommitted_changes_are_not_recorded() {
        assert!(!should_record(
            &Verdict::Pass,
            &working_tree(true),
            &commit(),
            false
        ));
    }

    #[test]
    fn a_clean_working_tree_is_recorded() {
        assert!(should_record(
            &Verdict::Pass,
            &working_tree(false),
            &commit(),
            false
        ));
    }

    #[test]
    fn aborted_and_invalid_runs_are_not_recorded() {
        for verdict in [Verdict::Aborted, Verdict::Invalid] {
            assert!(!should_record(&verdict, &commit(), &commit(), false));
        }
    }

    #[test]
    fn fail_and_inconclusive_are_recorded() {
        for verdict in [Verdict::Fail, Verdict::Inconclusive] {
            assert!(should_record(&verdict, &commit(), &commit(), false));
        }
    }

    #[test]
    fn recording_can_be_disabled() {
        assert!(!should_record(&Verdict::Pass, &commit(), &commit(), true));
    }

    #[test]
    fn each_entry_is_one_line_that_reads_back() {
        let path = temporary_path("history.jsonl");
        let _ = fs::remove_file(&path);
        append_entry(&path, &sample_entry()).unwrap();
        append_entry(&path, &sample_entry()).unwrap();
        let text = fs::read_to_string(&path).unwrap();
        assert_eq!(text.lines().count(), 2);
        for line in text.lines() {
            let read_back: HistoryEntry = serde_json::from_str(line).unwrap();
            assert_eq!(read_back, sample_entry());
        }
        fs::remove_file(&path).unwrap();
    }

    #[test]
    fn a_file_option_is_recorded_by_name_and_fingerprint() {
        let path = temporary_path("net.nnue");
        fs::write(&path, b"a").unwrap();
        let option = EngineOption {
            name: "EvalFile".into(),
            value: path.to_string_lossy().into_owned(),
        };
        let recorded = describe_engine_option(&option);
        let file_name = path.file_name().unwrap().to_string_lossy();
        assert_eq!(recorded, format!("EvalFile={file_name}#af63dc4c8601ec8c"));
        assert!(!recorded.contains('/'));
        fs::remove_file(&path).unwrap();
    }

    #[test]
    fn a_plain_option_is_recorded_as_is() {
        let option = EngineOption {
            name: "Hash".into(),
            value: "16".into(),
        };
        assert_eq!(describe_engine_option(&option), "Hash=16");
    }

    #[test]
    fn the_fingerprint_matches_the_fnv1a_reference() {
        assert_eq!(content_fingerprint(b"a"), 0xaf63_dc4c_8601_ec8c);
    }

    #[test]
    fn the_machine_is_described_without_its_hostname() {
        let description = machine_description();
        assert!(description.contains("cores"), "{description}");
    }
}
