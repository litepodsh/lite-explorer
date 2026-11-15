use std::{
    fs,
    path::{Path, PathBuf},
};

/// The `HEAD` file of the repository that contains `start`, following the `.git` file that
/// worktrees and submodules use instead of a directory.
fn head_file(start: &Path) -> Option<PathBuf> {
    for dir in start.ancestors() {
        let dot_git = dir.join(".git");
        let Ok(metadata) = fs::metadata(&dot_git) else {
            continue;
        };
        if metadata.is_dir() {
            return Some(dot_git.join("HEAD"));
        }
        let pointer = fs::read_to_string(&dot_git).ok()?;
        let target = pointer.trim().strip_prefix("gitdir:")?.trim();
        let git_dir = Path::new(target);
        let git_dir = if git_dir.is_absolute() {
            git_dir.to_path_buf()
        } else {
            dir.join(git_dir)
        };
        return Some(git_dir.join("HEAD"));
    }
    None
}

/// The branch name in a `HEAD` file, or the short commit id when the head is detached.
fn branch_from_head(contents: &str) -> Option<String> {
    let head = contents.trim();
    if let Some(reference) = head.strip_prefix("ref:") {
        let reference = reference.trim();
        let name = reference.strip_prefix("refs/heads/").unwrap_or(reference);
        return (!name.is_empty()).then(|| name.to_owned());
    }
    let is_commit = head.len() >= 7 && head.chars().all(|c| c.is_ascii_hexdigit());
    is_commit.then(|| head[..7].to_owned())
}

/// The current branch of the Git repository that contains `path`, if any. Reads `HEAD` directly
/// instead of running `git`, so it costs a few file reads and works without Git installed.
#[tauri::command]
pub async fn git_branch(path: String) -> Option<String> {
    tauri::async_runtime::spawn_blocking(move || {
        let head = head_file(Path::new(&path))?;
        branch_from_head(&fs::read_to_string(head).ok()?)
    })
    .await
    .ok()
    .flatten()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_branch_of_a_symbolic_head() {
        assert_eq!(branch_from_head("ref: refs/heads/main\n").as_deref(), Some("main"));
        assert_eq!(
            branch_from_head("ref: refs/heads/feat/sidebar").as_deref(),
            Some("feat/sidebar")
        );
    }

    #[test]
    fn shortens_a_detached_head_to_seven_characters() {
        assert_eq!(
            branch_from_head("c9ff8fb1e2d3c4b5a69788776655443322110000\n").as_deref(),
            Some("c9ff8fb")
        );
    }

    #[test]
    fn rejects_contents_that_are_not_a_head() {
        assert_eq!(branch_from_head(""), None);
        assert_eq!(branch_from_head("ref: refs/heads/"), None);
        assert_eq!(branch_from_head("not a commit"), None);
    }

    #[test]
    fn finds_the_head_from_a_nested_folder_and_through_a_gitdir_file() {
        let root = std::env::temp_dir().join(format!("lite-explorer-git-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let repo = root.join("repo");
        fs::create_dir_all(repo.join(".git")).unwrap();
        fs::write(repo.join(".git/HEAD"), "ref: refs/heads/main\n").unwrap();
        fs::create_dir_all(repo.join("src/deep")).unwrap();
        assert_eq!(head_file(&repo.join("src/deep")), Some(repo.join(".git/HEAD")));

        let worktree = root.join("worktree");
        fs::create_dir_all(&worktree).unwrap();
        fs::write(worktree.join(".git"), "gitdir: ../repo/.git\n").unwrap();
        assert_eq!(head_file(&worktree), Some(worktree.join("../repo/.git/HEAD")));

        fs::remove_dir_all(&root).unwrap();
    }
}
