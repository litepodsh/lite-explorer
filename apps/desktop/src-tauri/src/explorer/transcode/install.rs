//! Downloads the pinned ffmpeg build, checks it against its SHA-256, and installs it.

use std::{
    fs,
    io::{self, Write},
    path::Path,
};

use sha2::{Digest, Sha256};

use super::{
    manifest::{total_size, Item},
    tools::{tools_in, Source},
};

pub const CANCELLED: &str = "Download cancelled";

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// Streams `url` into `dest` and checks its SHA-256. A file that fails any check is deleted.
/// `progress` gets the size of each chunk as it arrives.
pub async fn download_file(
    client: &reqwest::Client,
    url: &str,
    dest: &Path,
    expected_sha256: &str,
    progress: &mut impl FnMut(u64),
    cancelled: &impl Fn() -> bool,
) -> Result<(), String> {
    let mut response = client
        .get(url)
        .send()
        .await
        .map_err(|error| format!("Couldn’t reach the download server: {error}"))?;
    if !response.status().is_success() {
        return Err(format!("The download server answered {}", response.status()));
    }
    let mut file = fs::File::create(dest).map_err(|error| error.to_string())?;
    let mut hasher = Sha256::new();
    let result = async {
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|error| format!("The download was interrupted: {error}"))?
        {
            if cancelled() {
                return Err(CANCELLED.to_string());
            }
            hasher.update(&chunk);
            file.write_all(&chunk).map_err(|error| error.to_string())?;
            progress(chunk.len() as u64);
        }
        file.flush().map_err(|error| error.to_string())?;
        let actual = hex(&hasher.clone().finalize());
        if actual.eq_ignore_ascii_case(expected_sha256) {
            Ok(())
        } else {
            Err("The download didn’t match its checksum, so it was discarded.".to_string())
        }
    }
    .await;
    if result.is_err() {
        drop(file);
        let _ = fs::remove_file(dest);
    }
    result
}

/// Copies one file out of a zip, marking it executable.
pub fn extract_entry(zip_path: &Path, inside: &str, out: &Path) -> Result<(), String> {
    let file = fs::File::open(zip_path).map_err(|error| error.to_string())?;
    let mut archive = zip::ZipArchive::new(file).map_err(|_| "The download is not a valid archive".to_string())?;
    let mut entry = archive
        .by_name(inside)
        .map_err(|_| format!("The download is missing “{inside}”"))?;
    let mut output = fs::File::create(out).map_err(|error| error.to_string())?;
    io::copy(&mut entry, &mut output).map_err(|error| error.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(out, fs::Permissions::from_mode(0o755)).map_err(|error| error.to_string())?;
    }
    Ok(())
}

async fn install_into(
    staging: &Path,
    items: &[Item],
    client: &reqwest::Client,
    report: &mut impl FnMut(u64, u64),
    cancelled: &impl Fn() -> bool,
) -> Result<(), String> {
    let total = total_size(items);
    let mut done = 0u64;
    for (index, item) in items.iter().enumerate() {
        let archive = staging.join(format!("download-{index}.zip"));
        download_file(
            client,
            item.url,
            &archive,
            item.sha256,
            &mut |bytes| {
                done += bytes;
                report(done, total);
            },
            cancelled,
        )
        .await?;
        for entry in item.entries {
            extract_entry(&archive, entry.inside, &staging.join(entry.name))?;
        }
        let _ = fs::remove_file(&archive);
    }
    tools_in(staging, Source::Managed).ok_or_else(|| "The downloaded ffmpeg did not start".to_string())?;
    Ok(())
}

