//! GitLab support via the `glab` CLI.
//!
//! Mirrors the small slice of `github` used by the `workmux add --pr` flow:
//! fetching merge-request details and mapping them onto the shared
//! [`crate::github::PrDetails`] shape so the downstream fork/worktree logic is
//! forge-agnostic.

use anyhow::{Context, Result, anyhow};
use serde::Deserialize;
use std::path::Path;
use std::process::Command;
use tracing::debug;

use crate::github::{Author, PrDetails, RepositoryOwner};

/// Raw subset of `glab mr view <iid> -F json` output.
#[derive(Debug, Deserialize)]
struct GlabMr {
    source_branch: String,
    title: String,
    state: String,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    work_in_progress: bool,
    author: GlabUser,
    #[serde(default)]
    source_project_id: u64,
    #[serde(default)]
    target_project_id: u64,
}

#[derive(Debug, Deserialize)]
struct GlabUser {
    username: String,
}

/// Raw subset of `glab api projects/<id>` output, used to resolve a fork's
/// namespace (owner) for cross-project merge requests.
#[derive(Debug, Deserialize)]
struct GlabProject {
    namespace: GlabNamespace,
}

#[derive(Debug, Deserialize)]
struct GlabNamespace {
    path: String,
}

/// Normalize a GitLab MR state (`opened`/`closed`/`merged`/`locked`) to the
/// uppercase GitHub-style values the rest of the codebase compares against.
fn normalize_state(state: &str) -> String {
    match state {
        "opened" => "OPEN".to_string(),
        other => other.to_uppercase(),
    }
}

fn run_glab(repo_root: Option<&Path>, args: &[&str]) -> Result<Vec<u8>> {
    let mut command = Command::new("glab");
    if let Some(path) = repo_root {
        command.current_dir(path);
    }
    let output = command.args(args).output();

    let output = match output {
        Ok(out) => out,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            debug!("gitlab:glab CLI not found");
            return Err(anyhow!(
                "GitLab CLI (glab) is required for --pr on GitLab repositories. \
                 Install from https://gitlab.com/gitlab-org/cli"
            ));
        }
        Err(e) => return Err(e).context("Failed to execute glab command"),
    };

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow!("glab command failed: {}", stderr.trim()));
    }

    Ok(output.stdout)
}

/// Fetch merge-request details in a specific repository path.
pub fn get_pr_details_in(repo_root: Option<&Path>, pr_number: u32) -> Result<PrDetails> {
    let stdout = run_glab(
        repo_root,
        &["mr", "view", &pr_number.to_string(), "-F", "json"],
    )
    .with_context(|| format!("Failed to fetch merge request !{}", pr_number))?;

    let mr: GlabMr =
        serde_json::from_slice(&stdout).context("Failed to parse glab mr view JSON output")?;

    // For cross-project (fork) MRs the source project differs from the target.
    // Resolve its namespace so downstream fork-remote logic can construct the
    // fork URL; for same-repo MRs fall back to the origin owner.
    let is_fork = mr.source_project_id != 0
        && mr.target_project_id != 0
        && mr.source_project_id != mr.target_project_id;

    let owner_login = if is_fork {
        fork_namespace(repo_root, mr.source_project_id).unwrap_or_else(|_| {
            // Fall back to the author's username if the project lookup fails.
            mr.author.username.clone()
        })
    } else {
        crate::git::get_repo_owner_in(repo_root)
            .unwrap_or_else(|_| mr.author.username.clone())
    };

    Ok(PrDetails {
        head_ref_name: mr.source_branch,
        head_repository_owner: RepositoryOwner { login: owner_login },
        state: normalize_state(&mr.state),
        is_draft: mr.draft || mr.work_in_progress,
        title: mr.title,
        author: Author {
            login: mr.author.username,
        },
    })
}

/// Resolve the namespace (owner) of a source project by id.
fn fork_namespace(repo_root: Option<&Path>, project_id: u64) -> Result<String> {
    let stdout = run_glab(repo_root, &["api", &format!("projects/{}", project_id)])
        .with_context(|| format!("Failed to look up source project {}", project_id))?;
    let project: GlabProject =
        serde_json::from_slice(&stdout).context("Failed to parse glab api project JSON")?;
    Ok(project.namespace.path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_gitlab_states() {
        assert_eq!(normalize_state("opened"), "OPEN");
        assert_eq!(normalize_state("closed"), "CLOSED");
        assert_eq!(normalize_state("merged"), "MERGED");
        assert_eq!(normalize_state("locked"), "LOCKED");
    }
}
