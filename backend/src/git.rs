use std::fs;
use std::io;
use std::path::Path;
use std::process::{Command, Stdio};

use anyhow::Context;

pub fn preflight() -> anyhow::Result<()> {
    let status = Command::new("git")
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();

    match status {
        Ok(s) if s.success() => Ok(()),
        Ok(_) => anyhow::bail!("git is required but returned a non-zero exit code"),
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            anyhow::bail!("git is required but was not found on PATH")
        }
        Err(e) => anyhow::bail!("git is required but could not be launched: {}", e),
    }
}

pub fn clone_or_update(repo_url: &str, dest: &Path, log_file: &Path) -> anyhow::Result<()> {
    if repo_url.starts_with('-') {
        anyhow::bail!("refusing repo URL starting with '-': {}", repo_url);
    }

    // Fresh log per clone/update so a retry doesn't include stderr from the
    // previous failed attempt. `launch_detached` truncates again when it opens
    // the same file for qs stderr; this truncation covers the failed-before-launch
    // window that truncation wouldn't otherwise touch.
    fs::write(log_file, b"")
        .with_context(|| format!("truncating git log {}", log_file.display()))?;

    if dest.join(".git").exists() {
        let fetch_status = git_cmd()
            .args(["-C"])
            .arg(dest)
            .args(["fetch", "--depth", "1", "origin", "HEAD"])
            .stderr(open_log(log_file)?)
            .status()
            .context("spawning git fetch")?;

        if !fetch_status.success() {
            anyhow::bail!("git fetch failed with exit code {:?}", fetch_status.code());
        }

        let reset_status = git_cmd()
            .args(["-C"])
            .arg(dest)
            .args(["reset", "--hard", "FETCH_HEAD"])
            .stderr(open_log(log_file)?)
            .status()
            .context("spawning git reset")?;

        if !reset_status.success() {
            anyhow::bail!("git reset failed with exit code {:?}", reset_status.code());
        }
    } else {
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("creating rice cache parent {}", parent.display()))?;
        }

        // `--` separates options from positional arguments; without it, a repo URL starting
        // with `-` (e.g. `--upload-pack=...`) would be interpreted as a git option — a
        // known RCE vector for git wrappers.
        let clone_status = git_cmd()
            .args(["clone", "--depth", "1", "--", repo_url])
            .arg(dest)
            .stderr(open_log(log_file)?)
            .status()
            .context("spawning git clone")?;

        if !clone_status.success() {
            anyhow::bail!("git clone failed with exit code {:?}", clone_status.code());
        }
    }

    Ok(())
}

/// Clone a repo and check out a specific commit. `dest` must not already
/// exist — caller is responsible for deleting it first.
pub fn clone_at_commit(repo_url: &str, commit: &str, dest: &Path) -> anyhow::Result<()> {
    if repo_url.starts_with('-') {
        anyhow::bail!("refusing repo URL starting with '-': {repo_url}");
    }
    if commit.starts_with('-') {
        anyhow::bail!("refusing commit starting with '-': {commit}");
    }
    // Defense-in-depth: reject commit args that aren't a hex SHA
    // (≥7 chars) or the PLACEHOLDER sentinel. The catalog validator
    // already enforces this, but a future caller that bypasses the
    // catalog still can't pass arbitrary strings into git checkout.
    let is_hex_sha = commit.len() >= 7 && commit.chars().all(|c| c.is_ascii_hexdigit());
    let is_placeholder = commit.contains("PLACEHOLDER");
    if !is_hex_sha && !is_placeholder {
        anyhow::bail!(
            "refusing commit that is neither a hex SHA (≥7 chars) nor PLACEHOLDER: {commit}"
        );
    }
    // Full clone (not shallow), since we need a specific historical SHA.
    // --no-checkout so we land without a working tree and can check out
    // the pinned commit explicitly.
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent).with_context(|| format!("creating {}", parent.display()))?;
    }
    let status = git_cmd()
        .args(["clone", "--no-checkout", "--", repo_url])
        .arg(dest)
        .status()
        .context("git clone")?;
    if !status.success() {
        anyhow::bail!("git clone failed: exit {:?}", status.code());
    }
    let checkout = git_cmd()
        .args(["-C"])
        .arg(dest)
        .args(["checkout", "--detach", commit])
        .status()
        .context("git checkout")?;
    if !checkout.success() {
        anyhow::bail!("git checkout {commit} failed: exit {:?}", checkout.code());
    }
    Ok(())
}

