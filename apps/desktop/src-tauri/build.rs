// Build-time environment values that must be baked into the binary (not runtime env).
// `SENTRY_DSN` comes from the process env in CI (a GitHub secret) or from a local
// `.env` file during development.
fn main() {
    // tauri-build embeds icons/icon.ico in the Windows executable but only re-runs when
    // tauri.conf.json changes, so a new icon would otherwise keep the old one in cached builds.
    println!("cargo:rerun-if-changed=icons");
    emit_env("SENTRY_DSN");
    tauri_build::build()
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
