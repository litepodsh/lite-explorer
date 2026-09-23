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

    let mut current = PathBuf::new();
    for component in path.components() {
        current.push(component.as_os_str());
        if fs::symlink_metadata(&current)
            .map_err(|_| INVALID_PATH)?
            .file_type()
            .is_symlink()
        {
            return Err(INVALID_PATH.into());
        }
    }

    let metadata = fs::metadata(path).map_err(|_| INVALID_PATH)?;
    if matches!(expected, ExpectedKind::File) && !metadata.is_file()
        || matches!(expected, ExpectedKind::Directory) && !metadata.is_dir()
    {
        return Err(INVALID_PATH.into());
    }
    Ok(path.to_path_buf())
}

pub fn validate_directory(path: &Path) -> Result<PathBuf, String> {
    validate_existing(path, ExpectedKind::Directory)
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
}
