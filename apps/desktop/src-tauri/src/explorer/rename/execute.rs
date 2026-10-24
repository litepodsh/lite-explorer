//! Applies a list of renames all-or-nothing, in two phases through temporary names.
//!
//! Going through temporary names lets swaps (a to b while b to a), chains and
//! case-only changes work, and keeps every original recoverable if something fails.

use std::path::{Path, PathBuf};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Move {
    pub from: PathBuf,
    pub to: PathBuf,
}

#[derive(Debug, PartialEq, Eq)]
pub struct Failure {
    /// The name that could not be renamed.
    pub name: String,
    pub reason: String,
    /// Problems while putting the already-moved items back. Empty when everything was restored.
    pub restore_errors: Vec<String>,
}

fn name_of(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default()
}

fn temporary_for(path: &Path, index: usize, batch: &str) -> PathBuf {
    path.with_file_name(format!(".liteexplorer-rename-{batch}-{index}"))
}

/// `rename` performs one move. `progress(done, total)` is called after each final move.
pub fn execute(
    moves: &[Move],
    rename: &dyn Fn(&Path, &Path) -> Result<(), String>,
    progress: &mut dyn FnMut(usize, usize),
) -> Result<(), Failure> {
    let batch = uuid::Uuid::new_v4().simple().to_string();
    let temporaries: Vec<PathBuf> = moves
        .iter()
        .enumerate()
        .map(|(index, step)| temporary_for(&step.from, index, &batch))
        .collect();

    // Phase 1: every original moves out of the way. When item `index` fails, items
    // `0..index` are already out of the way.
    for (index, step) in moves.iter().enumerate() {
        if let Err(reason) = rename(&step.from, &temporaries[index]) {
            let restore_errors = restore(moves, &temporaries, index, 0, rename);
            return Err(Failure {
                name: name_of(&step.from),
                reason,
                restore_errors,
            });
        }
    }

    // Phase 2: every temporary takes its final name. When item `index` fails, items
    // `0..index` already have their final name.
    for (index, step) in moves.iter().enumerate() {
        if let Err(reason) = rename(&temporaries[index], &step.to) {
            let restore_errors = restore(moves, &temporaries, moves.len(), index, rename);
            return Err(Failure {
                name: name_of(&step.from),
                reason,
                restore_errors,
            });
        }
        progress(index + 1, moves.len());
    }
    Ok(())
}

