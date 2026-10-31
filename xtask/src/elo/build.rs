//! Building the engine for a given version of the code.
//!
//! A commit is checked out into its own *git worktree* (a second checkout of
//! the same repository, sharing its history, in a separate directory) and
//! built there with that commit's own `Cargo.toml` profiles,
//! `.cargo/config.toml` and `rust-toolchain.toml`. The binary is cached by
//! commit hash, so `main` is built once until it moves. Because each
//! version brings its own build configuration, a build change such as
//! `target-cpu=native` can itself be measured.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

use anyhow::{Context, Result, bail};

use crate::elo::versions::{CodeVersion, run_git};

/// Name of the binary the `engine` crate builds.
const ENGINE_BINARY_NAME: &str = "chessmonok";

/// Build `version` and return the path of its engine binary.
pub fn build_engine(version: &CodeVersion, repository_root: &Path) -> Result<PathBuf> {
    match version {
        CodeVersion::Commit { hash } => build_engine_at_commit(hash, repository_root),
        CodeVersion::WorkingTree { .. } => build_engine_from_working_tree(repository_root),
    }
    .with_context(|| format!("building {}", version.label()))
}

fn build_engine_at_commit(hash: &str, repository_root: &Path) -> Result<PathBuf> {
    let cached_binary = repository_root
        .join("target/elo/bin")
        .join(hash)
        .join(ENGINE_BINARY_NAME);
    if cached_binary.exists() {
        return Ok(cached_binary);
    }
    let worktree = TemporaryWorktree::check_out(hash, repository_root)?;
    let built_binary = cargo_build_engine_in(worktree.path())?;
    copy_atomically_into(&built_binary, cached_binary.parent().expect("has a parent"))
}

fn build_engine_from_working_tree(repository_root: &Path) -> Result<PathBuf> {
    let built_binary = cargo_build_engine_in(repository_root)?;
    // Copied out of `target/release` so that a later build cannot replace
    // the binary while fastchess is running it.
    copy_atomically_into(
        &built_binary,
        &repository_root.join("target/elo/bin/working"),
    )
}

/// `cargo build --release --package engine` in `directory`; returns the
/// binary it built.
fn cargo_build_engine_in(directory: &Path) -> Result<PathBuf> {
    let target_directory = directory.join("target");
    let status = Command::new("cargo")
        .args(["build", "--release", "--package", "engine", "--target-dir"])
        .arg(&target_directory)
        .current_dir(directory)
        // `cargo xtask` runs under rustup, which exports RUSTUP_TOOLCHAIN to
        // its children. Left in place, it would override the
        // `rust-toolchain.toml` of the version being built, so an old commit
        // would be compiled with today's compiler.
        .env_remove("RUSTUP_TOOLCHAIN")
        .status()
        .context("cannot run cargo")?;
    if !status.success() {
        bail!("cargo build failed in {}", directory.display());
    }
    let binary = target_directory.join("release").join(ENGINE_BINARY_NAME);
    if !binary.exists() {
        bail!("cargo succeeded but {} is missing", binary.display());
    }
    Ok(binary)
}

/// Copy `file` into `directory` (created if needed) and return the copy.
///
/// The copy is written under a temporary name, then renamed. A rename within
/// one directory is atomic: anyone looking at the final name sees either
/// nothing or the whole file, never a half-written binary left by an
/// interrupted copy or a concurrent run.
fn copy_atomically_into(file: &Path, directory: &Path) -> Result<PathBuf> {
    fs::create_dir_all(directory)
        .with_context(|| format!("cannot create {}", directory.display()))?;
    let file_name = file.file_name().context("no file name")?;
    let destination = directory.join(file_name);
    let temporary = directory.join(format!(
        ".{}.{}",
        file_name.to_string_lossy(),
        std::process::id()
    ));
    fs::copy(file, &temporary)
        .with_context(|| format!("cannot copy to {}", temporary.display()))?;
    fs::rename(&temporary, &destination)
        .with_context(|| format!("cannot move to {}", destination.display()))?;
    Ok(destination)
}

