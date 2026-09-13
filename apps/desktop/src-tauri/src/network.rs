//! Network locations: SMB, NFS and WebDAV shares, SFTP and FTP servers.
//!
//! Settings live in the `network_locations` table. A remembered password lives in the OS
//! credential store under `lite-explorer.network` and never reaches the frontend.
//! Paths use `<protocol>://<location-id>/<path>`. SMB, NFS and WebDAV are mounted by the
//! operating system (see `mount`) and browsed through their local folder. SFTP and FTP
//! connections arrive in a later phase; for them Test only checks that the server answers.

use std::{collections::HashMap, path::PathBuf, sync::Mutex, time::Duration};

use serde::{de::DeserializeOwned, Deserialize, Serialize};
use sqlx::{sqlite::SqliteRow, Row, SqlitePool};

use tauri::State;

use super::{now_secs, Database, Location};
use crate::remote::{delete_secret, read_optional_secret, store_secret};

pub mod discovery;
mod mount;
pub mod servers;

const KEYCHAIN_SERVICE: &str = "lite-explorer.network";
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Deserialize, Serialize, Clone, Copy, Debug, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Protocol {
    Smb,
    Nfs,
    Webdav,
    Sftp,
    Ftp,
}

#[derive(Deserialize, Serialize, Clone, Copy, Debug, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Auth {
    Guest,
    Password,
}

/// `http`/`https` for WebDAV, `explicit`/`implicit`/`plain` for FTP.
#[derive(Deserialize, Serialize, Clone, Copy, Debug, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Security {
    Http,
    Https,
    Explicit,
    Implicit,
    Plain,
}

/// Form input from the add-location dialog, and saved settings sent back for editing
/// (with an empty password).
#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct NetworkLocationInput {
    protocol: Protocol,
    #[serde(default)]
    name: String,
    #[serde(default)]
    host: String,
    #[serde(default)]
    port: Option<u16>,
    #[serde(default)]
    path: String,
    auth: Auth,
    #[serde(default)]
    username: String,
    #[serde(default)]
    password: String,
    #[serde(default)]
    security: Option<Security>,
    #[serde(default)]
    remember_password: bool,
}

/// Validated, normalized settings, as stored in `network_locations`.
#[derive(Debug, Clone, PartialEq)]
struct Settings {
    protocol: Protocol,
    name: String,
    host: String,
    port: Option<u16>,
    path: String,
    auth: Auth,
    username: Option<String>,
    security: Option<Security>,
    remember_password: bool,
}

impl Settings {
    fn port(&self) -> u16 {
        self.port.unwrap_or(match (self.protocol, self.security) {
            (Protocol::Smb, _) => 445,
            (Protocol::Nfs, _) => 2049,
            (Protocol::Webdav, Some(Security::Http)) => 80,
            (Protocol::Webdav, _) => 443,
            (Protocol::Sftp, _) => 22,
            (Protocol::Ftp, Some(Security::Implicit)) => 990,
            (Protocol::Ftp, _) => 21,
        })
    }

    fn input(&self) -> NetworkLocationInput {
        NetworkLocationInput {
            protocol: self.protocol,
            name: self.name.clone(),
            host: self.host.clone(),
            port: self.port,
            path: self.path.clone(),
            auth: self.auth,
            username: self.username.clone().unwrap_or_default(),
            password: String::new(),
            security: self.security,
            remember_password: self.remember_password,
        }
    }

    fn target(&self) -> mount::Target {
        mount::Target {
            protocol: self.protocol,
            host: self.host.clone(),
            port: self.port,
            path: self.path.clone(),
            security: self.security,
        }
    }
}

/// Local folders of shares this app mounted, and connected SMB servers, by location id.
#[derive(Default)]
pub struct Mounts {
    folders: Mutex<HashMap<String, PathBuf>>,
    /// SMB locations without a share that are connected, with the password typed for them.
    servers: Mutex<HashMap<String, Option<String>>>,
}

impl Mounts {
    fn get(&self, id: &str) -> Option<PathBuf> {
        self.folders.lock().unwrap().get(id).cloned()
    }

    fn insert(&self, id: &str, local: PathBuf) {
        self.folders.lock().unwrap().insert(id.to_string(), local);
    }

    fn remove(&self, id: &str) -> Option<PathBuf> {
        self.servers.lock().unwrap().remove(id);
        self.folders.lock().unwrap().remove(id)
    }

    fn connect_server(&self, id: &str, typed: Option<String>) {
        let mut servers = self.servers.lock().unwrap();
        let entry = servers.entry(id.to_string()).or_default();
        if typed.is_some() {
            *entry = typed;
        }
    }

    fn is_server_connected(&self, id: &str) -> bool {
        self.servers.lock().unwrap().contains_key(id)
    }

    fn server_password(&self, id: &str) -> Option<String> {
        self.servers.lock().unwrap().get(id).cloned().flatten()
    }
}

/// A saved location and the local folder it's mounted at.
#[derive(Serialize, Debug)]
pub struct Connection {
    location: String,
    path: String,
}

#[derive(Serialize, Clone, Copy, Debug, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ErrorKind {
    Auth,
    Invalid,
    NotFound,
    Unreachable,
    Timeout,
    Unsupported,
    HostKeyUnknown,
    HostKeyChanged,
    CertificateUntrusted,
    Other,
}

/// Error returned by network commands. The frontend maps `kind` to copy and UI.
#[derive(Serialize, Debug, PartialEq)]
pub struct ConnectError {
    kind: ErrorKind,
    message: String,
    /// Key or certificate to confirm before connecting again.
    #[serde(skip_serializing_if = "Option::is_none")]
    trust: Option<servers::TrustRequest>,
}

