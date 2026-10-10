//! Reads `~/.ssh/config` so SFTP locations can use the same host aliases, users, ports and
//! keys as the `ssh` command. Only `Host` blocks are understood; `Match` blocks and
//! `Include` are skipped.

use std::path::{Path, PathBuf};

use serde::Serialize;

/// A concrete `Host` alias, offered in the add-location dialog.
#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SshHost {
    pub alias: String,
    pub host_name: Option<String>,
    pub user: Option<String>,
    pub port: Option<u16>,
}

/// Settings that apply to one host, merged like `ssh` does: the first value seen wins.
#[derive(Debug, Default, PartialEq)]
pub struct Resolved {
    pub host_name: Option<String>,
    pub user: Option<String>,
    pub port: Option<u16>,
    pub identity_files: Vec<PathBuf>,
}

struct Block {
    patterns: Vec<String>,
    options: Vec<(String, String)>,
}

fn home() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}

fn parse(text: &str) -> Vec<Block> {
    // Options before the first `Host` line apply to every host.
    let mut blocks = vec![Block {
        patterns: vec!["*".into()],
        options: Vec::new(),
    }];
    let mut skipping = false;
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let (key, value) = match line.split_once(|c: char| c.is_whitespace() || c == '=') {
            Some((key, value)) => (
                key,
                value
                    .trim_start_matches(|c: char| c.is_whitespace() || c == '=')
                    .trim(),
            ),
            None => continue,
        };
        let key = key.to_ascii_lowercase();
        match key.as_str() {
            "host" => {
                skipping = false;
                blocks.push(Block {
                    patterns: value
                        .split_whitespace()
                        .map(|p| p.trim_matches('"').to_string())
                        .collect(),
                    options: Vec::new(),
                });
            }
            "match" => skipping = true,
            _ if !skipping => {
                let value = value.trim_matches('"').to_string();
                blocks.last_mut().unwrap().options.push((key, value));
            }
            _ => {}
        }
    }
    blocks
}

/// `ssh_config` patterns: `*` and `?` wildcards, `!` negates.
fn glob(pattern: &str, text: &str) -> bool {
    fn matches(pattern: &[u8], text: &[u8]) -> bool {
        match (pattern.first(), text.first()) {
            (None, None) => true,
            (Some(b'*'), _) => {
                matches(&pattern[1..], text) || (!text.is_empty() && matches(pattern, &text[1..]))
            }
            (Some(b'?'), Some(_)) => matches(&pattern[1..], &text[1..]),
            (Some(p), Some(t)) if p.eq_ignore_ascii_case(t) => matches(&pattern[1..], &text[1..]),
            _ => false,
        }
    }
    matches(pattern.as_bytes(), text.as_bytes())
}

fn applies(patterns: &[String], host: &str) -> bool {
    let mut matched = false;
    for pattern in patterns {
        if let Some(negated) = pattern.strip_prefix('!') {
            if glob(negated, host) {
                return false;
            }
        } else if glob(pattern, host) {
            matched = true;
        }
    }
    matched
}

fn expand(path: &str, host: &str, home: Option<&Path>) -> PathBuf {
    let path = path.replace("%h", host).replace("%%", "%");
    match (path.strip_prefix("~/"), home) {
        (Some(rest), Some(home)) => home.join(rest),
        _ => PathBuf::from(path),
    }
}

fn resolve_in(blocks: &[Block], host: &str, home: Option<&Path>) -> Resolved {
    let mut resolved = Resolved::default();
    for block in blocks.iter().filter(|block| applies(&block.patterns, host)) {
        for (key, value) in &block.options {
            match key.as_str() {
                "hostname" if resolved.host_name.is_none() => {
                    resolved.host_name = Some(value.replace("%h", host))
                }
                "user" if resolved.user.is_none() => resolved.user = Some(value.clone()),
                "port" if resolved.port.is_none() => resolved.port = value.parse().ok(),
                // Unlike other options, every IdentityFile adds to the list.
                "identityfile" => resolved.identity_files.push(expand(value, host, home)),
                _ => {}
            }
        }
    }
    resolved
}