// Preconfigured `git` invocation. `-c protocol.ext.allow=never` forbids the `ext::`
// protocol (arbitrary-command RCE) — cheap defense-in-depth in case a curated entry
// or upstream rename ever sneaks one in. The terminal/askpass env guards were
// dropped when the backend moved to a curated-URL world: public rice repos won't
// prompt for credentials.
fn git_cmd() -> Command {
    let mut cmd = Command::new("git");
    cmd.args(["-c", "protocol.ext.allow=never"]);
    cmd
}

fn open_log(log_file: &Path) -> anyhow::Result<fs::File> {
    fs::File::options()
        .create(true)
        .append(true)
        .open(log_file)
        .map_err(|e| anyhow::anyhow!("opening git log {}: {}", log_file.display(), e))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use tempfile::tempdir;

    fn make_bare_repo() -> (tempfile::TempDir, PathBuf) {
        let dir = tempdir().expect("tempdir");
        let work = dir.path().join("work");
        let bare = dir.path().join("repo.git");

        // Init a normal repo, configure identity, commit an empty file.
        let run = |args: &[&str], cwd: &Path| {
            let ok = Command::new("git")
                .args(args)
                .current_dir(cwd)
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .unwrap_or_else(|_| panic!("git {:?} failed to spawn", args))
                .success();
            assert!(ok, "git {:?} in {:?} failed", args, cwd);
        };

        fs::create_dir_all(&work).expect("create work dir");
        run(&["init"], &work);
        run(&["config", "user.email", "test@example.com"], &work);
        run(&["config", "user.name", "Test"], &work);
        // Create an initial commit so HEAD exists.
        fs::write(work.join("README"), b"rice").expect("write README");
        run(&["add", "."], &work);
        run(&["commit", "-m", "init"], &work);

        // Clone into a bare repo — this becomes the "remote".
        let ok = Command::new("git")
            .args(["clone", "--bare"])
            .arg(&work)
            .arg(&bare)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .expect("git clone --bare")
            .success();
        assert!(ok, "git clone --bare failed");

        (dir, bare)
    }

    #[test]
    fn preflight_returns_ok_when_git_available() {
        assert!(preflight().is_ok());
    }

    #[test]
    fn clone_into_empty_dest_creates_git_dir() {
        let (_bare_guard, bare_repo) = make_bare_repo();
        let dest_dir = tempdir().expect("dest tempdir");
        let dest = dest_dir.path().join("clone");
        let log_dir = tempdir().expect("log tempdir");
        let log_file = log_dir.path().join("git.log");

        clone_or_update(bare_repo.to_str().unwrap(), &dest, &log_file)
            .expect("clone_or_update should succeed");

        assert!(
            dest.join(".git").exists(),
            ".git directory should exist after clone"
        );
    }

    #[test]
    fn clone_with_invalid_url_returns_err_and_log_populated() {
        let dest_dir = tempdir().expect("dest tempdir");
        let dest = dest_dir.path().join("clone");
        let log_dir = tempdir().expect("log tempdir");
        let log_file = log_dir.path().join("git.log");

        let result = clone_or_update("/no/such/path.git", &dest, &log_file);

        assert!(result.is_err(), "clone of bogus URL should fail");
        let log_len = fs::metadata(&log_file).map(|m| m.len()).unwrap_or(0);
        assert!(
            log_len > 0,
            "log file should be non-empty after failed clone"
        );
    }
}