impl ConnectError {
    fn new(kind: ErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
            trust: None,
        }
    }

    fn invalid(message: impl Into<String>) -> Self {
        Self::new(ErrorKind::Invalid, message)
    }

    fn other(error: impl ToString) -> Self {
        Self::new(ErrorKind::Other, error.to_string())
    }
}

impl From<sqlx::Error> for ConnectError {
    fn from(error: sqlx::Error) -> Self {
        Self::other(error)
    }
}

#[derive(Serialize, Debug)]
pub struct ConnectionCheck {
    message: String,
}

#[derive(Debug, PartialEq)]
pub struct NetworkPath {
    pub protocol: Protocol,
    pub id: String,
    pub path: String,
}

/// Serde's lowercase name of a unit enum, for the text columns.
fn to_db<T: Serialize>(value: T) -> String {
    serde_json::to_value(value)
        .ok()
        .and_then(|value| value.as_str().map(str::to_string))
        .unwrap_or_default()
}

fn from_db<T: DeserializeOwned>(value: &str) -> Option<T> {
    serde_json::from_value(serde_json::Value::String(value.to_string())).ok()
}

fn non_empty(value: &str) -> Option<String> {
    let value = value.trim();
    (!value.is_empty()).then(|| value.to_string())
}

/// SMB paths start with the share name; every other protocol uses an absolute path.
fn normalize_path(protocol: Protocol, path: &str) -> String {
    let path = path.trim().replace('\\', "/");
    let trimmed = path.trim_matches('/');
    match (trimmed.is_empty(), protocol) {
        (true, _) => String::new(),
        (false, Protocol::Smb) => trimmed.to_string(),
        (false, _) => format!("/{trimmed}"),
    }
}

fn resolve(input: &NetworkLocationInput) -> Result<Settings, ConnectError> {
    let host = input.host.trim();
    if host.is_empty() {
        return Err(ConnectError::invalid("Enter a server name or IP address."));
    }
    if host.contains("://") || host.contains(['/', '\\']) || host.contains(char::is_whitespace) {
        return Err(ConnectError::invalid(
            "Server should be a name or IP address, without :// or a folder.",
        ));
    }
    if input.port == Some(0) {
        return Err(ConnectError::invalid("Port must be between 1 and 65535."));
    }

    let protocol = input.protocol;
    let path = normalize_path(protocol, &input.path);
    if protocol == Protocol::Nfs && path.is_empty() {
        return Err(ConnectError::invalid(
            "NFS needs the export path, like /srv/exports/media.",
        ));
    }

    let auth = match protocol {
        Protocol::Nfs => Auth::Guest,
        Protocol::Sftp => Auth::Password,
        _ => input.auth,
    };
    let username = match auth {
        Auth::Guest => None,
        Auth::Password => Some(
            non_empty(&input.username)
                .ok_or_else(|| ConnectError::invalid("Enter your username."))?,
        ),
    };

    let security = match (protocol, input.security) {
        (Protocol::Webdav, None) => Some(Security::Https),
        (Protocol::Webdav, Some(value @ (Security::Http | Security::Https))) => Some(value),
        (Protocol::Webdav, Some(_)) => {
            return Err(ConnectError::invalid(
                "WebDAV security must be http or https.",
            ))
        }
        (Protocol::Ftp, None) => Some(Security::Explicit),
        (
            Protocol::Ftp,
            Some(value @ (Security::Explicit | Security::Implicit | Security::Plain)),
        ) => Some(value),
        (Protocol::Ftp, Some(_)) => {
            return Err(ConnectError::invalid(
                "FTP security must be explicit, implicit or plain.",
            ))
        }
        _ => None,
    };

    let name = non_empty(&input.name)
        .or_else(|| {
            path.rsplit('/')
                .find(|segment| !segment.is_empty())
                .map(str::to_string)
        })
        .unwrap_or_else(|| host.to_string());

    Ok(Settings {
        protocol,
        name,
        host: host.to_string(),
        port: input.port,
        path,
        auth,
        username,
        security,
        remember_password: auth == Auth::Password && input.remember_password,
    })
}

fn network_path(protocol: Protocol, id: &str, path: &str) -> String {
    format!(
        "{}://{id}/{}",
        to_db(protocol),
        path.trim_start_matches('/')
    )
}

pub fn parse_network_path(path: &str) -> Option<NetworkPath> {
    let (scheme, rest) = path.split_once("://")?;
    let protocol = from_db::<Protocol>(scheme)?;
    let (id, remainder) = rest.split_once('/').unwrap_or((rest, ""));
    if id.is_empty() {
        return None;
    }
    Some(NetworkPath {
        protocol,
        id: id.to_string(),
        path: remainder.to_string(),
    })
}

pub fn is_network_path(path: &str) -> bool {
    parse_network_path(path).is_some()
}

fn location(id: &str, settings: &Settings) -> Location {
    Location {
        name: settings.name.clone(),
        path: network_path(settings.protocol, id, &settings.path),
        kind: to_db(settings.protocol),
    }
}

