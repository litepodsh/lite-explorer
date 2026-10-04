use std::{
    fs,
    path::{Component, Path, PathBuf},
};

const INVALID_PATH: &str = "Invalid local path";
const INVALID_NAME: &str = "Invalid item name";

#[derive(Clone, Copy)]
pub enum ExpectedKind {
    Any,
    File,
    Directory,
}

pub fn validate_existing(path: &Path, expected: ExpectedKind) -> Result<PathBuf, String> {
    if !path.is_absolute()
        || path
            .components()
            .any(|component| matches!(component, Component::CurDir | Component::ParentDir))
    {
        return Err(INVALID_PATH.into());
    }

    // Only the final component must not be a symlink; ancestors may be
    // (macOS: /var, /tmp and /etc link into /private).
    if fs::symlink_metadata(path)
        .map_err(|_| INVALID_PATH)?
        .file_type()
        .is_symlink()
    {
        return Err(INVALID_PATH.into());
    }

    let metadata = fs::metadata(path).map_err(|_| INVALID_PATH)?;
    if matches!(expected, ExpectedKind::File) && !metadata.is_file()
        || matches!(expected, ExpectedKind::Directory) && !metadata.is_dir()
    {
        return Err(INVALID_PATH.into());
    }
    Ok(path.to_path_buf())
}

/// Read-only access (info, preview, open) follows symlinks to their target, like Finder.
pub fn validate_readable(path: &Path) -> Result<PathBuf, String> {
    if !path.is_absolute() {
        return Err(INVALID_PATH.into());
    }
    let real = fs::canonicalize(path).map_err(|_| INVALID_PATH)?;
    validate_existing(&real, ExpectedKind::Any)
}

/// Follows symlinks like Finder does (e.g. `/var` -> `/private/var`) and returns the real
/// directory, so listings and destinations resolve to the link target.
pub fn validate_directory(path: &Path) -> Result<PathBuf, String> {
    if !path.is_absolute() {
        return Err(INVALID_PATH.into());
    }
    let real = fs::canonicalize(path).map_err(|_| INVALID_PATH)?;
    validate_existing(&real, ExpectedKind::Directory)
}

pub fn validate_child_name(name: &str) -> Result<&str, String> {
    let name = name.trim();
    if name.is_empty() || matches!(name, "." | "..") || name.contains(['/', '\\']) {
        return Err(INVALID_NAME.into());
    }
    Ok(name)
}

pub fn child_path(parent: &Path, name: &str) -> Result<PathBuf, String> {
    Ok(validate_directory(parent)?.join(validate_child_name(name)?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_relative_and_nested_names() {
        assert!(validate_existing(Path::new("notes.txt"), ExpectedKind::Any).is_err());
        assert!(validate_child_name("../escape").is_err());
        assert!(validate_child_name("nested/name").is_err());
        assert!(validate_child_name("nested\\name").is_err());
    }

    #[test]
    fn accepts_an_absolute_real_child_name() {
        let root = std::env::temp_dir().canonicalize().unwrap();
        assert_eq!(
            child_path(&root, "report.txt").unwrap(),
            root.join("report.txt")
        );
    }

    #[cfg(unix)]
    #[test]
    fn rejects_symlink_paths() {
        use std::os::unix::fs::symlink;
        let root = std::env::temp_dir().join(format!("liteexplorer-path-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let target = root.join("target");
        fs::write(&target, "safe").unwrap();
        let link = root.join("link");
        symlink(&target, &link).unwrap();
        assert!(validate_existing(&link, ExpectedKind::File).is_err());
        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn accepts_paths_under_symlinked_ancestors() {
        use std::os::unix::fs::symlink;
        let root =
            std::env::temp_dir().join(format!("liteexplorer-ancestor-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let real = root.join("real");
        fs::create_dir_all(real.join("child")).unwrap();
        let link = root.join("link");
        symlink(&real, &link).unwrap();
        assert!(validate_directory(&link.join("child")).is_ok());
        assert_eq!(
            validate_directory(&link).unwrap(),
            real.canonicalize().unwrap()
        );
        fs::remove_dir_all(root).unwrap();
    }
}
