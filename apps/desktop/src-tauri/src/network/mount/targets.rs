//! Platform-independent pieces of mounting: addresses each OS expects, parsers for tool
//! output and error mapping. Everything here is pure so it's tested on every platform.

use percent_encoding::{percent_decode_str, utf8_percent_encode, AsciiSet, CONTROLS};

use super::super::{ConnectError, ErrorKind, Protocol, Security};
use super::{Credentials, Target};

/// Characters escaped inside one path segment of a URL.
const SEGMENT: &AsciiSet = &CONTROLS
    .add(b' ')
    .add(b'"')
    .add(b'#')
    .add(b'%')
    .add(b'/')
    .add(b'<')
    .add(b'>')
    .add(b'?')
    .add(b'`')
    .add(b'{')
    .add(b'}');

/// User and domain names in a URL, where `@`, `:` and `;` separate parts.
const USER: &AsciiSet = &SEGMENT.add(b'@').add(b':').add(b';');

fn encode_path(path: &str) -> String {
    path.split('/')
        .filter(|segment| !segment.is_empty())
        .map(|segment| utf8_percent_encode(segment, SEGMENT).to_string())
        .collect::<Vec<_>>()
        .join("/")
}

fn decode(value: &str) -> String {
    percent_decode_str(value).decode_utf8_lossy().into_owned()
}

fn url_host(host: &str) -> String {
    if host.contains(':') {
        format!("[{host}]")
    } else {
        host.to_string()
    }
}

fn authority(target: &Target) -> String {
    match target.port {
        Some(port) => format!("{}:{port}", url_host(&target.host)),
        None => url_host(&target.host),
    }
}

/// SMB paths are `share/sub/folders`. Returns the share and the rest.
pub fn split_share(path: &str) -> (&str, &str) {
    let path = path.trim_matches('/');
    path.split_once('/').unwrap_or((path, ""))
}

/// URL handed to NetFS on macOS.
pub fn netfs_url(target: &Target) -> String {
    let path = encode_path(&target.path);
    let scheme = match (target.protocol, target.security) {
        (Protocol::Webdav, Some(Security::Http)) => "http",
        (Protocol::Webdav, _) => "https",
        (Protocol::Nfs, _) => "nfs",
        _ => "smb",
    };
    format!("{scheme}://{}/{path}", authority(target))
}

/// URI handed to `gio` on Linux.
pub fn gio_uri(target: &Target) -> String {
    let path = encode_path(&target.path);
    let scheme = match (target.protocol, target.security) {
        (Protocol::Webdav, Some(Security::Http)) => "dav",
        (Protocol::Webdav, _) => "davs",
        (Protocol::Nfs, _) => "nfs",
        _ => "smb",
    };
    format!("{scheme}://{}/{path}", authority(target))
}

fn backslashes(path: &str) -> String {
    path.split('/')
        .filter(|segment| !segment.is_empty())
        .collect::<Vec<_>>()
        .join("\\")
}

/// Windows UNC root that a connection is made to: `\\host\share` for SMB,
/// `\\host@SSL@port\DavWWWRoot` for WebDAV, `\\host\export\path` for NFS.
pub fn unc_root(target: &Target) -> Result<String, ConnectError> {
    match target.protocol {
        Protocol::Smb => {
            if target.port.is_some_and(|port| port != 445) {
                return Err(ConnectError::new(
                    ErrorKind::Unsupported,
                    "Windows connects to SMB shares only on port 445.",
                ));
            }
            let (share, _) = split_share(&target.path);
            if share.is_empty() {
                return Err(ConnectError::invalid("Enter a share name."));
            }
            Ok(format!("\\\\{}\\{share}", target.host))
        }
        Protocol::Webdav => {
            let https = target.security != Some(Security::Http);
            let default_port = if https { 443 } else { 80 };
            let mut host = target.host.clone();
            if https {
                host.push_str("@SSL");
            }
            if let Some(port) = target.port.filter(|port| *port != default_port) {
                host.push_str(&format!("@{port}"));
            }
            Ok(format!("\\\\{host}\\DavWWWRoot"))
        }
        Protocol::Nfs => Ok(format!(
            "\\\\{}\\{}",
            target.host,
            backslashes(&target.path)
        )),
        Protocol::Sftp | Protocol::Ftp => Err(ConnectError::new(
            ErrorKind::Unsupported,
            "SFTP and FTP servers aren't mounted by the system.",
        )),
    }
}