/// Phase 1 test: the server accepts a TCP connection on its port.
async fn check_reachable(settings: &Settings) -> Result<ConnectionCheck, ConnectError> {
    let (host, port) = (settings.host.as_str(), settings.port());
    let attempt = tokio::net::TcpStream::connect((host, port));
    match tokio::time::timeout(CONNECT_TIMEOUT, attempt).await {
        Ok(Ok(_)) => Ok(ConnectionCheck {
            message: format!("{host} answered on port {port}."),
        }),
        Ok(Err(error)) if error.kind() == std::io::ErrorKind::ConnectionRefused => {
            Err(ConnectError::new(
                ErrorKind::Unreachable,
                format!("{host} refused connections on port {port}."),
            ))
        }
        Ok(Err(error)) => Err(ConnectError::new(
            ErrorKind::Unreachable,
            format!("Can’t reach {host}: {error}."),
        )),
        Err(_) => Err(ConnectError::new(
            ErrorKind::Timeout,
            format!("No answer from {host} on port {port} after 5 seconds."),
        )),
    }
}

async fn insert_location(
    pool: &SqlitePool,
    id: &str,
    settings: &Settings,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO network_locations (id, name, protocol, host, port, path, username, auth, security, remember_password, position, created_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, (SELECT COALESCE(MAX(position), -1) + 1 FROM network_locations), ?)",
    )
    .bind(id)
    .bind(&settings.name)
    .bind(to_db(settings.protocol))
    .bind(&settings.host)
    .bind(settings.port.map(i64::from))
    .bind(&settings.path)
    .bind(&settings.username)
    .bind(to_db(settings.auth))
    .bind(settings.security.map(to_db))
    .bind(settings.remember_password)
    .bind(now_secs())
    .execute(pool)
    .await?;
    Ok(())
}

/// Returns false when no row has this id.
async fn update_location(
    pool: &SqlitePool,
    id: &str,
    settings: &Settings,
) -> Result<bool, sqlx::Error> {
    let result = sqlx::query(
        "UPDATE network_locations
         SET name = ?, protocol = ?, host = ?, port = ?, path = ?, username = ?, auth = ?, security = ?, remember_password = ?
         WHERE id = ?",
    )
    .bind(&settings.name)
    .bind(to_db(settings.protocol))
    .bind(&settings.host)
    .bind(settings.port.map(i64::from))
    .bind(&settings.path)
    .bind(&settings.username)
    .bind(to_db(settings.auth))
    .bind(settings.security.map(to_db))
    .bind(settings.remember_password)
    .bind(id)
    .execute(pool)
    .await?;
    Ok(result.rows_affected() > 0)
}

fn settings_from_row(row: &SqliteRow) -> Option<Settings> {
    Some(Settings {
        protocol: from_db(&row.get::<String, _>("protocol"))?,
        name: row.get("name"),
        host: row.get("host"),
        port: row
            .get::<Option<i64>, _>("port")
            .and_then(|port| u16::try_from(port).ok()),
        path: row.get("path"),
        auth: from_db(&row.get::<String, _>("auth"))?,
        username: row.get("username"),
        security: row
            .get::<Option<String>, _>("security")
            .and_then(|security| from_db(&security)),
        remember_password: row.get("remember_password"),
    })
}

async fn load_settings(pool: &SqlitePool, id: &str) -> Result<Option<Settings>, sqlx::Error> {
    let row = sqlx::query(
        "SELECT name, protocol, host, port, path, username, auth, security, remember_password
         FROM network_locations WHERE id = ?",
    )
    .bind(id)
    .fetch_optional(pool)
    .await?;
    Ok(row.as_ref().and_then(settings_from_row))
}

async fn delete_row(pool: &SqlitePool, id: &str) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM network_locations WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

async fn all_settings(pool: &SqlitePool) -> Result<Vec<(String, Settings)>, sqlx::Error> {
    let rows = sqlx::query(
        "SELECT id, name, protocol, host, port, path, username, auth, security, remember_password
         FROM network_locations ORDER BY position, name",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows
        .iter()
        .filter_map(|row| settings_from_row(row).map(|settings| (row.get("id"), settings)))
        .collect())
}

/// Saved network locations as sidebar entries, in the order they were added.
pub async fn network_locations(pool: &SqlitePool) -> Result<Vec<Location>, sqlx::Error> {
    Ok(all_settings(pool)
        .await?
        .iter()
        .map(|(id, settings)| location(id, settings))
        .collect())
}

async fn remember_password(pool: &SqlitePool, id: &str) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE network_locations SET remember_password = 1 WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

async fn blocking<T: Send + 'static>(
    job: impl FnOnce() -> T + Send + 'static,
) -> Result<T, ConnectError> {
    tauri::async_runtime::spawn_blocking(job)
        .await
        .map_err(ConnectError::other)
}

fn credentials(settings: &Settings, password: Option<String>) -> mount::Credentials {
    mount::Credentials {
        username: settings.username.clone(),
        password: password.filter(|password| !password.is_empty()),
    }
}

/// SMB without a share on macOS: the location lists the server's shares instead of mounting one.
fn picks_share_later(settings: &Settings) -> bool {
    settings.protocol == Protocol::Smb && settings.path.is_empty() && mount::picks_shares()
}

/// Signs in to list the shares of an SMB server, for a location without a share.
async fn check_shares(settings: &Settings, password: String) -> Result<Vec<String>, ConnectError> {
    let target = settings.target();
    let credentials = credentials(settings, Some(password));
    blocking(move || mount::list_shares(&target, &credentials)).await?
}

fn signed_in(settings: &Settings) -> ConnectionCheck {
    ConnectionCheck {
        message: match &settings.username {
            Some(username) => format!("Signed in to {} as {username}.", settings.host),
            None => format!("Connected to {}.", settings.host),
        },
    }
}

fn saved_id(path: &str) -> Result<String, ConnectError> {
    parse_network_path(path)
        .map(|network| network.id)
        .ok_or_else(|| ConnectError::new(ErrorKind::NotFound, "Not a network location."))
}

