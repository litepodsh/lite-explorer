//! Name search through Spotlight's index (`mdfind`), the same index Finder searches. It
//! answers in milliseconds where walking a large folder takes seconds, but it skips hidden
//! files and unindexed volumes, so it only seeds the results the walk completes.
use std::{
    io::{BufRead, BufReader},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::atomic::{AtomicBool, Ordering},
    thread,
    time::Duration,
};

/// How often a running `mdfind` is checked for a search that has stopped.
const POLL: Duration = Duration::from_millis(30);

/// Wildcard queries keep to the walk: `?` has no Spotlight equivalent.
pub(crate) fn can_search(query: &str) -> bool {
    !query.is_empty() && !query.contains(['*', '?'])
}

/// `kMDItemFSName` contains `query`, ignoring case and diacritics (`cd`).
fn name_query(query: &str) -> String {
    let escaped = query.replace('\\', "\\\\").replace('"', "\\\"");
    format!("kMDItemFSName == \"*{escaped}*\"cd")
}

/// Hands each path Spotlight finds under `root` to `found` until it returns false, Spotlight
/// is done, or `halt` is set. A missing or failing `mdfind` simply finds nothing.
pub(crate) fn find_names(
    root: &Path,
    query: &str,
    halt: &AtomicBool,
    mut found: impl FnMut(PathBuf) -> bool + Send,
) {
    let Ok(mut child) = Command::new("/usr/bin/mdfind")
        .arg("-0")
        .arg("-onlyin")
        .arg(root)
        .arg(name_query(query))
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
    else {
        return;
    };
    let Some(stdout) = child.stdout.take() else {
        let _ = child.kill();
        let _ = child.wait();
        return;
    };
    let reading = AtomicBool::new(true);
    thread::scope(|scope| {
        scope.spawn(|| {
            let mut reader = BufReader::new(stdout);
            let mut path = Vec::new();
            while !halt.load(Ordering::Relaxed) {
                path.clear();
                match reader.read_until(0, &mut path) {
                    Ok(0) | Err(_) => break,
                    Ok(_) => {}
                }
                if path.last() == Some(&0) {
                    path.pop();
                }
                let Ok(text) = std::str::from_utf8(&path) else {
                    continue;
                };
                if !found(PathBuf::from(text)) {
                    break;
                }
            }
            reading.store(false, Ordering::Relaxed);
        });
        // The reader blocks on the pipe, so a stopped search kills `mdfind` to unblock it.
        while reading.load(Ordering::Relaxed) {
            if halt.load(Ordering::Relaxed) {
                let _ = child.kill();
                break;
            }
            thread::sleep(POLL);
        }
    });
    let _ = child.kill();
    let _ = child.wait();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quotes_and_backslashes_stay_inside_the_query() {
        assert_eq!(name_query(r#"a"b\c"#), r#"kMDItemFSName == "*a\"b\\c*"cd"#);
    }

    #[test]
    fn wildcards_are_left_to_the_walk() {
        assert!(can_search("invoice"));
        assert!(!can_search("*.pdf"));
        assert!(!can_search("a?c"));
        assert!(!can_search(""));
    }
}
