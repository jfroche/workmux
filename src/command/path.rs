use crate::vcs;
use anyhow::{Result, anyhow};

pub fn run(name: &str) -> Result<()> {
    let vcs = vcs::detect_vcs()?;
    // Smart resolution: try handle first, then branch name
    let (path, _branch) = vcs.find_workspace(name).map_err(|_| {
        anyhow!(
            "Worktree '{}' not found. Use 'workmux list' to see available worktrees.",
            name
        )
    })?;
    println!("{}", path.display());
    Ok(())
}
