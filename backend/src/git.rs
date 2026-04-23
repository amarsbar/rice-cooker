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

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use tempfile::tempdir;

    fn make_bare_repo() -> (tempfile::TempDir, PathBuf, String) {
        let dir = tempdir().expect("tempdir");
        let work = dir.path().join("work");
        let bare = dir.path().join("repo.git");

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
        fs::write(work.join("README"), b"rice").expect("write README");
        run(&["add", "."], &work);
        run(&["commit", "-m", "init"], &work);

        // Grab the freshly committed SHA so tests can check out specific commits.
        let sha_out = Command::new("git")
            .args(["rev-parse", "HEAD"])
            .current_dir(&work)
            .output()
            .expect("git rev-parse");
        let sha = String::from_utf8(sha_out.stdout)
            .expect("utf-8 sha")
            .trim()
            .to_string();

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

        (dir, bare, sha)
    }

    #[test]
    fn preflight_returns_ok_when_git_available() {
        assert!(preflight().is_ok());
    }

    #[test]
    fn clone_at_commit_checks_out_the_requested_sha() {
        let (_guard, bare, sha) = make_bare_repo();
        let dest_dir = tempdir().expect("dest tempdir");
        let dest = dest_dir.path().join("clone");

        clone_at_commit(bare.to_str().unwrap(), &sha, &dest).expect("clone_at_commit ok");

        assert!(dest.join(".git").exists());
        let head = Command::new("git")
            .args(["-C", dest.to_str().unwrap(), "rev-parse", "HEAD"])
            .output()
            .expect("rev-parse");
        assert_eq!(String::from_utf8_lossy(&head.stdout).trim(), sha);
    }

    #[test]
    fn clone_at_commit_refuses_non_hex_non_placeholder_commit() {
        let dest_dir = tempdir().expect("dest tempdir");
        let dest = dest_dir.path().join("clone");
        // "HEAD" isn't hex — the defense-in-depth guard should reject it
        // before we spawn git.
        let err = clone_at_commit("/does/not/matter", "HEAD", &dest).unwrap_err();
        assert!(err.to_string().contains("refusing commit"), "got: {err:#}");
    }
}