fn gone() -> ConnectError {
    ConnectError::new(ErrorKind::NotFound, "This location no longer exists.")
}

/// Mountable shares are mounted and, when this call mounted them, unmounted again.
#[tauri::command]
pub async fn test_network_location(
    database: State<'_, Database>,
    input: NetworkLocationInput,
) -> Result<ConnectionCheck, ConnectError> {
    let settings = resolve(&input)?;
    if !mount::is_mountable(settings.protocol) {
        let session = servers::open_session(&database.0, &settings, Some(input.password)).await?;
        let home = if settings.path.is_empty() {
            session.home().await.ok()
        } else {
            None
        };
        session.close().await;
        let mut check = signed_in(&settings);
        if let Some(home) = home {
            check.message.push_str(&format!(" Starts in {home}."));
        }
        return Ok(check);
    }
    let target = settings.target();
    mount::require_share(&target)?;
    check_reachable(&settings).await?;
    if picks_share_later(&settings) {
        let shares = check_shares(&settings, input.password).await?;
        let mut check = signed_in(&settings);
        check.message.push_str(&match shares.len() {
            0 => " No shares are available to this account.".to_string(),
            1 => format!(" 1 share: {}.", shares[0]),
            count => format!(" {count} shares: {}.", shares.join(", ")),
        });
        return Ok(check);
    }
    let credentials = credentials(&settings, Some(input.password));
    let (existing, target) = blocking(move || (mount::find_mounted(&target), target)).await?;
    let (mounted, target) = blocking(move || (mount::mount(&target, &credentials), target)).await?;
    let local = mounted?;
    if existing.is_none() {
        let _ = blocking(move || mount::unmount(&target, &local)).await;
    }
    Ok(signed_in(&settings))
}

/// Mountable shares are mounted before saving, so a wrong password is caught here and the
/// location opens right away.
#[tauri::command]
pub async fn add_network_location(
    database: State<'_, Database>,
    mounts: State<'_, Mounts>,
    sessions: State<'_, servers::Sessions>,
    input: NetworkLocationInput,
) -> Result<Location, ConnectError> {
    let mut settings = resolve(&input)?;
    let mut local = None;
    let mut session = None;
    if mount::is_mountable(settings.protocol) {
        let target = settings.target();
        mount::require_share(&target)?;
        check_reachable(&settings).await?;
        if picks_share_later(&settings) {
            check_shares(&settings, input.password.clone()).await?;
        } else {
            let credentials = credentials(&settings, Some(input.password.clone()));
            local = Some(blocking(move || mount::mount(&target, &credentials)).await??);
        }
    } else {
        // Sign in before saving; an empty path starts in the account's home folder.
        let opened =
            servers::open_session(&database.0, &settings, Some(input.password.clone())).await?;
        if settings.path.is_empty() {
            if let Ok(home) = opened.home().await {
                settings.path = normalize_path(settings.protocol, &home);
            }
        }
        session = Some(opened);
    }

    let id = uuid::Uuid::new_v4().to_string();
    insert_location(&database.0, &id, &settings).await?;
    if let Some(local) = local {
        mounts.insert(&id, local);
    } else if picks_share_later(&settings) {
        mounts.connect_server(&id, Some(input.password.clone()).filter(|p| !p.is_empty()));
    }
    if let Some(session) = session {
        sessions
            .adopt(&id, session, Some(input.password.clone()))
            .await;
    }
    if settings.remember_password && !input.password.is_empty() {
        if let Err(error) = store_secret(KEYCHAIN_SERVICE, id.clone(), input.password).await {
            let _ = delete_row(&database.0, &id).await;
            return Err(ConnectError::other(error));
        }
    }
    Ok(location(&id, &settings))
}

/// Saves edits without a reachability check, so offline servers can still be renamed.
/// An empty password keeps the saved one.
#[tauri::command]
pub async fn update_network_location(
    database: State<'_, Database>,
    sessions: State<'_, servers::Sessions>,
    path: String,
    input: NetworkLocationInput,
) -> Result<Location, ConnectError> {
    let id = saved_id(&path)?;
    let settings = resolve(&input)?;
    sessions.close(&id).await;
    if !update_location(&database.0, &id, &settings).await? {
        return Err(gone());
    }
    if !settings.remember_password {
        delete_secret(KEYCHAIN_SERVICE, id.clone())
            .await
            .map_err(ConnectError::other)?;
    } else if !input.password.is_empty() {
        store_secret(KEYCHAIN_SERVICE, id.clone(), input.password)
            .await
            .map_err(ConnectError::other)?;
    }
    Ok(location(&id, &settings))
}

#[tauri::command]
pub async fn remove_network_location(
    database: State<'_, Database>,
    mounts: State<'_, Mounts>,
    sessions: State<'_, servers::Sessions>,
    path: String,
) -> Result<(), ConnectError> {
    let id = saved_id(&path)?;
    mounts.remove(&id);
    sessions.close(&id).await;
    delete_row(&database.0, &id).await?;
    delete_secret(KEYCHAIN_SERVICE, id)
        .await
        .map_err(ConnectError::other)
}

/// Saved settings for the edit dialog. The password is never included.
#[tauri::command]
pub async fn network_location(
    database: State<'_, Database>,
    path: String,
) -> Result<NetworkLocationInput, ConnectError> {
    let id = saved_id(&path)?;
    load_settings(&database.0, &id)
        .await?
        .map(|settings| settings.input())
        .ok_or_else(gone)
}