fn hosts_in(blocks: &[Block], home: Option<&Path>) -> Vec<SshHost> {
    let mut hosts: Vec<SshHost> = Vec::new();
    for block in blocks {
        for alias in &block.patterns {
            if alias.contains(['*', '?', '!']) || hosts.iter().any(|host| &host.alias == alias) {
                continue;
            }
            let resolved = resolve_in(blocks, alias, home);
            hosts.push(SshHost {
                alias: alias.clone(),
                host_name: resolved.host_name,
                user: resolved.user,
                port: resolved.port,
            });
        }
    }
    hosts
}

fn read_config() -> Vec<Block> {
    home()
        .and_then(|home| std::fs::read_to_string(home.join(".ssh/config")).ok())
        .map(|text| parse(&text))
        .unwrap_or_default()
}

/// Settings from `~/.ssh/config` for `host`, which may be an alias.
pub fn resolve(host: &str) -> Resolved {
    resolve_in(&read_config(), host, home().as_deref())
}

/// Keys `ssh` tries when the config names none.
pub fn default_identity_files() -> Vec<PathBuf> {
    let Some(home) = home() else {
        return Vec::new();
    };
    ["id_ed25519", "id_ecdsa", "id_rsa"]
        .iter()
        .map(|name| home.join(".ssh").join(name))
        .collect()
}

#[tauri::command]
pub async fn ssh_config_hosts() -> Vec<SshHost> {
    tauri::async_runtime::spawn_blocking(|| hosts_in(&read_config(), home().as_deref()))
        .await
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    const CONFIG: &str = r#"
IdentityFile ~/.ssh/global

Host pi raspberry
    HostName 192.168.1.20
    User pi
    Port 2222
    IdentityFile ~/.ssh/pi_ed25519

Host *.lan !printer.lan
    User admin

Match host something
    User ignored

Host *
    User fallback
    IdentityFile ~/.ssh/%h_key
"#;

    #[test]
    fn first_value_wins_and_identity_files_accumulate() {
        let blocks = parse(CONFIG);
        let home = Path::new("/home/me");
        let pi = resolve_in(&blocks, "pi", Some(home));
        assert_eq!(pi.host_name.as_deref(), Some("192.168.1.20"));
        assert_eq!(pi.user.as_deref(), Some("pi"));
        assert_eq!(pi.port, Some(2222));
        assert_eq!(
            pi.identity_files,
            [
                PathBuf::from("/home/me/.ssh/global"),
                PathBuf::from("/home/me/.ssh/pi_ed25519"),
                PathBuf::from("/home/me/.ssh/pi_key"),
            ]
        );
    }

    #[test]
    fn wildcards_negation_and_match_blocks() {
        let blocks = parse(CONFIG);
        assert_eq!(
            resolve_in(&blocks, "nas.lan", None).user.as_deref(),
            Some("admin")
        );
        assert_eq!(
            resolve_in(&blocks, "printer.lan", None).user.as_deref(),
            Some("fallback")
        );
        assert_eq!(
            resolve_in(&blocks, "something", None).user.as_deref(),
            Some("fallback")
        );
    }

    #[test]
    fn lists_only_concrete_aliases() {
        let hosts = hosts_in(&parse(CONFIG), None);
        let aliases: Vec<&str> = hosts.iter().map(|host| host.alias.as_str()).collect();
        assert_eq!(aliases, ["pi", "raspberry"]);
        assert_eq!(hosts[1].host_name.as_deref(), Some("192.168.1.20"));
    }

    #[test]
    fn accepts_equals_separators() {
        let blocks = parse("Host box\n  HostName=10.0.0.5\n  Port = 2200\n");
        let resolved = resolve_in(&blocks, "box", None);
        assert_eq!(resolved.host_name.as_deref(), Some("10.0.0.5"));
        assert_eq!(resolved.port, Some(2200));
    }
}