/// Windows path of `target.path` once connected.
pub fn unc_path(target: &Target) -> Result<String, ConnectError> {
    let root = unc_root(target)?;
    let rest = match target.protocol {
        Protocol::Smb => backslashes(split_share(&target.path).1),
        Protocol::Webdav => backslashes(&target.path),
        _ => String::new(),
    };
    Ok(if rest.is_empty() {
        root
    } else {
        format!("{root}\\{rest}")
    })
}

/// Answers written to `gio mount` prompts: user, domain (SMB only) and password.
pub fn gio_answers(protocol: Protocol, credentials: &Credentials) -> String {
    let Some(username) = credentials.username.as_deref() else {
        return String::new();
    };
    let password = credentials.password.as_deref().unwrap_or_default();
    let (domain, user) = username.split_once('\\').unwrap_or(("", username));
    match protocol {
        Protocol::Smb => format!("{user}\n{domain}\n{password}\n"),
        _ => format!("{username}\n{password}\n"),
    }
}

/// `local path:` line of `gio info`.
pub fn gio_local_path(output: &str) -> Option<String> {
    output.lines().find_map(|line| {
        line.trim()
            .strip_prefix("local path:")
            .map(|path| path.trim().to_string())
            .filter(|path| !path.is_empty())
    })
}

pub fn gio_already_mounted(stderr: &str) -> bool {
    stderr.to_lowercase().contains("already mounted")
}

pub fn gio_error(stderr: &str) -> ConnectError {
    let text = stderr.to_lowercase();
    let has = |needles: &[&str]| needles.iter().any(|needle| text.contains(needle));
    if has(&[
        "permission denied",
        "password",
        "login",
        "authentication",
        "access denied",
    ]) {
        auth_rejected()
    } else if has(&["timed out", "timeout"]) {
        ConnectError::new(ErrorKind::Timeout, "The server didn’t answer in time.")
    } else if has(&[
        "connection refused",
        "no route",
        "resolv",
        "unreachable",
        "network is down",
    ]) {
        ConnectError::new(ErrorKind::Unreachable, "Can’t reach the server.")
    } else if has(&[
        "no such file",
        "not found",
        "doesn’t exist",
        "doesn't exist",
        "does not exist",
    ]) {
        missing_share()
    } else if has(&["not supported"]) {
        ConnectError::new(
            ErrorKind::Unsupported,
            "This system can’t mount this kind of share. Install the GVfs backends (for example gvfs-backends, gvfs-smb or gvfs-nfs).",
        )
    } else {
        let message = stderr.trim();
        ConnectError::new(
            ErrorKind::Other,
            if message.is_empty() {
                "gio couldn’t mount the share."
            } else {
                message
            },
        )
    }
}

fn auth_rejected() -> ConnectError {
    ConnectError::new(
        ErrorKind::Auth,
        "The server rejected the username or password.",
    )
}

fn missing_share() -> ConnectError {
    ConnectError::new(
        ErrorKind::NotFound,
        "The share or folder doesn’t exist on the server.",
    )
}

/// Status returned by `NetFSMountURLSync`: errno values, or negative OSStatus codes.
pub fn netfs_error(status: i32) -> ConnectError {
    match status {
        1 | 13 | 80 | -5045 | -5999 => auth_rejected(),
        -6004 => ConnectError::new(ErrorKind::Auth, "This server doesn’t allow guest access."),
        2 => missing_share(),
        -5998 | -6003 => ConnectError::new(
            ErrorKind::NotFound,
            "The server has no shares this account can open.",
        ),
        60 => ConnectError::new(ErrorKind::Timeout, "The server didn’t answer in time."),
        51 | 61 | 64 | 65 => ConnectError::new(ErrorKind::Unreachable, "Can’t reach the server."),
        -128 => ConnectError::new(ErrorKind::Other, "Connecting was canceled."),
        -5996 | -5997 => ConnectError::new(
            ErrorKind::Unsupported,
            "The server uses a protocol version or sign-in method macOS doesn’t support.",
        ),
        status => ConnectError::new(
            ErrorKind::Other,
            format!("macOS couldn’t mount the share (error {status})."),
        ),
    }
}