/// Mounts a saved location, or reuses its mount, and returns the local folder.
/// `password` comes from the password prompt; without it the saved password is used.
///
/// An SMB location without a share signs in to list the server's shares, like Finder, and
/// returns its own path. Shares are mounted one by one with `mount_network_share`.
#[tauri::command]
pub async fn connect_network_location(
    database: State<'_, Database>,
    mounts: State<'_, Mounts>,
    sessions: State<'_, servers::Sessions>,
    path: String,
    password: Option<String>,
    remember: Option<bool>,
) -> Result<Connection, ConnectError> {
    let id = saved_id(&path)?;
    let settings = load_settings(&database.0, &id).await?.ok_or_else(gone)?;
    if !mount::is_mountable(settings.protocol) {
        // SFTP and FTP are browsed through their own `sftp://` or `ftp://` paths.
        sessions.connect(&database.0, &id, password.clone()).await?;
        remember_typed(&database.0, &id, password, remember).await?;
        return Ok(Connection {
            location: path.clone(),
            path,
        });
    }
    let target = settings.target();
    mount::require_share(&target)?;

    if mount::is_server_root(&target) {
        let typed = password.filter(|password| !password.is_empty());
        let lookup = target.clone();
        let mounted = !blocking(move || mount::mounted_shares(&lookup))
            .await?
            .is_empty();
        let resolved = match typed.clone().or_else(|| mounts.server_password(&id)) {
            Some(password) => Ok(Some(password)),
            None => saved_password(&settings, &id, None).await,
        };
        let listed = match resolved {
            Ok(password) => {
                let credentials = credentials(&settings, password);
                blocking(move || mount::list_shares(&target, &credentials)).await?
            }
            Err(error) => Err(error),
        };
        match listed {
            Ok(_) => remember_typed(&database.0, &id, typed.clone(), remember).await?,
            // Shares already mounted, for example from Finder, stay usable when listing fails,
            // unless the password typed in the prompt was rejected.
            Err(error) if mounted && !(error.kind == ErrorKind::Auth && typed.is_some()) => {}
            Err(error) => return Err(error),
        }
        mounts.connect_server(&id, typed);
        return Ok(Connection {
            location: path.clone(),
            path,
        });
    }

    let connected = |local: PathBuf| Connection {
        location: path.clone(),
        path: local.to_string_lossy().into_owned(),
    };
    if let Some(local) = mounts.get(&id).filter(|local| local.exists()) {
        return Ok(connected(local));
    }
    let (existing, target) = blocking(move || (mount::find_mounted(&target), target)).await?;
    if let Some(local) = existing {
        mounts.insert(&id, local.clone());
        return Ok(connected(local));
    }

    let typed = password.filter(|password| !password.is_empty());
    let credentials = credentials(
        &settings,
        saved_password(&settings, &id, typed.clone()).await?,
    );
    let local = blocking(move || mount::mount(&target, &credentials)).await??;
    mounts.insert(&id, local.clone());
    remember_typed(&database.0, &id, typed, remember).await?;
    Ok(connected(local))
}

/// The typed password, or the saved one. Guests need none.
async fn saved_password(
    settings: &Settings,
    id: &str,
    typed: Option<String>,
) -> Result<Option<String>, ConnectError> {
    if settings.auth == Auth::Guest {
        return Ok(None);
    }
    if typed.is_some() {
        return Ok(typed);
    }
    read_optional_secret(KEYCHAIN_SERVICE, id.to_string())
        .await
        .map_err(ConnectError::other)?
        .map(Some)
        .ok_or_else(|| ConnectError::new(ErrorKind::Auth, "Enter the password for this server."))
}

/// Saves a password typed in the prompt when "Remember password" was on.
async fn remember_typed(
    pool: &SqlitePool,
    id: &str,
    typed: Option<String>,
    remember: Option<bool>,
) -> Result<(), ConnectError> {
    if let (Some(password), Some(true)) = (typed.filter(|p| !p.is_empty()), remember) {
        store_secret(KEYCHAIN_SERVICE, id.to_string(), password)
            .await
            .map_err(ConnectError::other)?;
        remember_password(pool, id).await?;
    }
    Ok(())
}

/// Entries of kind `share` for an SMB location without a share. Mounted shares point at their
/// local folder; the others are `smb://<id>/<share>` paths, mounted when opened.
pub async fn list_shares(
    pool: &SqlitePool,
    mounts: &Mounts,
    path: &str,
) -> Result<Vec<crate::DirectoryEntry>, String> {
    let unsupported = || String::from("Connecting to network locations isn’t supported yet.");
    let id = saved_id(path).map_err(|_| unsupported())?;
    let settings = load_settings(pool, &id)
        .await
        .map_err(|error| error.to_string())?
        .ok_or_else(gone)?;
    let target = settings.target();
    if !mount::is_server_root(&target) {
        return Err(unsupported());
    }
    let password = match mounts.server_password(&id) {
        Some(password) => Some(password),
        None => saved_password(&settings, &id, None).await.ok().flatten(),
    };
    let credentials = credentials(&settings, password);
    let (mounted, listed) = blocking(move || {
        (
            mount::mounted_shares(&target),
            mount::list_shares(&target, &credentials),
        )
    })
    .await?;
    let listed = match listed {
        Ok(listed) => listed,
        Err(_) if !mounted.is_empty() => Vec::new(),
        Err(error) => return Err(error.into()),
    };

    let mut entries: Vec<crate::DirectoryEntry> = mounted
        .iter()
        .map(|(share, local)| share_entry(share, local.to_string_lossy().into_owned()))
        .collect();
    for share in listed {
        if !mounted
            .iter()
            .any(|(name, _)| name.eq_ignore_ascii_case(&share))
        {
            let path = network_path(settings.protocol, &id, &share);
            entries.push(share_entry(&share, path));
        }
    }
    entries.sort_by_key(|entry| entry.name.to_lowercase());
    Ok(entries)
}

