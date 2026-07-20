//! Forge detection and dispatch for PR/MR operations.
//!
//! `workmux add --pr` works against both GitHub (via `gh`) and GitLab (via
//! `glab`). The forge is inferred from the `origin` remote host so the rest of
//! the `--pr` flow stays forge-agnostic.

use anyhow::Result;
use std::path::Path;

use crate::git;
use crate::github::PrDetails;
use crate::{github, gitlab};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Forge {
    GitHub,
    GitLab,
}

impl Forge {
    /// Detect the forge from the `origin` remote host in the given workdir.
    /// Defaults to GitHub when the host is unknown, preserving existing
    /// behavior for the common case and self-hosted GitHub Enterprise.
    pub fn detect(workdir: Option<&Path>) -> Result<Forge> {
        let host = git::get_origin_host_in(workdir)?;
        Ok(Self::from_host(&host))
    }

    fn from_host(host: &str) -> Forge {
        let host = host.to_ascii_lowercase();
        if host == "gitlab.com" || host.starts_with("gitlab.") || host.contains(".gitlab.") {
            Forge::GitLab
        } else {
            Forge::GitHub
        }
    }

    /// Fetch PR/MR details, dispatching to the appropriate CLI.
    pub fn get_pr_details(&self, workdir: Option<&Path>, pr_number: u32) -> Result<PrDetails> {
        match self {
            Forge::GitHub => github::get_pr_details_in(workdir, pr_number),
            Forge::GitLab => gitlab::get_pr_details_in(workdir, pr_number),
        }
    }

    /// The server-side ref where the forge exposes a PR/MR head commit,
    /// fetchable from `origin` without a fork remote.
    pub fn pr_head_ref(&self, pr_number: u32) -> String {
        match self {
            Forge::GitHub => format!("refs/pull/{}/head", pr_number),
            Forge::GitLab => format!("refs/merge-requests/{}/head", pr_number),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_gitlab_hosts() {
        assert_eq!(Forge::from_host("gitlab.com"), Forge::GitLab);
        assert_eq!(Forge::from_host("GitLab.com"), Forge::GitLab);
        assert_eq!(Forge::from_host("gitlab.example.org"), Forge::GitLab);
        assert_eq!(Forge::from_host("gitlab.imio.be"), Forge::GitLab);
    }

    #[test]
    fn defaults_to_github() {
        assert_eq!(Forge::from_host("github.com"), Forge::GitHub);
        assert_eq!(Forge::from_host("github.example.com"), Forge::GitHub);
        assert_eq!(Forge::from_host("example.com"), Forge::GitHub);
    }

    #[test]
    fn pr_head_ref_per_forge() {
        assert_eq!(Forge::GitHub.pr_head_ref(42), "refs/pull/42/head");
        assert_eq!(Forge::GitLab.pr_head_ref(42), "refs/merge-requests/42/head");
    }
}