/// Win32 error returned by `WNetAddConnection2W`.
pub fn wnet_error(code: u32) -> ConnectError {
    match code {
        5 | 86 | 1326 | 1327 | 1330 | 1331 | 1907 | 1909 => auth_rejected(),
        1272 => ConnectError::new(
            ErrorKind::Auth,
            "Windows blocks guest access to shared folders. Connect as a registered user.",
        ),
        67 => missing_share(),
        53 | 1231 => ConnectError::new(ErrorKind::Unreachable, "Can’t reach the server."),
        121 => ConnectError::new(ErrorKind::Timeout, "The server didn’t answer in time."),
        1203 | 1222 => ConnectError::new(
            ErrorKind::Unsupported,
            "Windows couldn’t open this address. For WebDAV, check that the WebClient service is running.",
        ),
        1219 => ConnectError::new(
            ErrorKind::Other,
            "Windows is already connected to this server with other credentials. Disconnect that connection first.",
        ),
        code => ConnectError::new(
            ErrorKind::Other,
            format!("Windows couldn’t connect (error {code})."),
        ),
    }
}

/// Drive letter from `mount.exe` output such as `Z: is now successfully connected to \\host\path`.
pub fn windows_nfs_drive(output: &str) -> Option<String> {
    output.lines().find_map(|line| {
        let line = line.trim();
        let drive = line.get(..2)?;
        (line.contains("successfully connected")
            && drive.as_bytes()[0].is_ascii_alphabetic()
            && drive.ends_with(':'))
        .then(|| format!("{drive}\\"))
    })
}

/// Drive of an NFS mount already listed by `mount.exe` without arguments.
pub fn windows_nfs_listed(output: &str, remote: &str) -> Option<String> {
    output.lines().find_map(|line| {
        let mut columns = line.split_whitespace();
        let drive = columns.next()?;
        let listed = columns.next()?;
        (drive.len() == 2 && drive.ends_with(':') && listed.eq_ignore_ascii_case(remote))
            .then(|| format!("{drive}\\"))
    })
}

fn starts_with_path<'a>(path: &'a str, prefix: &str) -> Option<&'a str> {
    let path = path.trim_matches('/');
    let prefix = prefix.trim_matches('/');
    if prefix.is_empty() {
        return Some(path);
    }
    let head = path.get(..prefix.len())?;
    let rest = &path[prefix.len()..];
    (head.eq_ignore_ascii_case(prefix) && (rest.is_empty() || rest.starts_with('/')))
        .then(|| rest.trim_start_matches('/'))
}

/// Matches a macOS mount table entry against a target. Returns the part of `target.path`
/// below the mounted folder, or `None` when the entry is a different share.
pub fn match_mount(fstype: &str, from: &str, target: &Target) -> Option<String> {
    let (host, mounted) = match (fstype, target.protocol) {
        ("smbfs", Protocol::Smb) => {
            let rest = from.strip_prefix("//")?;
            let (authority, path) = rest.split_once('/').unwrap_or((rest, ""));
            let host = authority.rsplit('@').next()?;
            (host.split(':').next()?.to_string(), decode(path))
        }
        ("nfs", Protocol::Nfs) => {
            let (host, path) = from.split_once(":/")?;
            (host.trim_matches(['[', ']']).to_string(), decode(path))
        }
        ("webdav", Protocol::Webdav) => {
            let rest = from.split_once("://")?.1;
            let (authority, path) = rest.split_once('/').unwrap_or((rest, ""));
            let host = authority.rsplit('@').next()?;
            let host = if let Some(bracketed) = host.strip_prefix('[') {
                bracketed.split(']').next()?
            } else {
                host.split(':').next()?
            };
            (host.to_string(), decode(path))
        }
        _ => return None,
    };
    if !host.eq_ignore_ascii_case(&target.host) {
        return None;
    }
    starts_with_path(&target.path, &mounted).map(str::to_string)
}