fn share_entry(name: &str, path: String) -> crate::DirectoryEntry {
    crate::DirectoryEntry {
        name: name.to_string(),
        path,
        is_directory: true,
        is_hidden: false,
        size: None,
        created: None,
        kind: Some("share"),
    }
}

/// Mounts one share of an SMB location without a share (`smb://<id>/<share>`) and returns its
/// local folder. Reuses an existing mount of that share.
#[tauri::command]
pub async fn mount_network_share(
    database: State<'_, Database>,
    mounts: State<'_, Mounts>,
    path: String,
    password: Option<String>,
    remember: Option<bool>,
) -> Result<Connection, ConnectError> {
    let network = parse_network_path(&path)
        .ok_or_else(|| ConnectError::new(ErrorKind::NotFound, "Not a network location."))?;
    let settings = load_settings(&database.0, &network.id)
        .await?
        .ok_or_else(gone)?;
    let (share, _) = mount::targets_split_share(&network.path);
    let mut target = settings.target();
    if !mount::is_server_root(&target) || share.is_empty() {
        return Err(ConnectError::new(
            ErrorKind::NotFound,
            "Not a share of this server.",
        ));
    }
    target.path = share.to_string();
    let location = network_path(settings.protocol, &network.id, &settings.path);
    let connected = |local: PathBuf| Connection {
        location: location.clone(),
        path: local.to_string_lossy().into_owned(),
    };

    let (existing, target) = blocking(move || (mount::find_mounted(&target), target)).await?;
    if let Some(local) = existing {
        return Ok(connected(local));
    }
    let typed = password.filter(|password| !password.is_empty());
    let password = match typed
        .clone()
        .or_else(|| mounts.server_password(&network.id))
    {
        Some(password) => Some(password),
        None => saved_password(&settings, &network.id, None).await?,
    };
    let credentials = credentials(&settings, password);
    let local = blocking(move || mount::mount(&target, &credentials)).await??;
    mounts.connect_server(&network.id, typed.clone());
    remember_typed(&database.0, &network.id, typed, remember).await?;
    Ok(connected(local))
}

#[tauri::command]
pub async fn disconnect_network_location(
    database: State<'_, Database>,
    mounts: State<'_, Mounts>,
    sessions: State<'_, servers::Sessions>,
    path: String,
) -> Result<(), ConnectError> {
    let id = saved_id(&path)?;
    let settings = load_settings(&database.0, &id).await?.ok_or_else(gone)?;
    if !mount::is_mountable(settings.protocol) {
        sessions.close(&id).await;
        return Ok(());
    }
    let target = settings.target();
    if mount::is_server_root(&target) {
        // Disconnecting the server ejects every share mounted from it.
        mounts.remove(&id);
        return blocking(move || {
            mount::mounted_shares(&target)
                .into_iter()
                .try_for_each(|(_, local)| mount::unmount(&target, &local))
        })
        .await?;
    }
    let local = match mounts.remove(&id) {
        Some(local) => Some(local),
        None => {
            let lookup = target.clone();
            blocking(move || mount::find_mounted(&lookup)).await?
        }
    };
    match local {
        Some(local) => blocking(move || mount::unmount(&target, &local)).await?,
        None => Ok(()),
    }
}

/// Saved locations that are connected right now: mounted shares, and SFTP or FTP locations
/// with an open session (their `path` is the location path itself).
#[tauri::command]
pub async fn network_connections(
    database: State<'_, Database>,
    mounts: State<'_, Mounts>,
    sessions: State<'_, servers::Sessions>,
) -> Result<Vec<Connection>, ConnectError> {
    let saved = all_settings(&database.0).await?;
    let mut servers = Vec::new();
    for (id, settings) in &saved {
        if !mount::is_mountable(settings.protocol) && sessions.is_open(id).await {
            let location = network_path(settings.protocol, id, &settings.path);
            servers.push(Connection {
                path: location.clone(),
                location,
            });
        }
    }
    let known: Vec<_> = saved
        .into_iter()
        .filter(|(_, settings)| mount::is_mountable(settings.protocol))
        .map(|(id, settings)| (mounts.get(&id), id, settings))
        .collect();
    let (roots, known): (Vec<_>, Vec<_>) = known
        .into_iter()
        .partition(|(_, _, settings)| mount::is_server_root(&settings.target()));
    let roots: Vec<_> = roots
        .into_iter()
        .map(|(_, id, settings)| (mounts.is_server_connected(&id), id, settings))
        .collect();
    let roots = blocking(move || {
        roots
            .into_iter()
            .filter(|(connected, _, settings)| {
                *connected || !mount::mounted_shares(&settings.target()).is_empty()
            })
            .map(|(_, id, settings)| {
                let location = network_path(settings.protocol, &id, &settings.path);
                Connection {
                    path: location.clone(),
                    location,
                }
            })
            .collect::<Vec<_>>()
    })
    .await?;
    let found = blocking(move || {
        known
            .into_iter()
            .filter_map(|(local, id, settings)| {
                let local = local
                    .filter(|local| local.exists())
                    .or_else(|| mount::find_mounted(&settings.target()))?;
                Some((id, settings, local))
            })
            .collect::<Vec<_>>()
    })
    .await?;
    let mounted = found.into_iter().map(|(id, settings, local)| {
        mounts.insert(&id, local.clone());
        Connection {
            location: network_path(settings.protocol, &id, &settings.path),
            path: local.to_string_lossy().into_owned(),
        }
    });
    Ok(mounted.chain(roots).chain(servers).collect())
}

