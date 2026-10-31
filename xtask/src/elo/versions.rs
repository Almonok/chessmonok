//! Which versions of the code are being compared.
//!
//! A match is only meaningful if each side is exactly the code it
//! claims to be. So the harness never takes an engine binary from the user:
//! it takes git references (a branch, a tag, a commit hash, `HEAD~1`...),
//! turns each into the full hash of one commit, and builds that commit
//! itself (see `build.rs`).

use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};

/// One of the two engines in a match.
#[derive(Clone, Debug)]
pub struct Side {
    /// `new` or `base`: the name fastchess and the summary use.
    pub name: String,
    pub code_version: CodeVersion,
    /// A placeholder path until the engine is built.
    pub engine_binary: PathBuf,
}

/// The version of the code a side runs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CodeVersion {
    /// An exact, committed snapshot of the code.
    Commit { hash: String },
    /// The files on disk right now: the `head_commit` plus any uncommitted
    /// changes, which no commit hash describes.
    WorkingTree {
        head_commit: String,
        has_uncommitted_changes: bool,
    },
}

impl CodeVersion {
    /// How this version is written in summaries and in the history file.
    pub fn label(&self) -> String {
        match self {
            CodeVersion::Commit { hash } => hash.clone(),
            CodeVersion::WorkingTree {
                head_commit,
                has_uncommitted_changes: false,
            } => head_commit.clone(),
            CodeVersion::WorkingTree {
                head_commit,
                has_uncommitted_changes: true,
            } => format!("{head_commit}-dirty"),
        }
    }

    /// Whether this version points at code that someone could check out
    /// again later.
    pub fn is_exact(&self) -> bool {
        !matches!(
            self,
            CodeVersion::WorkingTree {
                has_uncommitted_changes: true,
                ..
            }
        )
    }
}

/// Run `git` with `arguments`, in `directory` if given, and return what it
/// printed.
pub fn run_git(arguments: &[&str], directory: Option<&Path>) -> Result<String> {
    let mut command = Command::new("git");
    command.args(arguments);
    if let Some(directory) = directory {
        command.current_dir(directory);
    }
    let output = command.output().context("cannot run git")?;
    if !output.status.success() {
        bail!(
            "git {} failed: {}",
            arguments.join(" "),
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// The root directory of the chessmonok checkout.
pub fn repository_root() -> Result<PathBuf> {
    run_git(&["rev-parse", "--show-toplevel"], None).map(PathBuf::from)
}

/// The full hash of the commit that `git_reference` names: a branch, a tag,
/// a short hash, `HEAD~1`... (git calls these "revisions"; see
/// `git help revisions`). Branches move, so the harness resolves each
/// reference once, at the start, and uses the full hash from then on.
pub fn commit_hash_of(git_reference: &str) -> Result<String> {
    // `^{commit}` follows a tag to the commit it points at and refuses
    // anything that is not a commit.
    let commit = format!("{git_reference}^{{commit}}");
    run_git(&["rev-parse", "--verify", "--quiet", &commit], None)
        .with_context(|| format!("unknown git reference `{git_reference}`"))
}

/// The working tree as a code version, seen from the current directory.
pub fn working_tree_version() -> Result<CodeVersion> {
    working_tree_version_in(Path::new("."))
}

fn working_tree_version_in(directory: &Path) -> Result<CodeVersion> {
    Ok(CodeVersion::WorkingTree {
        head_commit: run_git(&["rev-parse", "--verify", "HEAD^{commit}"], Some(directory))?,
        has_uncommitted_changes: has_uncommitted_changes(directory)?,
    })
}

/// Whether any file of the repository differs from the last commit. The
/// history file does not count: recording a result must not make the next
/// run look modified.
fn has_uncommitted_changes(directory: &Path) -> Result<bool> {
    // `cargo xtask` runs from any subdirectory, and paths given to git are
    // relative to the current one. `:/` means "the whole repository", and
    // `top` anchors the exclusion at the repository root.
    let changed_files = run_git(
        &[
            "status",
            "--porcelain",
            "--",
            ":/",
            ":(top,exclude)elo/history.jsonl",
        ],
        Some(directory),
    )?;
    Ok(!changed_files.is_empty())
}

/// The distinct versions to build: one when a version is compared with
/// itself (the harness's self-test), two otherwise.
pub fn versions_to_build<'a>(new: &'a CodeVersion, base: &'a CodeVersion) -> Vec<&'a CodeVersion> {
    if new == base {
        vec![new]
    } else {
        vec![new, base]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn commit(hash: &str) -> CodeVersion {
        CodeVersion::Commit { hash: hash.into() }
    }

    #[test]
    fn a_self_test_builds_once() {
        assert_eq!(versions_to_build(&commit("abc"), &commit("abc")).len(), 1);
    }

    #[test]
    fn different_versions_build_twice() {
        assert_eq!(versions_to_build(&commit("abc"), &commit("def")).len(), 2);
    }

    #[test]
    fn an_unknown_reference_is_named_in_the_error() {
        let error = format!("{:#}", commit_hash_of("no-such-ref-xyz").unwrap_err());
        assert!(error.contains("no-such-ref-xyz"), "{error}");
    }

    #[test]
    fn a_modified_working_tree_is_labelled_dirty() {
        let version = CodeVersion::WorkingTree {
            head_commit: "abc".into(),
            has_uncommitted_changes: true,
        };
        assert_eq!(version.label(), "abc-dirty");
        assert!(!version.is_exact());
    }

    /// A throwaway repository with `a.txt`, `sub/b.txt` and
    /// `elo/history.jsonl` committed.
    fn scratch_repository(name: &str) -> PathBuf {
        let repository =
            std::env::temp_dir().join(format!("xtask-repository-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&repository);
        fs::create_dir_all(repository.join("sub")).unwrap();
        fs::create_dir_all(repository.join("elo")).unwrap();
        fs::write(repository.join("a.txt"), "a").unwrap();
        fs::write(repository.join("sub/b.txt"), "b").unwrap();
        fs::write(repository.join("elo/history.jsonl"), "").unwrap();
        let identity = ["-c", "user.name=test", "-c", "user.email=test@test"];
        run_git(&["init", "-q"], Some(&repository)).unwrap();
        run_git(&["add", "-A"], Some(&repository)).unwrap();
        run_git(
            &[&identity[..], &["commit", "-q", "-m", "init"]].concat(),
            Some(&repository),
        )
        .unwrap();
        repository
    }

    #[test]
    fn a_change_outside_the_current_directory_counts() {
        let repository = scratch_repository("changes");
        fs::write(repository.join("a.txt"), "changed").unwrap();
        let version = working_tree_version_in(&repository.join("sub")).unwrap();
        assert!(!version.is_exact(), "{version:?}");
    }

    #[test]
    fn a_change_to_the_history_file_alone_does_not_count() {
        let repository = scratch_repository("history");
        fs::write(repository.join("elo/history.jsonl"), "{}\n").unwrap();
        let version = working_tree_version_in(&repository.join("sub")).unwrap();
        assert!(version.is_exact(), "{version:?}");
    }
}