/// Server address for `smbutil view`: `//user@host:port`. A `DOMAIN\\user` name becomes
/// `DOMAIN;user`. Guests have no user.
pub fn smbutil_address(target: &Target, username: Option<&str>) -> String {
    let user = username
        .map(|name| {
            let (domain, user) = name.split_once('\\').unwrap_or(("", name));
            let user = utf8_percent_encode(user, USER).to_string();
            if domain.is_empty() {
                format!("{user}@")
            } else {
                format!("{};{user}@", utf8_percent_encode(domain, USER))
            }
        })
        .unwrap_or_default();
    format!("//{user}{}", authority(target))
}

/// Disk shares in `smbutil view` output, without hidden administrative shares like `C$`.
pub fn smbutil_shares(output: &str) -> Vec<String> {
    let mut lines = output.lines();
    let Some(type_column) = lines.find_map(|line| {
        line.starts_with("Share")
            .then(|| line.find("Type"))
            .flatten()
    }) else {
        return Vec::new();
    };
    lines
        .filter(|line| !line.starts_with('-') && line.len() > type_column)
        .filter_map(|line| {
            let (name, rest) = line.split_at_checked(type_column)?;
            let name = name.trim_end();
            (rest.starts_with("Disk") && !name.is_empty() && !name.ends_with('$'))
                .then(|| name.to_string())
        })
        .collect()
}

/// Maps a failed `smbutil view` to a connection error.
pub fn smbutil_error(stderr: &str) -> ConnectError {
    let lower = stderr.to_lowercase();
    if lower.contains("authentication") || lower.contains("permission denied") {
        auth_rejected()
    } else if lower.contains("timed out") {
        ConnectError::new(ErrorKind::Timeout, "The server didn’t answer in time.")
    } else if lower.contains("no route")
        || lower.contains("refused")
        || lower.contains("unreachable")
        || lower.contains("not found")
        || lower.contains("resolve")
    {
        ConnectError::new(ErrorKind::Unreachable, "Can’t reach the server.")
    } else {
        let message = stderr.trim().trim_start_matches("smbutil: ");
        ConnectError::other(if message.is_empty() {
            "Couldn’t list the server’s shares.".to_string()
        } else {
            format!("Couldn’t list the server’s shares: {message}.")
        })
    }
}

