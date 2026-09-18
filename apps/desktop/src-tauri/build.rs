// Build-time environment values that must be baked into the binary (not runtime env).
// `SENTRY_DSN` comes from the process env in CI (a GitHub secret) or from a local
// `.env` file during development.
fn main() {
    // tauri-build embeds icons/icon.ico in the Windows executable but only re-runs when
    // tauri.conf.json changes, so a new icon would otherwise keep the old one in cached builds.
    println!("cargo:rerun-if-changed=icons");
    emit_env("SENTRY_DSN");
    bundle_apple_intelligence();
    tauri_build::build()
}

/// Copies the Apple Intelligence plugin's prebuilt Swift dylib next to the app so the binary can
/// load it at runtime, and links it *weakly*.
///
/// The dylib hard-links `FoundationModels` and declares `minos 26.0`, so a strong link would make
/// dyld refuse to launch the whole app on macOS 15 and older. A weak link keeps the app launchable
/// there — the dylib simply resolves to nothing, and `ai::apple_intelligence_ready` refuses to
/// call into it before it has checked the OS version.
fn bundle_apple_intelligence() {
    // The plugin crate emits its link-search path for this target only, so anything else has
    // nothing to bundle and nothing to link.
    #[cfg(not(all(target_os = "macos", target_arch = "aarch64")))]
    return;

    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    {
        let Some(dylib) = find_prebuilt("libappleai.dylib") else {
            println!("cargo:warning=couldn't find libappleai.dylib in the cargo registry; Apple Intelligence name suggestions will be unavailable");
            return;
        };
        let resources = std::path::PathBuf::from("resources");
        let target = resources.join("libappleai.dylib");
        if std::fs::create_dir_all(&resources).is_err() || std::fs::copy(&dylib, &target).is_err() {
            println!("cargo:warning=couldn't copy libappleai.dylib into src-tauri/resources");
            return;
        }
        println!("cargo:rerun-if-changed={}", target.display());
        // Weak: a missing or too-new dylib must not stop the app from launching.
        println!("cargo:rustc-link-arg=-Wl,-weak-lappleai");
        // Bundled builds resolve it from the app's Resources directory.
        println!("cargo:rustc-link-arg=-Wl,-rpath,@executable_path/../Resources");
        // Dev builds: Tauri serves no bundle resources, so point at the copy in the source tree.
        let absolute = std::fs::canonicalize(&target);
        if let Ok(path) = absolute {
            if let Some(parent) = path.parent() {
                println!("cargo:rustc-link-arg=-Wl,-rpath,{}", parent.display());
            }
        }
    }
}

/// Locates a file shipped inside a crate's extracted sources (`$CARGO_HOME/registry/src/...`).
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
fn find_prebuilt(file_name: &str) -> Option<std::path::PathBuf> {
    let home = std::env::var_os("CARGO_HOME")
        .map(std::path::PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME").map(|home| std::path::PathBuf::from(home).join(".cargo"))
        })?;
    let sources = home.join("registry").join("src");
    let registries = std::fs::read_dir(sources).ok()?;
    for registry in registries.flatten() {
        let crates = std::fs::read_dir(registry.path()).ok()?;
        for entry in crates.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if !name.starts_with("tauri-plugin-apple-intelligence-") {
                continue;
            }
            let candidate = entry.path().join("prebuilt").join(file_name);
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
}

/// Exposes `key` to the crate through `option_env!`. The process env wins so CI secrets
/// override the local `.env`; a missing value just leaves the variable unset.
fn emit_env(key: &str) {
    println!("cargo:rerun-if-env-changed={key}");
    if std::env::var_os(key).is_some() {
        return;
    }
    if let Some(value) = read_env_file(key) {
        println!("cargo:rustc-env={key}={value}");
    }
}

fn read_env_file(key: &str) -> Option<String> {
    for path in [".env", "../.env"] {
        println!("cargo:rerun-if-changed={path}");
        let Ok(contents) = std::fs::read_to_string(path) else {
            continue;
        };
        if let Some(value) = parse_env(&contents, key) {
            return Some(value);
        }
    }
    None
}

fn parse_env(contents: &str, key: &str) -> Option<String> {
    for line in contents.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((name, value)) = line.split_once('=') else {
            continue;
        };
        if name.trim() != key {
            continue;
        }
        let value = value.trim().trim_matches('"').trim_matches('\'');
        return Some(value.to_owned());
    }
    None
}