/// Puts everything back where it was. Items already given their final name first return to
/// their temporary name, so no restore step can land on a name another item still holds.
fn restore(
    moves: &[Move],
    temporaries: &[PathBuf],
    moved_out: usize,
    finalized: usize,
    rename: &dyn Fn(&Path, &Path) -> Result<(), String>,
) -> Vec<String> {
    let mut errors = Vec::new();
    for index in (0..finalized).rev() {
        if let Err(error) = rename(&moves[index].to, &temporaries[index]) {
            errors.push(format!("{}: {error}", name_of(&moves[index].to)));
        }
    }
    for index in (0..moved_out).rev() {
        if let Err(error) = rename(&temporaries[index], &moves[index].from) {
            errors.push(format!("{}: {error}", name_of(&moves[index].from)));
        }
    }
    errors
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{cell::Cell, fs};

    fn real_rename(from: &Path, to: &Path) -> Result<(), String> {
        fs::rename(from, to).map_err(|error| error.to_string())
    }

    fn write(dir: &Path, name: &str, content: &str) -> PathBuf {
        let path = dir.join(name);
        fs::write(&path, content).unwrap();
        path
    }

    fn mv(dir: &Path, from: &str, to: &str) -> Move {
        Move {
            from: dir.join(from),
            to: dir.join(to),
        }
    }

    fn listing(dir: &Path) -> Vec<String> {
        let mut names: Vec<_> = fs::read_dir(dir)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }

    #[test]
    fn renames_files_and_reports_progress() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "a.txt", "A");
        write(dir.path(), "b.txt", "B");
        let seen = std::cell::RefCell::new(Vec::new());
        execute(
            &[mv(dir.path(), "a.txt", "x.txt"), mv(dir.path(), "b.txt", "y.txt")],
            &real_rename,
            &mut |done, total| seen.borrow_mut().push((done, total)),
        )
        .unwrap();
        assert_eq!(listing(dir.path()), ["x.txt", "y.txt"]);
        assert_eq!(fs::read_to_string(dir.path().join("x.txt")).unwrap(), "A");
        assert_eq!(seen.into_inner(), [(1, 2), (2, 2)]);
    }

    #[test]
    fn swaps_two_names_without_losing_content() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "a.txt", "A");
        write(dir.path(), "b.txt", "B");
        execute(
            &[mv(dir.path(), "a.txt", "b.txt"), mv(dir.path(), "b.txt", "a.txt")],
            &real_rename,
            &mut |_, _| {},
        )
        .unwrap();
        assert_eq!(fs::read_to_string(dir.path().join("a.txt")).unwrap(), "B");
        assert_eq!(fs::read_to_string(dir.path().join("b.txt")).unwrap(), "A");
        assert_eq!(listing(dir.path()), ["a.txt", "b.txt"]);
    }

    #[test]
    fn chains_move_each_item_once() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "a", "A");
        write(dir.path(), "b", "B");
        execute(
            &[mv(dir.path(), "a", "b"), mv(dir.path(), "b", "c")],
            &real_rename,
            &mut |_, _| {},
        )
        .unwrap();
        assert_eq!(fs::read_to_string(dir.path().join("b")).unwrap(), "A");
        assert_eq!(fs::read_to_string(dir.path().join("c")).unwrap(), "B");
        assert_eq!(listing(dir.path()), ["b", "c"]);
    }

    #[test]
    fn a_change_of_case_only_works() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "Photo.JPG", "P");
        execute(&[mv(dir.path(), "Photo.JPG", "photo.jpg")], &real_rename, &mut |_, _| {}).unwrap();
        assert_eq!(listing(dir.path()), ["photo.jpg"]);
    }

    #[test]
    fn a_failure_in_the_second_phase_restores_everything() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "a", "A");
        write(dir.path(), "b", "B");
        write(dir.path(), "c", "C");
        let calls = Cell::new(0);
        // Calls 1-3 are phase 1; call 5 is the second final move.
        let flaky = |from: &Path, to: &Path| {
            calls.set(calls.get() + 1);
            if calls.get() == 5 {
                return Err("disk on fire".to_string());
            }
            real_rename(from, to)
        };
        let failure = execute(
            &[mv(dir.path(), "a", "b"), mv(dir.path(), "b", "c"), mv(dir.path(), "c", "d")],
            &flaky,
            &mut |_, _| {},
        )
        .unwrap_err();
        assert_eq!(failure.name, "b");
        assert_eq!(failure.reason, "disk on fire");
        assert!(failure.restore_errors.is_empty(), "{:?}", failure.restore_errors);
        assert_eq!(listing(dir.path()), ["a", "b", "c"]);
        assert_eq!(fs::read_to_string(dir.path().join("a")).unwrap(), "A");
        assert_eq!(fs::read_to_string(dir.path().join("b")).unwrap(), "B");
        assert_eq!(fs::read_to_string(dir.path().join("c")).unwrap(), "C");
    }

    #[test]
    fn a_failure_in_the_first_phase_restores_everything() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "a", "A");
        write(dir.path(), "b", "B");
        let calls = Cell::new(0);
        let flaky = |from: &Path, to: &Path| {
            calls.set(calls.get() + 1);
            if calls.get() == 2 {
                return Err("locked".to_string());
            }
            real_rename(from, to)
        };
        let failure = execute(
            &[mv(dir.path(), "a", "x"), mv(dir.path(), "b", "y")],
            &flaky,
            &mut |_, _| {},
        )
        .unwrap_err();
        assert_eq!(failure.name, "b");
        assert!(failure.restore_errors.is_empty());
        assert_eq!(listing(dir.path()), ["a", "b"]);
    }

    #[test]
    fn an_empty_batch_is_fine() {
        assert!(execute(&[], &real_rename, &mut |_, _| {}).is_ok());
    }
}