/// Share name of a macOS `smbfs` mount table entry (`//user@host/share`) on `host`.
pub fn smb_mount_share(fstype: &str, from: &str, host: &str) -> Option<String> {
    if fstype != "smbfs" {
        return None;
    }
    let rest = from.strip_prefix("//")?;
    let (authority, path) = rest.split_once('/').unwrap_or((rest, ""));
    let mounted = authority.rsplit('@').next()?.split(':').next()?;
    if !mounted.eq_ignore_ascii_case(host) {
        return None;
    }
    let share = decode(split_share(path).0);
    (!share.is_empty()).then_some(share)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn smbutil_view_output() {
        let output = "Share                                           Type    Comments\n-------------------------------\nADMIN$                                          Disk    Remote Admin\nsmbtest                                         Disk    \nIPC$                                            Pipe    Remote IPC\nMy Photos                                       Disk    Family\nUsers                                           Disk    \n\n5 shares listed\n";
        assert_eq!(smbutil_shares(output), ["smbtest", "My Photos", "Users"]);
        assert!(smbutil_shares("").is_empty());

        let smb = target(Protocol::Smb, "");
        assert_eq!(smbutil_address(&smb, None), "//nas.local");
        assert_eq!(smbutil_address(&smb, Some("sebas")), "//sebas@nas.local");
        assert_eq!(
            smbutil_address(&smb, Some("me@mail.com")),
            "//me%40mail.com@nas.local"
        );
        assert_eq!(
            smbutil_address(&smb, Some("WORK\\Ana María")),
            "//WORK;Ana%20Mar%C3%ADa@nas.local"
        );
        assert_eq!(
            smbutil_error("smbutil: server rejected the authentication: Authentication error").kind,
            ErrorKind::Auth
        );
        assert_eq!(
            smbutil_error("smbutil: Operation timed out").kind,
            ErrorKind::Timeout
        );
    }

    #[test]
    fn shares_mounted_from_a_server() {
        assert_eq!(
            smb_mount_share("smbfs", "//me@192.168.1.65/smbtest", "192.168.1.65").as_deref(),
            Some("smbtest")
        );
        assert_eq!(
            smb_mount_share("smbfs", "//GUEST:@NAS.local/My%20Photos", "nas.local").as_deref(),
            Some("My Photos")
        );
        assert_eq!(
            smb_mount_share("smbfs", "//nas.local/Photos", "other"),
            None
        );
        assert_eq!(
            smb_mount_share("nfs", "//nas.local/Photos", "nas.local"),
            None
        );
        assert_eq!(smb_mount_share("smbfs", "//nas.local", "nas.local"), None);
    }

    fn target(protocol: Protocol, path: &str) -> Target {
        Target {
            protocol,
            host: "nas.local".into(),
            port: None,
            path: path.into(),
            security: match protocol {
                Protocol::Webdav => Some(Security::Https),
                _ => None,
            },
        }
    }

    #[test]
    fn netfs_and_gio_addresses() {
        let smb = target(Protocol::Smb, "My Photos/2026");
        assert_eq!(netfs_url(&smb), "smb://nas.local/My%20Photos/2026");
        assert_eq!(gio_uri(&smb), "smb://nas.local/My%20Photos/2026");
        assert_eq!(netfs_url(&target(Protocol::Smb, "")), "smb://nas.local/");

        let mut nfs = target(Protocol::Nfs, "/srv/media");
        nfs.port = Some(2049);
        assert_eq!(netfs_url(&nfs), "nfs://nas.local:2049/srv/media");

        let mut dav = target(Protocol::Webdav, "/remote.php/dav");
        assert_eq!(netfs_url(&dav), "https://nas.local/remote.php/dav");
        assert_eq!(gio_uri(&dav), "davs://nas.local/remote.php/dav");
        dav.security = Some(Security::Http);
        dav.host = "fe80::1".into();
        assert_eq!(netfs_url(&dav), "http://[fe80::1]/remote.php/dav");
        assert_eq!(gio_uri(&dav), "dav://[fe80::1]/remote.php/dav");
    }

    #[test]
    fn unc_paths_for_windows() {
        let smb = target(Protocol::Smb, "Photos/2026/May");
        assert_eq!(unc_root(&smb).unwrap(), "\\\\nas.local\\Photos");
        assert_eq!(unc_path(&smb).unwrap(), "\\\\nas.local\\Photos\\2026\\May");
        assert_eq!(
            unc_root(&target(Protocol::Smb, "")).unwrap_err().kind,
            ErrorKind::Invalid
        );
        let mut odd_port = smb.clone();
        odd_port.port = Some(1445);
        assert_eq!(
            unc_root(&odd_port).unwrap_err().kind,
            ErrorKind::Unsupported
        );

        let mut dav = target(Protocol::Webdav, "/files/me");
        assert_eq!(unc_root(&dav).unwrap(), "\\\\nas.local@SSL\\DavWWWRoot");
        assert_eq!(
            unc_path(&dav).unwrap(),
            "\\\\nas.local@SSL\\DavWWWRoot\\files\\me"
        );
        dav.port = Some(5006);
        assert_eq!(
            unc_root(&dav).unwrap(),
            "\\\\nas.local@SSL@5006\\DavWWWRoot"
        );
        dav.security = Some(Security::Http);
        dav.port = Some(80);
        assert_eq!(unc_root(&dav).unwrap(), "\\\\nas.local\\DavWWWRoot");

        let nfs = target(Protocol::Nfs, "/srv/media");
        assert_eq!(unc_path(&nfs).unwrap(), "\\\\nas.local\\srv\\media");
    }

    #[test]
    fn gio_prompt_answers() {
        let guest = Credentials::default();
        assert_eq!(gio_answers(Protocol::Smb, &guest), "");
        let user = Credentials {
            username: Some("OFFICE\\sebas".into()),
            password: Some("pw".into()),
        };
        assert_eq!(gio_answers(Protocol::Smb, &user), "sebas\nOFFICE\npw\n");
        let plain = Credentials {
            username: Some("sebas".into()),
            password: None,
        };
        assert_eq!(gio_answers(Protocol::Smb, &plain), "sebas\n\n\n");
        assert_eq!(gio_answers(Protocol::Webdav, &plain), "sebas\n\n");
    }

    #[test]
    fn gio_output_parsing() {
        let info = "display name: Photos on nas\nlocal path: /run/user/1000/gvfs/smb-share:server=nas,share=photos/2026\nuri: smb://nas/photos/2026\n";
        assert_eq!(
            gio_local_path(info).as_deref(),
            Some("/run/user/1000/gvfs/smb-share:server=nas,share=photos/2026")
        );
        assert_eq!(gio_local_path("uri: smb://nas/\n"), None);
        assert!(gio_already_mounted(
            "gio: smb://nas/photos: Location is already mounted"
        ));
        assert_eq!(
            gio_error("gio: Failed to mount: Permission denied").kind,
            ErrorKind::Auth
        );
        assert_eq!(
            gio_error("Error resolving “nas”: Name or service not known").kind,
            ErrorKind::Unreachable
        );
        assert_eq!(gio_error("Connection timed out").kind, ErrorKind::Timeout);
        assert_eq!(
            gio_error("The specified location is not supported").kind,
            ErrorKind::Unsupported
        );
        assert_eq!(
            gio_error("No such file or directory").kind,
            ErrorKind::NotFound
        );
        assert_eq!(gio_error("weird").message, "weird");
    }

    #[test]
    fn platform_error_codes() {
        assert_eq!(netfs_error(80).kind, ErrorKind::Auth);
        assert_eq!(netfs_error(2).kind, ErrorKind::NotFound);
        assert_eq!(netfs_error(65).kind, ErrorKind::Unreachable);
        assert_eq!(
            netfs_error(-6004).message,
            "This server doesn’t allow guest access."
        );
        assert_eq!(
            netfs_error(99).message,
            "macOS couldn’t mount the share (error 99)."
        );
        assert_eq!(wnet_error(1326).kind, ErrorKind::Auth);
        assert_eq!(wnet_error(67).kind, ErrorKind::NotFound);
        assert_eq!(wnet_error(53).kind, ErrorKind::Unreachable);
        assert_eq!(wnet_error(1203).kind, ErrorKind::Unsupported);
        assert_eq!(wnet_error(4).message, "Windows couldn’t connect (error 4).");
    }

    #[test]
    fn windows_nfs_output() {
        let mounted = "Z: is now successfully connected to \\\\nas\\srv\\media\r\n\r\nThe command completed successfully.";
        assert_eq!(windows_nfs_drive(mounted).as_deref(), Some("Z:\\"));
        assert_eq!(windows_nfs_drive("Network Error - 53"), None);
        let listing = "Local    Remote                                 Properties\r\n-------------------------------------------------------------------------------\r\nY:       \\\\NAS\\srv\\media                        UID=-2, GID=-2\r\n";
        assert_eq!(
            windows_nfs_listed(listing, "\\\\nas\\srv\\media").as_deref(),
            Some("Y:\\")
        );
        assert_eq!(windows_nfs_listed(listing, "\\\\nas\\other"), None);
    }

    #[test]
    fn mac_mount_table_matching() {
        let smb = target(Protocol::Smb, "Photos/2026");
        assert_eq!(
            match_mount("smbfs", "//sebas@NAS.local/photos", &smb).as_deref(),
            Some("2026")
        );
        assert_eq!(
            match_mount("smbfs", "//GUEST:@nas.local/Photos/2026", &smb).as_deref(),
            Some("")
        );
        assert_eq!(match_mount("smbfs", "//nas.local/Photos%20Old", &smb), None);
        assert_eq!(match_mount("smbfs", "//other/Photos", &smb), None);
        assert_eq!(match_mount("smbfs", "//nas.local/Pho", &smb), None);
        assert_eq!(
            match_mount(
                "smbfs",
                "//nas.local/My%20Photos",
                &target(Protocol::Smb, "My Photos")
            )
            .as_deref(),
            Some("")
        );
        assert_eq!(match_mount("nfs", "//nas.local/Photos", &smb), None);

        let nfs = target(Protocol::Nfs, "/srv/media/movies");
        assert_eq!(
            match_mount("nfs", "nas.local:/srv/media", &nfs).as_deref(),
            Some("movies")
        );

        let dav = target(Protocol::Webdav, "/remote.php/dav/files/me");
        assert_eq!(
            match_mount("webdav", "https://nas.local:443/remote.php/dav/", &dav).as_deref(),
            Some("files/me")
        );
        assert_eq!(
            match_mount("webdav", "https://other/remote.php/dav/", &dav),
            None
        );
    }
}