/// Pins a server key or certificate the user confirmed, so the next connection accepts it.
#[tauri::command]
pub async fn trust_network_host(
    database: State<'_, Database>,
    trust: servers::TrustRequest,
) -> Result<(), ConnectError> {
    servers::trust(&database.0, &trust).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::SqlitePoolOptions;

    #[test]
    fn settings_become_mount_targets() {
        let mut smb = input(Protocol::Smb);
        smb.path = "/Photos/2026".into();
        let target = resolve(&smb).unwrap().target();
        assert_eq!(
            target,
            mount::Target {
                protocol: Protocol::Smb,
                host: "nas.local".into(),
                port: None,
                path: "Photos/2026".into(),
                security: None,
            }
        );
        assert!(mount::is_mountable(Protocol::Webdav));
        assert!(!mount::is_mountable(Protocol::Sftp));
    }

    #[test]
    fn missing_share_depends_on_the_system_picker() {
        let target = resolve(&input(Protocol::Smb)).unwrap().target();
        assert_eq!(mount::require_share(&target).is_ok(), mount::picks_shares());
        let mut shared = input(Protocol::Smb);
        shared.path = "Photos".into();
        assert!(mount::require_share(&resolve(&shared).unwrap().target()).is_ok());
    }

    fn input(protocol: Protocol) -> NetworkLocationInput {
        NetworkLocationInput {
            protocol,
            name: String::new(),
            host: " nas.local ".into(),
            port: None,
            path: String::new(),
            auth: Auth::Guest,
            username: String::new(),
            password: String::new(),
            security: None,
            remember_password: true,
        }
    }

    fn invalid(input: &NetworkLocationInput) -> String {
        let error = resolve(input).unwrap_err();
        assert_eq!(error.kind, ErrorKind::Invalid);
        error.message
    }

    #[test]
    fn default_ports_follow_protocol_and_security() {
        let port = |input: NetworkLocationInput| resolve(&input).unwrap().port();
        assert_eq!(port(input(Protocol::Smb)), 445);
        let mut nfs = input(Protocol::Nfs);
        nfs.path = "/srv".into();
        assert_eq!(port(nfs), 2049);
        assert_eq!(port(input(Protocol::Webdav)), 443);
        let mut http = input(Protocol::Webdav);
        http.security = Some(Security::Http);
        assert_eq!(port(http), 80);
        let mut sftp = input(Protocol::Sftp);
        sftp.username = "pi".into();
        assert_eq!(port(sftp), 22);
        assert_eq!(port(input(Protocol::Ftp)), 21);
        let mut implicit = input(Protocol::Ftp);
        implicit.security = Some(Security::Implicit);
        assert_eq!(port(implicit), 990);
        let mut custom = input(Protocol::Smb);
        custom.port = Some(1445);
        assert_eq!(port(custom), 1445);
    }

    #[test]
    fn host_must_be_a_plain_name() {
        let mut smb = input(Protocol::Smb);
        assert_eq!(resolve(&smb).unwrap().host, "nas.local");
        smb.host = "  ".into();
        assert_eq!(invalid(&smb), "Enter a server name or IP address.");
        for host in ["smb://nas", "nas/Photos", "nas\\Photos", "nas local"] {
            smb.host = host.into();
            assert_eq!(
                invalid(&smb),
                "Server should be a name or IP address, without :// or a folder."
            );
        }
        smb.host = "nas".into();
        smb.port = Some(0);
        assert_eq!(invalid(&smb), "Port must be between 1 and 65535.");
    }

    #[test]
    fn nfs_requires_a_path_and_never_uses_a_password() {
        let mut nfs = input(Protocol::Nfs);
        assert_eq!(
            invalid(&nfs),
            "NFS needs the export path, like /srv/exports/media."
        );
        nfs.path = "srv/media/".into();
        nfs.auth = Auth::Password;
        nfs.username = "root".into();
        let settings = resolve(&nfs).unwrap();
        assert_eq!(settings.path, "/srv/media");
        assert_eq!(settings.auth, Auth::Guest);
        assert_eq!(settings.username, None);
        assert!(!settings.remember_password);
    }

    #[test]
    fn sftp_always_needs_a_username() {
        let mut sftp = input(Protocol::Sftp);
        assert_eq!(invalid(&sftp), "Enter your username.");
        sftp.username = " pi ".into();
        let settings = resolve(&sftp).unwrap();
        assert_eq!(settings.auth, Auth::Password);
        assert_eq!(settings.username.as_deref(), Some("pi"));
        assert!(settings.remember_password);
    }

    #[test]
    fn guest_access_drops_username_and_remember() {
        let mut smb = input(Protocol::Smb);
        smb.username = "sebas".into();
        let settings = resolve(&smb).unwrap();
        assert_eq!(settings.username, None);
        assert!(!settings.remember_password);
        smb.auth = Auth::Password;
        smb.username = String::new();
        assert_eq!(invalid(&smb), "Enter your username.");
    }

    #[test]
    fn security_defaults_per_protocol() {
        assert_eq!(
            resolve(&input(Protocol::Webdav)).unwrap().security,
            Some(Security::Https)
        );
        assert_eq!(
            resolve(&input(Protocol::Ftp)).unwrap().security,
            Some(Security::Explicit)
        );
        let mut webdav = input(Protocol::Webdav);
        webdav.security = Some(Security::Plain);
        assert_eq!(invalid(&webdav), "WebDAV security must be http or https.");
        let mut ftp = input(Protocol::Ftp);
        ftp.security = Some(Security::Https);
        assert_eq!(
            invalid(&ftp),
            "FTP security must be explicit, implicit or plain."
        );
        let mut smb = input(Protocol::Smb);
        smb.security = Some(Security::Https);
        assert_eq!(resolve(&smb).unwrap().security, None);
    }

    #[test]
    fn paths_and_names_are_normalized() {
        let mut smb = input(Protocol::Smb);
        smb.path = "/Photos/2026/".into();
        let settings = resolve(&smb).unwrap();
        assert_eq!(settings.path, "Photos/2026");
        assert_eq!(settings.name, "2026");
        smb.path = "Photos\\2026".into();
        assert_eq!(resolve(&smb).unwrap().path, "Photos/2026");

        let mut sftp = input(Protocol::Sftp);
        sftp.username = "pi".into();
        sftp.path = "home/pi".into();
        let settings = resolve(&sftp).unwrap();
        assert_eq!(settings.path, "/home/pi");
        assert_eq!(settings.name, "pi");

        let mut named = input(Protocol::Smb);
        named.name = "  NAS  ".into();
        assert_eq!(resolve(&named).unwrap().name, "NAS");
        assert_eq!(resolve(&input(Protocol::Smb)).unwrap().name, "nas.local");
    }

    #[test]
    fn network_paths_round_trip() {
        assert_eq!(
            network_path(Protocol::Smb, "abc", "Photos/2026"),
            "smb://abc/Photos/2026"
        );
        assert_eq!(
            network_path(Protocol::Sftp, "abc", "/home/pi"),
            "sftp://abc/home/pi"
        );
        assert_eq!(
            parse_network_path("sftp://abc/home/pi"),
            Some(NetworkPath {
                protocol: Protocol::Sftp,
                id: "abc".into(),
                path: "home/pi".into()
            })
        );
        assert_eq!(
            parse_network_path("webdav://abc"),
            Some(NetworkPath {
                protocol: Protocol::Webdav,
                id: "abc".into(),
                path: String::new()
            })
        );
        assert_eq!(parse_network_path("s3://abc/"), None);
        assert_eq!(parse_network_path("smb:///Photos"), None);
        assert_eq!(parse_network_path("/Users/me"), None);
        assert!(is_network_path("ftp://abc/"));
        assert!(!is_network_path("s3://abc/"));
    }

    #[test]
    fn errors_serialize_with_snake_case_kind() {
        let json = serde_json::to_value(ConnectError::new(ErrorKind::NotFound, "gone")).unwrap();
        assert_eq!(
            json,
            serde_json::json!({ "kind": "not_found", "message": "gone" })
        );
    }

    #[test]
    fn reachability_reports_open_and_closed_ports() {
        tauri::async_runtime::block_on(async {
            let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            let port = listener.local_addr().unwrap().port();
            let mut smb = input(Protocol::Smb);
            smb.host = "127.0.0.1".into();
            smb.port = Some(port);
            let settings = resolve(&smb).unwrap();
            assert_eq!(
                check_reachable(&settings).await.unwrap().message,
                format!("127.0.0.1 answered on port {port}.")
            );
            drop(listener);
            let error = check_reachable(&settings).await.unwrap_err();
            assert_eq!(error.kind, ErrorKind::Unreachable);
        });
    }

    #[test]
    fn saved_locations_list_update_and_load_for_editing() {
        tauri::async_runtime::block_on(async {
            let pool = SqlitePoolOptions::new()
                .max_connections(1)
                .connect("sqlite::memory:")
                .await
                .unwrap();
            super::super::apply_migrations(&pool).await.unwrap();

            let mut smb = input(Protocol::Smb);
            smb.path = "Photos".into();
            insert_location(&pool, "one", &resolve(&smb).unwrap())
                .await
                .unwrap();
            let mut sftp = input(Protocol::Sftp);
            sftp.username = "pi".into();
            sftp.path = "/home/pi".into();
            sftp.password = "secret".into();
            insert_location(&pool, "two", &resolve(&sftp).unwrap())
                .await
                .unwrap();

            let locations = network_locations(&pool).await.unwrap();
            let listed: Vec<_> = locations
                .iter()
                .map(|l| (l.path.as_str(), l.kind.as_str(), l.name.as_str()))
                .collect();
            assert_eq!(
                listed,
                [
                    ("smb://one/Photos", "smb", "Photos"),
                    ("sftp://two/home/pi", "sftp", "pi")
                ]
            );

            let loaded = load_settings(&pool, "two").await.unwrap().unwrap();
            assert_eq!(loaded, resolve(&sftp).unwrap());
            let for_editing = loaded.input();
            assert_eq!(for_editing.password, "");
            assert_eq!(for_editing.username, "pi");
            assert_eq!(for_editing.port, None);

            let mut renamed = sftp.clone();
            renamed.name = "Pi home".into();
            renamed.port = Some(2222);
            assert!(update_location(&pool, "two", &resolve(&renamed).unwrap())
                .await
                .unwrap());
            let updated = load_settings(&pool, "two").await.unwrap().unwrap();
            assert_eq!(
                (updated.name.as_str(), updated.port),
                ("Pi home", Some(2222))
            );
            assert!(
                !update_location(&pool, "missing", &resolve(&renamed).unwrap())
                    .await
                    .unwrap()
            );

            delete_row(&pool, "one").await.unwrap();
            assert_eq!(network_locations(&pool).await.unwrap().len(), 1);
            assert!(load_settings(&pool, "one").await.unwrap().is_none());
        });
    }
}