/// Installs every item into `dir`, replacing what was there. Nothing is left behind on failure.
pub async fn install(
    dir: &Path,
    items: &[Item],
    client: &reqwest::Client,
    report: &mut impl FnMut(u64, u64),
    cancelled: &impl Fn() -> bool,
) -> Result<(), String> {
    let name = dir
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .ok_or("Invalid install folder")?;
    let staging = dir.with_file_name(format!("{name}.partial"));
    let _ = fs::remove_dir_all(&staging);
    fs::create_dir_all(&staging).map_err(|error| error.to_string())?;
    if let Err(error) = install_into(&staging, items, client, report, cancelled).await {
        let _ = fs::remove_dir_all(&staging);
        return Err(error);
    }
    let _ = fs::remove_dir_all(dir);
    if let Err(error) = fs::rename(&staging, dir) {
        let _ = fs::remove_dir_all(&staging);
        return Err(error.to_string());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::explorer::transcode::manifest::Entry;
    use std::{
        cell::Cell,
        collections::HashMap,
        io::Read,
        net::TcpListener,
        sync::Arc,
        thread,
    };

    fn sha256(bytes: &[u8]) -> String {
        hex(&Sha256::digest(bytes))
    }

    /// Serves `routes` (path to body) on a local port until the test process ends.
    fn serve(routes: HashMap<String, Vec<u8>>) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let routes = Arc::new(routes);
        thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { continue };
                let routes = Arc::clone(&routes);
                thread::spawn(move || {
                    let mut request = Vec::new();
                    let mut buffer = [0u8; 1024];
                    while !request.windows(4).any(|window| window == b"\r\n\r\n") {
                        match stream.read(&mut buffer) {
                            Ok(0) | Err(_) => return,
                            Ok(read) => request.extend_from_slice(&buffer[..read]),
                        }
                    }
                    let text = String::from_utf8_lossy(&request).into_owned();
                    let path = text.split_whitespace().nth(1).unwrap_or("/").to_string();
                    let Some(body) = routes.get(&path) else {
                        let _ = stream.write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
                        return;
                    };
                    let head = format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len());
                    if stream.write_all(head.as_bytes()).is_err() {
                        return;
                    }
                    for piece in body.chunks(16 * 1024) {
                        if stream.write_all(piece).is_err() {
                            return;
                        }
                    }
                });
            }
        });
        format!("http://{address}")
    }

    fn run<F: std::future::Future>(future: F) -> F::Output {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(future)
    }

    fn zip_of(files: &[(&str, &str)]) -> Vec<u8> {
        let mut buffer = io::Cursor::new(Vec::new());
        let mut writer = zip::ZipWriter::new(&mut buffer);
        for (name, content) in files {
            writer
                .start_file(*name, zip::write::SimpleFileOptions::default())
                .unwrap();
            writer.write_all(content.as_bytes()).unwrap();
        }
        writer.finish().unwrap();
        buffer.into_inner()
    }

    #[test]
    fn downloads_and_checks_the_checksum() {
        let body = vec![7u8; 200_000];
        let base = serve(HashMap::from([("/file.bin".to_string(), body.clone())]));
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("file.bin");
        let received = Cell::new(0u64);
        run(download_file(
            &reqwest::Client::new(),
            &format!("{base}/file.bin"),
            &dest,
            &sha256(&body),
            &mut |bytes| received.set(received.get() + bytes),
            &|| false,
        ))
        .unwrap();
        assert_eq!(fs::read(&dest).unwrap(), body);
        assert_eq!(received.get(), body.len() as u64);
    }

    #[test]
    fn a_wrong_checksum_deletes_the_file() {
        let base = serve(HashMap::from([("/file.bin".to_string(), b"payload".to_vec())]));
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("file.bin");
        let error = run(download_file(
            &reqwest::Client::new(),
            &format!("{base}/file.bin"),
            &dest,
            &sha256(b"something else"),
            &mut |_| {},
            &|| false,
        ))
        .unwrap_err();
        assert!(error.contains("checksum"), "{error}");
        assert!(!dest.exists());
    }

    #[test]
    fn a_server_error_is_reported() {
        let base = serve(HashMap::new());
        let dir = tempfile::tempdir().unwrap();
        let error = run(download_file(
            &reqwest::Client::new(),
            &format!("{base}/missing"),
            &dir.path().join("x"),
            &sha256(b""),
            &mut |_| {},
            &|| false,
        ))
        .unwrap_err();
        assert!(error.contains("404"), "{error}");
    }

    #[test]
    fn cancelling_stops_the_download_and_removes_the_file() {
        let body = vec![1u8; 2_000_000];
        let base = serve(HashMap::from([("/big".to_string(), body.clone())]));
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("big");
        let chunks = Cell::new(0);
        let error = run(download_file(
            &reqwest::Client::new(),
            &format!("{base}/big"),
            &dest,
            &sha256(&body),
            &mut |_| chunks.set(chunks.get() + 1),
            &|| chunks.get() >= 1,
        ))
        .unwrap_err();
        assert_eq!(error, CANCELLED);
        assert!(!dest.exists());
    }

    #[test]
    fn unreachable_servers_say_so() {
        let dir = tempfile::tempdir().unwrap();
        let error = run(download_file(
            &reqwest::Client::new(),
            "http://127.0.0.1:1/nothing",
            &dir.path().join("x"),
            &sha256(b""),
            &mut |_| {},
            &|| false,
        ))
        .unwrap_err();
        assert!(error.starts_with("Couldn’t reach the download server"), "{error}");
    }

    #[test]
    fn extracts_a_file_from_a_zip() {
        let dir = tempfile::tempdir().unwrap();
        let archive = dir.path().join("a.zip");
        fs::write(&archive, zip_of(&[("bin/tool", "hello"), ("other", "x")])).unwrap();
        let out = dir.path().join("tool");
        extract_entry(&archive, "bin/tool", &out).unwrap();
        assert_eq!(fs::read_to_string(&out).unwrap(), "hello");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(fs::metadata(&out).unwrap().permissions().mode() & 0o111, 0o111);
        }
        let missing = extract_entry(&archive, "nope", &dir.path().join("n")).unwrap_err();
        assert_eq!(missing, "The download is missing “nope”");
        fs::write(&archive, b"not a zip").unwrap();
        assert_eq!(
            extract_entry(&archive, "bin/tool", &out).unwrap_err(),
            "The download is not a valid archive"
        );
    }

    #[cfg(unix)]
    fn leak<T>(value: T) -> &'static T {
        Box::leak(Box::new(value))
    }

    /// Items that download from the local server; each zip holds a fake tool.
    #[cfg(unix)]
    fn fake_items(base: &str, bodies: &HashMap<String, Vec<u8>>, corrupt_second: bool) -> Vec<Item> {
        ["ffmpeg", "ffprobe"]
            .iter()
            .enumerate()
            .map(|(index, name)| {
                let body = &bodies[&format!("/{name}.zip")];
                let hash = if corrupt_second && index == 1 { sha256(b"different") } else { sha256(body) };
                Item {
                    url: Box::leak(format!("{base}/{name}.zip").into_boxed_str()),
                    sha256: Box::leak(hash.into_boxed_str()),
                    size: body.len() as u64,
                    entries: leak(vec![Entry {
                        inside: Box::leak((*name).to_string().into_boxed_str()),
                        name: Box::leak((*name).to_string().into_boxed_str()),
                    }]),
                }
            })
            .collect()
    }

    #[cfg(unix)]
    fn fake_bodies() -> HashMap<String, Vec<u8>> {
        ["ffmpeg", "ffprobe"]
            .iter()
            .map(|name| {
                let script = format!("#!/bin/sh\necho \"{name} version 9.9.9 Copyright\"\n");
                (format!("/{name}.zip"), zip_of(&[(name, &script)]))
            })
            .collect()
    }

    #[cfg(unix)]
    #[test]
    fn installs_a_working_pair_and_replaces_an_old_one() {
        let bodies = fake_bodies();
        let base = serve(bodies.clone());
        let items = fake_items(&base, &bodies, false);
        let parent = tempfile::tempdir().unwrap();
        let dir = parent.path().join("9.9.9");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("old-file"), "left over").unwrap();
        let reports = Cell::new((0u64, 0u64));
        run(install(
            &dir,
            &items,
            &reqwest::Client::new(),
            &mut |done, total| reports.set((done, total)),
            &|| false,
        ))
        .unwrap();
        let tools = tools_in(&dir, Source::Managed).unwrap();
        assert_eq!(tools.version, "9.9.9");
        assert!(!dir.join("old-file").exists());
        assert!(!parent.path().join("9.9.9.partial").exists());
        let (done, total) = reports.get();
        assert_eq!(done, total_size(&items));
        assert_eq!(total, total_size(&items));
    }

    #[cfg(unix)]
    #[test]
    fn a_failed_install_leaves_nothing_behind() {
        let bodies = fake_bodies();
        let base = serve(bodies.clone());
        let items = fake_items(&base, &bodies, true);
        let parent = tempfile::tempdir().unwrap();
        let dir = parent.path().join("9.9.9");
        let error = run(install(&dir, &items, &reqwest::Client::new(), &mut |_, _| {}, &|| false)).unwrap_err();
        assert!(error.contains("checksum"), "{error}");
        assert!(!dir.exists());
        assert!(!parent.path().join("9.9.9.partial").exists());
    }

    #[cfg(unix)]
    #[test]
    fn a_failed_install_keeps_the_previous_one() {
        let bodies = fake_bodies();
        let base = serve(bodies.clone());
        let good = fake_items(&base, &bodies, false);
        let bad = fake_items(&base, &bodies, true);
        let parent = tempfile::tempdir().unwrap();
        let dir = parent.path().join("9.9.9");
        run(install(&dir, &good, &reqwest::Client::new(), &mut |_, _| {}, &|| false)).unwrap();
        assert!(run(install(&dir, &bad, &reqwest::Client::new(), &mut |_, _| {}, &|| false)).is_err());
        assert!(tools_in(&dir, Source::Managed).is_some());
    }

    /// Downloads the real ffmpeg for this system: `cargo test -- --ignored --nocapture real_download`.
    #[test]
    #[ignore]
    fn real_download_installs_a_working_ffmpeg() {
        let items = crate::explorer::transcode::manifest::items().expect("a build for this system");
        let parent = tempfile::tempdir().unwrap();
        let dir = parent.path().join(crate::explorer::transcode::manifest::VERSION);
        let started = std::time::Instant::now();
        let client = reqwest::Client::builder()
            .user_agent("lite-explorer-test")
            .build()
            .unwrap();
        run(install(&dir, items, &client, &mut |_, _| {}, &|| false)).unwrap();
        let tools = tools_in(&dir, Source::Managed).unwrap();
        println!("installed {} in {:?}", tools.version, started.elapsed());
        assert!(tools.version.starts_with(crate::explorer::transcode::manifest::VERSION));
    }
}