/// A git worktree that is removed when this value is dropped.
///
/// This is the RAII pattern ("resource acquisition is initialisation"): tie
/// a cleanup to a value's lifetime, and the compiler runs it on every exit
/// path, early `?` returns and panics included, so no path can forget it.
#[derive(Debug)]
struct TemporaryWorktree {
    path: PathBuf,
}

impl TemporaryWorktree {
    fn check_out(hash: &str, repository_root: &Path) -> Result<Self> {
        let path = fresh_worktree_path(hash);
        let path_text = path.to_str().context("worktree path is not UTF-8")?;
        run_git(
            &["worktree", "add", "--detach", path_text, hash],
            Some(repository_root),
        )?;
        Ok(TemporaryWorktree { path })
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TemporaryWorktree {
    fn drop(&mut self) {
        if let Some(path_text) = self.path.to_str() {
            let _ = run_git(&["worktree", "remove", "--force", path_text], None);
        }
        let _ = fs::remove_dir_all(&self.path);
        let _ = run_git(&["worktree", "prune"], None);
    }
}

/// A new, unused directory to check a commit out into.
///
/// It is outside the repository on purpose. Cargo merges the
/// `.cargo/config.toml` of every parent directory, so a worktree under this
/// checkout would inherit today's build flags on top of the old commit's
/// own, and a build change could never be measured.
///
/// The process id plus a counter give every build its own directory, so two
/// builds of the same commit at once (two runs, or two threads) stay out of
/// each other's way.
fn fresh_worktree_path(hash: &str) -> PathBuf {
    // A `static` atomic is a global counter that needs no lock and no
    // `unsafe`: `fetch_add` reads and increments in one indivisible step.
    static BUILDS_STARTED: AtomicUsize = AtomicUsize::new(0);
    let build_number = BUILDS_STARTED.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir()
        .join("chessmonok-elo/worktrees")
        .join(format!("{hash}-{}-{build_number}", std::process::id()))
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::elo::versions::{commit_hash_of, repository_root};

    #[test]
    fn every_build_gets_its_own_worktree_directory() {
        let (first, second) = (fresh_worktree_path("abc"), fresh_worktree_path("abc"));
        assert_ne!(first, second);
        let name = first.file_name().unwrap().to_string_lossy().into_owned();
        assert!(
            name.starts_with(&format!("abc-{}-", std::process::id())),
            "{name}"
        );
    }

    #[test]
    fn worktrees_live_outside_the_repository() {
        let root = repository_root().unwrap();
        assert!(!fresh_worktree_path("abc").starts_with(&root));
    }

    #[test]
    fn copies_land_whole_under_their_final_name() {
        let directory = std::env::temp_dir().join(format!("xtask-copy-{}", std::process::id()));
        let _ = fs::remove_dir_all(&directory);
        let source = std::env::temp_dir().join(format!("xtask-source-{}", std::process::id()));
        fs::write(&source, b"binary").unwrap();
        let copy = copy_atomically_into(&source, &directory).unwrap();
        assert_eq!(fs::read(&copy).unwrap(), b"binary");
        let files_left = fs::read_dir(&directory).unwrap().count();
        assert_eq!(files_left, 1, "no temporary file may be left behind");
    }

    /// Held by every test that builds the real engine, so that a test
    /// looking at the list of worktrees never sees another test's build.
    pub(crate) static REAL_BUILDS: std::sync::Mutex<()> = std::sync::Mutex::new(());

    /// Builds `main` for real, then again from the cache.
    #[test]
    #[ignore = "builds the engine; run with --ignored"]
    fn builds_main_and_caches_it() {
        let _turn = REAL_BUILDS
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let root = repository_root().unwrap();
        let main = CodeVersion::Commit {
            hash: commit_hash_of("main").unwrap(),
        };
        let binary = build_engine(&main, &root).unwrap();
        assert!(binary.exists());
        assert_eq!(build_engine(&main, &root).unwrap(), binary);
        let worktrees = run_git(&["worktree", "list"], None).unwrap();
        assert!(!worktrees.contains("chessmonok-elo"), "{worktrees}");
    }
}
