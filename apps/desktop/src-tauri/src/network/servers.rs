//! SFTP and FTP servers, browsed inside the app instead of being mounted. Paths look like
//! `sftp://<location-id>/home/pi/notes.txt` and map to the absolute server path
//! `/home/pi/notes.txt`. One session per location stays open and reconnects once when the
//! connection drops.

use std::{
    collections::{HashMap, HashSet},
    future::Future,
    path::{Path, PathBuf},
    pin::Pin,
    sync::Arc,
    time::{Duration, Instant},
};

use serde::{Deserialize, Serialize};
use sqlx::{Row, SqlitePool};
use tauri::{AppHandle, Manager};
use tokio_util::sync::CancellationToken;

use super::{
    load_settings, network_path, parse_network_path, to_db, Auth, ConnectError, ErrorKind,
    Protocol, Security, Settings, KEYCHAIN_SERVICE,
};
use crate::remote::read_optional_secret;
use crate::remote::transfer::{self, TransferEvent, TransferRegistry};
use crate::{
    apply_text_extension_kind, classify_preview_bytes, media_preview_kind, preview::arrow,
    preview::avro, preview::calendar, preview::certificate, preview::dicom, preview::fb2,
    preview::geo, preview::iso, preview::mail, preview::mobi, preview::msg, preview::notebook,
    preview::parquet, preview::pcap, preview::psd, preview::sheet, preview::subtitle,
    preview::torrent, preview::vcard, DirectoryEntry, FilePreview, PreviewKind, PREVIEW_MAX_BYTES,
};

mod ftp;
mod sftp;

pub const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const CHUNK: usize = 256 * 1024;
const PROGRESS_INTERVAL: Duration = Duration::from_millis(150);
/// Largest file "Open" copies to the local cache.
const OPEN_MAX_BYTES: u64 = 200 * 1024 * 1024;

/// Where a session connects.
#[derive(Debug, Clone)]
pub struct ServerTarget {
    pub protocol: Protocol,
    pub host: String,
    pub port: u16,
    pub security: Option<Security>,
}

impl ServerTarget {
    fn of(settings: &Settings) -> Self {
        Self {
            protocol: settings.protocol,
            host: settings.host.clone(),
            port: settings.port(),
            security: settings.security,
        }
    }
}

/// A server key or certificate the user can choose to trust.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TrustRequest {
    pub protocol: Protocol,
    pub host: String,
    pub port: u16,
    pub algorithm: String,
    pub fingerprint: String,
    /// A different key or certificate was trusted before.
    pub changed: bool,
}

/// A file or folder as the server reports it.
#[derive(Debug, Clone, PartialEq)]
pub struct RemoteEntry {
    pub name: String,
    pub is_dir: bool,
    pub size: u64,
    /// Milliseconds since the Unix epoch.
    pub modified: Option<u64>,
}

pub fn join(dir: &str, name: &str) -> String {
    if dir.ends_with('/') {
        format!("{dir}{name}")
    } else {
        format!("{dir}/{name}")
    }
}

pub fn file_name(path: &str) -> &str {
    path.trim_end_matches('/').rsplit('/').next().unwrap_or("")
}

pub fn parent(path: &str) -> &str {
    let trimmed = path.trim_end_matches('/');
    match trimmed.rfind('/') {
        Some(0) | None => "/",
        Some(index) => &trimmed[..index],
    }
}

/// `base`, or `base 2`, `base 3`… (before the extension for files) when taken.
fn unique_among(names: &HashSet<String>, base: &str, is_dir: bool) -> String {
    if !names.contains(base) {
        return base.to_string();
    }
    let (stem, extension) = match base.rfind('.') {
        Some(dot) if !is_dir && dot > 0 => (&base[..dot], &base[dot..]),
        _ => (base, ""),
    };
    (2..)
        .map(|index| format!("{stem} {index}{extension}"))
        .find(|candidate| !names.contains(candidate))
        .unwrap()
}

pub fn lost() -> ConnectError {
    ConnectError::new(
        ErrorKind::Unreachable,
        "The connection to the server was lost.",
    )
}

pub fn untrusted(
    target: &ServerTarget,
    algorithm: String,
    fingerprint: String,
    changed: bool,
) -> ConnectError {
    let certificate = algorithm == "certificate";
    let (kind, message) = match (certificate, changed) {
        (true, false) => (
            ErrorKind::CertificateUntrusted,
            format!("{} uses a certificate this computer doesn’t trust.", target.host),
        ),
        (true, true) => (
            ErrorKind::CertificateUntrusted,
            format!("The certificate of {} changed since you trusted it.", target.host),
        ),
        (false, false) => (
            ErrorKind::HostKeyUnknown,
            format!("First connection to {}. Check its fingerprint before trusting it.", target.host),
        ),
        (false, true) => (
            ErrorKind::HostKeyChanged,
            format!(
                "The host key of {} changed since the last connection. Someone could be intercepting it, or the server was reinstalled.",
                target.host
            ),
        ),
    };
    let mut error = ConnectError::new(kind, message);
    error.trust = Some(TrustRequest {
        protocol: target.protocol,
        host: target.host.to_lowercase(),
        port: target.port,
        algorithm,
        fingerprint,
        changed,
    });
    error
}

async fn known_fingerprint(
    pool: &SqlitePool,
    target: &ServerTarget,
) -> Result<Option<String>, sqlx::Error> {
    let row = sqlx::query(
        "SELECT fingerprint FROM known_hosts WHERE host = ? AND port = ? AND protocol = ?",
    )
    .bind(target.host.to_lowercase())
    .bind(i64::from(target.port))
    .bind(to_db(target.protocol))
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|row| row.get("fingerprint")))
}

pub async fn trust(pool: &SqlitePool, request: &TrustRequest) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO known_hosts (host, port, protocol, algorithm, fingerprint) VALUES (?, ?, ?, ?, ?)
         ON CONFLICT (host, port, protocol) DO UPDATE SET algorithm = excluded.algorithm, fingerprint = excluded.fingerprint",
    )
    .bind(request.host.to_lowercase())
    .bind(i64::from(request.port))
    .bind(to_db(request.protocol))
    .bind(&request.algorithm)
    .bind(&request.fingerprint)
    .execute(pool)
    .await?;
    Ok(())
}

/// Transfer progress as `transfer-progress` events, or silent for internal copies.
pub struct Progress {
    app: Option<AppHandle>,
    token: CancellationToken,
    state: TransferEvent,
    last_emit: Instant,
}

impl Progress {
    fn silent() -> Self {
        Self {
            app: None,
            token: CancellationToken::new(),
            state: TransferEvent::new(String::new(), "", String::new(), 0, 0),
            last_emit: Instant::now(),
        }
    }

    fn new(
        app: AppHandle,
        token: CancellationToken,
        id: String,
        kind: &str,
        destination: String,
        files_total: u64,
        bytes_total: u64,
    ) -> Self {
        let progress = Self {
            app: Some(app),
            token,
            state: TransferEvent::new(id, kind, destination, files_total, bytes_total),
            last_emit: Instant::now(),
        };
        progress.emit();
        progress
    }

    fn emit(&self) {
        if let Some(app) = &self.app {
            transfer::emit(app, &self.state);
        }
    }

    pub fn cancelled(&self) -> bool {
        self.token.is_cancelled()
    }

    pub fn add_bytes(&mut self, bytes: u64) {
        self.state.add_bytes(bytes);
        if self.last_emit.elapsed() >= PROGRESS_INTERVAL {
            self.emit();
            self.last_emit = Instant::now();
        }
    }

    fn start_file(&mut self, name: &str) {
        self.state.label = name.to_string();
        self.emit();
        self.last_emit = Instant::now();
    }

    fn finish_file(&mut self) {
        self.state.files_done += 1;
        self.state.file_progress = None;
        self.emit();
    }

    fn finish(mut self) {
        self.state.state = if self.cancelled() {
            "cancelled"
        } else {
            "done"
        }
        .to_string();
        self.state.label.clear();
        self.emit();
    }

    fn fail(mut self, error: String) {
        self.state.state = "failed".to_string();
        self.state.error = Some(error);
        self.emit();
    }
}

/// An open SFTP or FTP session.
pub enum Session {
    Sftp(sftp::SftpConnection),
    Ftp(ftp::FtpConnection),
}

macro_rules! dispatch {
    ($self:ident . $method:ident ( $($argument:expr),* )) => {
        match $self {
            Session::Sftp(connection) => connection.$method($($argument),*).await,
            Session::Ftp(connection) => connection.$method($($argument),*).await,
        }
    };
}

impl Session {
    pub async fn home(&self) -> Result<String, ConnectError> {
        dispatch!(self.home())
    }
    async fn list(&self, dir: &str) -> Result<Vec<RemoteEntry>, ConnectError> {
        dispatch!(self.list(dir))
    }
    async fn stat(&self, path: &str) -> Result<RemoteEntry, ConnectError> {
        dispatch!(self.stat(path))
    }
    async fn mkdir(&self, path: &str) -> Result<(), ConnectError> {
        dispatch!(self.mkdir(path))
    }
    async fn create_file(&self, path: &str) -> Result<(), ConnectError> {
        dispatch!(self.create_file(path))
    }
    async fn rename(&self, from: &str, to: &str) -> Result<(), ConnectError> {
        dispatch!(self.rename(from, to))
    }
    async fn remove_file(&self, path: &str) -> Result<(), ConnectError> {
        dispatch!(self.remove_file(path))
    }
    async fn remove_dir(&self, path: &str) -> Result<(), ConnectError> {
        dispatch!(self.remove_dir(path))
    }
    async fn read_head(&self, path: &str, max: usize) -> Result<Vec<u8>, ConnectError> {
        dispatch!(self.read_head(path, max))
    }
    async fn download(
        &self,
        path: &str,
        local: &Path,
        progress: &mut Progress,
    ) -> Result<(), ConnectError> {
        dispatch!(self.download(path, local, progress))
    }
    async fn upload(
        &self,
        local: &Path,
        path: &str,
        progress: &mut Progress,
    ) -> Result<(), ConnectError> {
        dispatch!(self.upload(local, path, progress))
    }
    pub async fn close(&self) {
        dispatch!(self.close())
    }
}

/// Connects with explicit credentials. Used by Test, Add and the session registry.
pub(super) async fn open_session(
    pool: &SqlitePool,
    settings: &Settings,
    password: Option<String>,
) -> Result<Session, ConnectError> {
    let target = ServerTarget::of(settings);
    let known = known_fingerprint(pool, &target).await?;
    match settings.protocol {
        Protocol::Sftp => sftp::connect(
            &target,
            settings.username.as_deref().unwrap_or_default(),
            password.as_deref().unwrap_or_default(),
            known,
        )
        .await
        .map(Session::Sftp),
        Protocol::Ftp => ftp::connect(
            &target,
            settings.username.as_deref(),
            password.as_deref(),
            known,
        )
        .await
        .map(Session::Ftp),
        _ => Err(ConnectError::new(
            ErrorKind::Unsupported,
            "This location is mounted by the system.",
        )),
    }
}

/// Open sessions by location id. Passwords typed in the password prompt are kept in memory
/// so a dropped connection can reconnect.
#[derive(Default)]
pub struct Sessions {
    open: tokio::sync::Mutex<HashMap<String, Arc<Session>>>,
    passwords: std::sync::Mutex<HashMap<String, String>>,
}

impl Sessions {
    pub async fn connect(
        &self,
        pool: &SqlitePool,
        id: &str,
        typed: Option<String>,
    ) -> Result<Arc<Session>, ConnectError> {
        let mut open = self.open.lock().await;
        if let Some(session) = open.get(id) {
            return Ok(session.clone());
        }
        let settings = load_settings(pool, id).await?.ok_or_else(super::gone)?;
        let typed = typed.filter(|password| !password.is_empty());
        let password = match settings.auth {
            Auth::Guest => None,
            Auth::Password => match typed
                .clone()
                .or_else(|| self.passwords.lock().unwrap().get(id).cloned())
            {
                Some(password) => Some(password),
                None => Some(
                    read_optional_secret(KEYCHAIN_SERVICE, id.to_string())
                        .await
                        .map_err(ConnectError::other)?
                        .ok_or_else(|| {
                            ConnectError::new(
                                ErrorKind::Auth,
                                "Enter the password for this server.",
                            )
                        })?,
                ),
            },
        };
        let session = Arc::new(open_session(pool, &settings, password).await?);
        if let Some(password) = typed {
            self.passwords
                .lock()
                .unwrap()
                .insert(id.to_string(), password);
        }
        open.insert(id.to_string(), session.clone());
        Ok(session)
    }

    /// Registers a session opened while adding a location.
    pub async fn adopt(&self, id: &str, session: Session, password: Option<String>) {
        if let Some(password) = password.filter(|password| !password.is_empty()) {
            self.passwords
                .lock()
                .unwrap()
                .insert(id.to_string(), password);
        }
        self.open
            .lock()
            .await
            .insert(id.to_string(), Arc::new(session));
    }

    pub async fn close(&self, id: &str) {
        self.passwords.lock().unwrap().remove(id);
        let session = self.open.lock().await.remove(id);
        if let Some(session) = session {
            session.close().await;
        }
    }

    pub async fn is_open(&self, id: &str) -> bool {
        self.open.lock().await.contains_key(id)
    }

    async fn forget_broken(&self, id: &str) {
        self.open.lock().await.remove(id);
    }
}

/// Runs a session call, reconnecting once when the connection was lost.
macro_rules! with_session {
    ($sessions:expr, $pool:expr, $id:expr, |$session:ident| $call:expr) => {{
        let $session = $sessions.connect($pool, $id, None).await?;
        match $call {
            Err(error) if error.kind == ErrorKind::Unreachable => {
                $sessions.forget_broken($id).await;
                let $session = $sessions.connect($pool, $id, None).await?;
                $call
            }
            result => result,
        }
    }};
}

/// A server path split into location and absolute server path.
#[derive(Debug, PartialEq)]
pub struct ServerPath {
    pub protocol: Protocol,
    pub id: String,
    pub remote: String,
}

pub fn parse_server_path(path: &str) -> Option<ServerPath> {
    let network = parse_network_path(path)?;
    matches!(network.protocol, Protocol::Sftp | Protocol::Ftp).then(|| ServerPath {
        protocol: network.protocol,
        id: network.id,
        remote: format!("/{}", network.path.trim_matches('/')),
    })
}

pub fn is_server_path(path: &str) -> bool {
    parse_server_path(path).is_some()
}

fn server_path(path: &str) -> Result<ServerPath, ConnectError> {
    parse_server_path(path)
        .ok_or_else(|| ConnectError::new(ErrorKind::NotFound, "Not a server location."))
}

fn directory_entry(at: &ServerPath, dir: &str, entry: RemoteEntry) -> DirectoryEntry {
    DirectoryEntry {
        path: network_path(at.protocol, &at.id, &join(dir, &entry.name)),
        is_hidden: entry.name.starts_with('.'),
        size: (!entry.is_dir).then_some(entry.size),
        created: None,
        modified: entry.modified,
        is_directory: entry.is_dir,
        name: entry.name,
        kind: None,
    }
}

impl From<ConnectError> for String {
    fn from(error: ConnectError) -> Self {
        error.message
    }
}

pub async fn list_directory(
    pool: &SqlitePool,
    sessions: &Sessions,
    path: &str,
) -> Result<Vec<DirectoryEntry>, String> {
    let at = server_path(path)?;
    let entries = with_session!(sessions, pool, &at.id, |session| session
        .list(&at.remote)
        .await)?;
    Ok(entries
        .into_iter()
        .map(|entry| directory_entry(&at, &at.remote, entry))
        .collect())
}

async fn sibling_names(session: &Session, dir: &str) -> Result<HashSet<String>, ConnectError> {
    Ok(session
        .list(dir)
        .await?
        .into_iter()
        .map(|entry| entry.name)
        .collect())
}

pub async fn create_item(
    pool: &SqlitePool,
    sessions: &Sessions,
    parent_path: &str,
    kind: &str,
    name: &str,
) -> Result<DirectoryEntry, String> {
    let at = server_path(parent_path)?;
    let trimmed = name.trim();
    if trimmed.is_empty() || trimmed.contains('/') {
        return Err("Enter a name without slashes.".into());
    }
    let folder = kind == "folder";
    let session = sessions.connect(pool, &at.id, None).await?;
    let names = sibling_names(&session, &at.remote).await?;
    let target = join(&at.remote, &unique_among(&names, trimmed, folder));
    if folder {
        session.mkdir(&target).await?;
    } else {
        session.create_file(&target).await?;
    }
    let entry = session.stat(&target).await.unwrap_or(RemoteEntry {
        name: file_name(&target).to_string(),
        is_dir: folder,
        size: 0,
        modified: None,
    });
    Ok(directory_entry(&at, &at.remote, entry))
}

pub async fn rename_item(
    pool: &SqlitePool,
    sessions: &Sessions,
    path: &str,
    new_name: &str,
) -> Result<DirectoryEntry, String> {
    let at = server_path(path)?;
    let trimmed = new_name.trim();
    if trimmed.is_empty() || trimmed.contains('/') {
        return Err("Enter a name without slashes.".into());
    }
    let session = sessions.connect(pool, &at.id, None).await?;
    let dir = parent(&at.remote).to_string();
    let target = join(&dir, trimmed);
    if target != at.remote {
        if sibling_names(&session, &dir).await?.contains(trimmed) {
            return Err(format!("An item named “{trimmed}” already exists."));
        }
        session.rename(&at.remote, &target).await?;
    }
    let entry = session.stat(&target).await?;
    Ok(directory_entry(&at, &dir, entry))
}

/// Moves an item to another folder on the same server.
pub async fn move_item(
    pool: &SqlitePool,
    sessions: &Sessions,
    path: &str,
    destination: &str,
) -> Result<DirectoryEntry, String> {
    let (from, to) = (server_path(path)?, server_path(destination)?);
    if from.id != to.id {
        return Err("Items can only be moved within the same server.".into());
    }
    let session = sessions.connect(pool, &from.id, None).await?;
    let item = session.stat(&from.remote).await?;
    let names = sibling_names(&session, &to.remote).await?;
    let target = join(&to.remote, &unique_among(&names, &item.name, item.is_dir));
    session.rename(&from.remote, &target).await?;
    let entry = session.stat(&target).await?;
    Ok(directory_entry(&to, &to.remote, entry))
}

fn remove_tree(
    session: &Session,
    path: String,
    is_dir: bool,
) -> Pin<Box<dyn Future<Output = Result<(), ConnectError>> + Send + '_>> {
    Box::pin(async move {
        if !is_dir {
            return session.remove_file(&path).await;
        }
        for child in session.list(&path).await? {
            remove_tree(session, join(&path, &child.name), child.is_dir).await?;
        }
        session.remove_dir(&path).await
    })
}

pub async fn delete_items(
    pool: &SqlitePool,
    sessions: &Sessions,
    paths: &[String],
) -> Result<(), String> {
    for path in paths {
        let at = server_path(path)?;
        if at.remote == "/" {
            return Err("The server’s root folder can’t be deleted.".into());
        }
        let session = sessions.connect(pool, &at.id, None).await?;
        let item = session.stat(&at.remote).await?;
        remove_tree(&session, at.remote.clone(), item.is_dir).await?;
    }
    Ok(())
}

pub async fn file_preview(
    pool: &SqlitePool,
    sessions: &Sessions,
    path: &str,
) -> Result<FilePreview, String> {
    let at = server_path(path)?;
    let session = sessions.connect(pool, &at.id, None).await?;
    let item = session.stat(&at.remote).await?;
    let mut preview = FilePreview {
        name: if item.name.is_empty() {
            "/".into()
        } else {
            item.name.clone()
        },
        size: item.size,
        created: None,
        modified: item.modified,
        kind: PreviewKind::Directory,
        content: None,
        src: None,
        truncated: false,
    };
    if item.is_dir {
        return Ok(preview);
    }
    if let Some(kind) = Path::new(&item.name)
        .extension()
        .and_then(|extension| extension.to_str())
        .and_then(media_preview_kind)
    {
        // Media is downloaded to the cache and streamed from there by the media protocol.
        preview.kind = kind;
        return Ok(preview);
    }
    if Path::new(&item.name)
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(sheet::is_spreadsheet_extension)
    {
        // Spreadsheets are parsed on demand by `read_spreadsheet`.
        preview.kind = PreviewKind::Spreadsheet;
        return Ok(preview);
    }
    if let Some(extension) = Path::new(&item.name)
        .extension()
        .and_then(|extension| extension.to_str())
    {
        if mail::is_mail_extension(extension) {
            // Messages are parsed on demand by `open_mail`.
            preview.kind = PreviewKind::Mail;
            return Ok(preview);
        }
        if mail::is_mbox_extension(extension) {
            // Mailboxes are parsed on demand by `open_mbox`.
            preview.kind = PreviewKind::Mbox;
            return Ok(preview);
        }
        if vcard::is_vcard_extension(extension) {
            preview.kind = PreviewKind::Contact;
            return Ok(preview);
        }
        if calendar::is_calendar_extension(extension) {
            preview.kind = PreviewKind::Calendar;
            return Ok(preview);
        }
        if torrent::is_torrent_extension(extension) {
            preview.kind = PreviewKind::Torrent;
            return Ok(preview);
        }
        if notebook::is_notebook_extension(extension) {
            preview.kind = PreviewKind::Notebook;
            return Ok(preview);
        }
        if subtitle::is_subtitle_extension(extension) {
            preview.kind = PreviewKind::Subtitle;
            return Ok(preview);
        }
        if certificate::is_certificate_extension(extension) {
            preview.kind = PreviewKind::Certificate;
            return Ok(preview);
        }
        if geo::is_geo_extension(extension) {
            preview.kind = PreviewKind::Geo;
            return Ok(preview);
        }
        if fb2::is_fb2_extension(extension) {
            preview.kind = PreviewKind::Fb2;
            return Ok(preview);
        }
        if pcap::is_pcap_extension(extension) {
            preview.kind = PreviewKind::Pcap;
            return Ok(preview);
        }
        if iso::is_iso_extension(extension) {
            preview.kind = PreviewKind::Iso;
            return Ok(preview);
        }
        if msg::is_msg_extension(extension) {
            preview.kind = PreviewKind::Msg;
            return Ok(preview);
        }
        if psd::is_psd_extension(extension) {
            preview.kind = PreviewKind::Psd;
            return Ok(preview);
        }
        if dicom::is_dicom_extension(extension) {
            preview.kind = PreviewKind::Dicom;
            return Ok(preview);
        }
        if mobi::is_mobi_extension(extension) {
            preview.kind = PreviewKind::Mobi;
            return Ok(preview);
        }
        if avro::is_avro_extension(extension) {
            preview.kind = PreviewKind::Avro;
            return Ok(preview);
        }
        if parquet::is_parquet_extension(extension) {
            preview.kind = PreviewKind::Parquet;
            return Ok(preview);
        }
        if arrow::is_arrow_extension(extension) {
            preview.kind = PreviewKind::Arrow;
            return Ok(preview);
        }
    }
    // One byte past the limit tells a truncated file from one that fits exactly.
    let bytes = session.read_head(&at.remote, PREVIEW_MAX_BYTES + 1).await?;
    // PDF magic also catches PDF-based Illustrator (`.ai`) files.
    if bytes.starts_with(b"%PDF-") {
        preview.kind = PreviewKind::Pdf;
        return Ok(preview);
    }
    classify_preview_bytes(&mut preview, bytes);
    let name = preview.name.clone();
    apply_text_extension_kind(&mut preview, &name);
    Ok(preview)
}

/// Reads a whole server file (bounded by `max_bytes`) for previews that parse in memory.
pub(crate) async fn read_bytes(
    pool: &SqlitePool,
    sessions: &Sessions,
    path: &str,
    max_bytes: usize,
) -> Result<Vec<u8>, String> {
    let at = server_path(path)?;
    let session = sessions.connect(pool, &at.id, None).await?;
    let item = session.stat(&at.remote).await?;
    if item.size > max_bytes as u64 {
        return Err(format!(
            "{} is larger than {} MB",
            item.name,
            max_bytes / (1024 * 1024)
        ));
    }
    Ok(session.read_head(&at.remote, max_bytes + 1).await?)
}

/// Reads a server workbook and parses it for the spreadsheet preview.
pub(crate) async fn read_spreadsheet(
    pool: &SqlitePool,
    sessions: &Sessions,
    path: &str,
) -> Result<sheet::SpreadsheetData, String> {
    let at = server_path(path)?;
    let session = sessions.connect(pool, &at.id, None).await?;
    let item = session.stat(&at.remote).await?;
    if item.size > sheet::SHEET_MAX_BYTES as u64 {
        return Err(format!(
            "{} is larger than {} MB",
            item.name,
            sheet::SHEET_MAX_BYTES / (1024 * 1024)
        ));
    }
    let bytes = session
        .read_head(&at.remote, sheet::SHEET_MAX_BYTES + 1)
        .await?;
    sheet::parse_spreadsheet(bytes)
}

/// Local path under `root` for a server path, without `..` escapes.
fn cache_path(root: &Path, id: &str, remote: &str) -> Option<PathBuf> {
    let mut local = root.join("network").join(id);
    let mut any = false;
    for segment in remote.split('/') {
        match segment {
            "" | "." => {}
            ".." => return None,
            segment => {
                local.push(segment);
                any = true;
            }
        }
    }
    any.then_some(local)
}

/// Copies a server file into the app cache so it can open in its default app.
pub async fn download_to_cache(
    app: &AppHandle,
    pool: &SqlitePool,
    sessions: &Sessions,
    path: &str,
) -> Result<String, String> {
    download_to_cache_with_limit(app, pool, sessions, path, Some(OPEN_MAX_BYTES)).await
}

/// Copies a server file into the app cache without the "Open" size cap, so large
/// media can be streamed from the cache by the media protocol.
pub async fn download_media_to_cache(
    app: &AppHandle,
    pool: &SqlitePool,
    sessions: &Sessions,
    path: &str,
) -> Result<String, String> {
    download_to_cache_with_limit(app, pool, sessions, path, None).await
}

async fn download_to_cache_with_limit(
    app: &AppHandle,
    pool: &SqlitePool,
    sessions: &Sessions,
    path: &str,
    max_bytes: Option<u64>,
) -> Result<String, String> {
    let at = server_path(path)?;
    let root = app
        .path()
        .app_cache_dir()
        .map_err(|error| error.to_string())?;
    let local = cache_path(&root, &at.id, &at.remote).ok_or("Folders can’t be opened as files")?;
    let session = sessions.connect(pool, &at.id, None).await?;
    let item = session.stat(&at.remote).await?;
    if item.is_dir {
        return Err("Folders can’t be opened as files".into());
    }
    if let Some(max) = max_bytes.filter(|max| item.size > *max) {
        return Err(format!(
            "{} is larger than {} MB. Download it instead.",
            item.name,
            max / (1024 * 1024)
        ));
    }
    if let Some(folder) = local.parent() {
        tokio::fs::create_dir_all(folder)
            .await
            .map_err(|error| error.to_string())?;
    }
    if let Err(error) = session
        .download(&at.remote, &local, &mut Progress::silent())
        .await
    {
        let _ = tokio::fs::remove_file(&local).await;
        return Err(error.message);
    }
    Ok(local.to_string_lossy().into_owned())
}

/// One step of a transfer: create a folder, or copy a file of `size` bytes.
#[derive(Debug, PartialEq)]
struct Step {
    remote: String,
    local: PathBuf,
    is_dir: bool,
    size: u64,
}

/// Download steps for a server item into `top` (the local file or folder to create).
async fn plan_download(
    session: &Session,
    remote: &str,
    top: PathBuf,
    item: &RemoteEntry,
) -> Result<Vec<Step>, ConnectError> {
    let mut steps = vec![Step {
        remote: remote.to_string(),
        local: top.clone(),
        is_dir: item.is_dir,
        size: item.size,
    }];
    let mut folders = if item.is_dir {
        vec![(remote.to_string(), top)]
    } else {
        Vec::new()
    };
    while let Some((dir, local)) = folders.pop() {
        for child in session.list(&dir).await? {
            let remote = join(&dir, &child.name);
            let local = local.join(&child.name);
            if child.is_dir {
                folders.push((remote.clone(), local.clone()));
            }
            steps.push(Step {
                remote,
                local,
                is_dir: child.is_dir,
                size: child.size,
            });
        }
    }
    Ok(steps)
}

/// Upload steps for local sources into a server folder, with names unique among `names`.
fn plan_upload(
    sources: &[String],
    dir: &str,
    names: &mut HashSet<String>,
) -> Result<Vec<Step>, String> {
    fn walk(local: &Path, remote: &str, steps: &mut Vec<Step>) -> std::io::Result<()> {
        let mut children: Vec<_> = std::fs::read_dir(local)?.filter_map(Result::ok).collect();
        children.sort_by_key(|child| child.file_name());
        for child in children {
            let name = child.file_name().to_string_lossy().into_owned();
            if name == ".DS_Store" {
                continue;
            }
            let metadata = std::fs::metadata(child.path())?;
            let remote = join(remote, &name);
            steps.push(Step {
                remote: remote.clone(),
                local: child.path(),
                is_dir: metadata.is_dir(),
                size: if metadata.is_dir() { 0 } else { metadata.len() },
            });
            if metadata.is_dir() {
                walk(&child.path(), &remote, steps)?;
            }
        }
        Ok(())
    }

    let mut steps = Vec::new();
    for source in sources {
        let local = PathBuf::from(source);
        let metadata = std::fs::metadata(&local).map_err(|error| format!("{source}: {error}"))?;
        let base = local
            .file_name()
            .ok_or("Invalid file to upload")?
            .to_string_lossy()
            .into_owned();
        let name = unique_among(names, &base, metadata.is_dir());
        names.insert(name.clone());
        let remote = join(dir, &name);
        steps.push(Step {
            remote: remote.clone(),
            local: local.clone(),
            is_dir: metadata.is_dir(),
            size: if metadata.is_dir() { 0 } else { metadata.len() },
        });
        if metadata.is_dir() {
            walk(&local, &remote, &mut steps).map_err(|error| error.to_string())?;
        }
    }
    Ok(steps)
}

async fn run_download(
    session: &Session,
    steps: &[Step],
    progress: &mut Progress,
) -> Result<(), ConnectError> {
    for step in steps {
        if progress.cancelled() {
            break;
        }
        if step.is_dir {
            tokio::fs::create_dir_all(&step.local)
                .await
                .map_err(ConnectError::other)?;
            continue;
        }
        if let Some(folder) = step.local.parent() {
            tokio::fs::create_dir_all(folder)
                .await
                .map_err(ConnectError::other)?;
        }
        progress.state.start_download(&step.local, step.size);
        progress.start_file(file_name(&step.remote));
        if let Err(error) = session.download(&step.remote, &step.local, progress).await {
            let _ = tokio::fs::remove_file(&step.local).await;
            return Err(error);
        }
        progress.finish_file();
    }
    Ok(())
}

async fn run_upload(
    session: &Session,
    steps: &[Step],
    progress: &mut Progress,
) -> Result<(), ConnectError> {
    for step in steps {
        if progress.cancelled() {
            break;
        }
        if step.is_dir {
            session.mkdir(&step.remote).await?;
            continue;
        }
        progress.start_file(file_name(&step.remote));
        session.upload(&step.local, &step.remote, progress).await?;
        progress.finish_file();
    }
    Ok(())
}

fn files_and_bytes(steps: &[Step]) -> (u64, u64) {
    let files = steps.iter().filter(|step| !step.is_dir);
    (
        files.clone().count() as u64,
        files.map(|step| step.size).sum(),
    )
}

pub async fn upload_files(
    app: AppHandle,
    pool: &SqlitePool,
    sessions: &Sessions,
    transfers: &TransferRegistry,
    destination: String,
    sources: Vec<String>,
) -> Result<(), String> {
    let at = server_path(&destination)?;
    let session = sessions.connect(pool, &at.id, None).await?;
    let mut names = sibling_names(&session, &at.remote).await?;
    let steps = plan_upload(&sources, &at.remote, &mut names)?;
    let (files, bytes) = files_and_bytes(&steps);
    let id = uuid::Uuid::new_v4().to_string();
    let token = transfers.register(&id);
    let mut progress = Progress::new(app, token, id.clone(), "upload", destination, files, bytes);
    let result = run_upload(&session, &steps, &mut progress).await;
    match &result {
        Ok(()) => progress.finish(),
        Err(error) => progress.fail(error.message.clone()),
    }
    transfers.remove(&id);
    Ok(result?)
}

pub async fn download_items(
    app: AppHandle,
    pool: &SqlitePool,
    sessions: &Sessions,
    transfers: &TransferRegistry,
    paths: Vec<String>,
    destination: String,
) -> Result<(), String> {
    let folder = PathBuf::from(&destination);
    if !folder.is_dir() {
        return Err(format!("{} is not a folder", folder.display()));
    }
    let first = server_path(paths.first().ok_or("Nothing to download")?)?;
    let session = sessions.connect(pool, &first.id, None).await?;
    let mut steps = Vec::new();
    for path in &paths {
        let at = server_path(path)?;
        if at.id != first.id {
            return Err("Download items from one location at a time".into());
        }
        let item = session.stat(&at.remote).await?;
        let top = folder.join(crate::unique_name(&folder, &item.name));
        steps.extend(plan_download(&session, &at.remote, top, &item).await?);
    }
    let (files, bytes) = files_and_bytes(&steps);
    let id = uuid::Uuid::new_v4().to_string();
    let token = transfers.register(&id);
    let mut progress = Progress::new(
        app,
        token,
        id.clone(),
        "download",
        destination,
        files,
        bytes,
    );
    let result = run_download(&session, &steps, &mut progress).await;
    match &result {
        Ok(()) => progress.finish(),
        Err(error) => progress.fail(error.message.clone()),
    }
    transfers.remove(&id);
    Ok(result?)
}

/// Copies between a server and a local folder. Copies within a server aren't supported,
/// because neither SFTP nor FTP can copy on the server side.
pub async fn copy_item(
    pool: &SqlitePool,
    sessions: &Sessions,
    path: &str,
    destination: &str,
) -> Result<DirectoryEntry, String> {
    match (parse_server_path(path), parse_server_path(destination)) {
        (Some(_), Some(_)) => Err(
            "Copying within a server isn’t supported. Download the item and upload it again."
                .into(),
        ),
        (Some(from), None) => {
            let folder = PathBuf::from(destination);
            if !folder.is_dir() {
                return Err(format!("{} is not a folder", folder.display()));
            }
            let session = sessions.connect(pool, &from.id, None).await?;
            let item = session.stat(&from.remote).await?;
            let top = folder.join(crate::unique_name(&folder, &item.name));
            let steps = plan_download(&session, &from.remote, top.clone(), &item).await?;
            run_download(&session, &steps, &mut Progress::silent()).await?;
            crate::single_entry(&top)
        }
        (None, Some(to)) => {
            let session = sessions.connect(pool, &to.id, None).await?;
            let mut names = sibling_names(&session, &to.remote).await?;
            let steps = plan_upload(&[path.to_string()], &to.remote, &mut names)?;
            run_upload(&session, &steps, &mut Progress::silent()).await?;
            let top = steps.first().ok_or("Nothing to copy")?;
            let entry = session.stat(&top.remote).await?;
            Ok(directory_entry(&to, &to.remote, entry))
        }
        (None, None) => Err("Not a server location.".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn server_paths() {
        assert_eq!(
            parse_server_path("sftp://abc/home/pi/notes.txt"),
            Some(ServerPath {
                protocol: Protocol::Sftp,
                id: "abc".into(),
                remote: "/home/pi/notes.txt".into()
            })
        );
        assert_eq!(parse_server_path("ftp://abc/").unwrap().remote, "/");
        assert_eq!(parse_server_path("ftp://abc").unwrap().remote, "/");
        assert_eq!(parse_server_path("smb://abc/Photos"), None);
        assert!(is_server_path("sftp://abc/"));
        assert!(!is_server_path("s3://abc/"));
    }

    #[test]
    fn remote_path_helpers() {
        assert_eq!(join("/", "home"), "/home");
        assert_eq!(join("/home", "pi"), "/home/pi");
        assert_eq!(file_name("/home/pi/notes.txt"), "notes.txt");
        assert_eq!(file_name("/home/pi/"), "pi");
        assert_eq!(file_name("/"), "");
        assert_eq!(parent("/home/pi/notes.txt"), "/home/pi");
        assert_eq!(parent("/home"), "/");
        assert_eq!(parent("/"), "/");
    }

    #[test]
    fn unique_names_keep_extensions() {
        let names: HashSet<String> = ["notes.txt", "notes 2.txt", "Photos", ".env"]
            .map(String::from)
            .into();
        assert_eq!(unique_among(&names, "todo.txt", false), "todo.txt");
        assert_eq!(unique_among(&names, "notes.txt", false), "notes 3.txt");
        assert_eq!(unique_among(&names, "Photos", true), "Photos 2");
        assert_eq!(unique_among(&names, ".env", false), ".env 2");
    }

    #[test]
    fn cache_paths_stay_inside_the_cache() {
        let root = Path::new("/cache");
        assert_eq!(
            cache_path(root, "id", "/home/pi/notes.txt"),
            Some(PathBuf::from("/cache/network/id/home/pi/notes.txt"))
        );
        assert_eq!(cache_path(root, "id", "/home/../etc/passwd"), None);
        assert_eq!(cache_path(root, "id", "/"), None);
    }

    #[test]
    fn upload_plans_walk_folders_with_unique_names() {
        let base =
            std::env::temp_dir().join(format!("lite-explorer-plan-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(base.join("site/assets")).unwrap();
        std::fs::write(base.join("site/index.html"), "hi").unwrap();
        std::fs::write(base.join("site/assets/app.js"), "x").unwrap();
        std::fs::write(base.join("site/.DS_Store"), "").unwrap();

        let mut names: HashSet<String> = ["site".to_string()].into();
        let sources = [base.join("site").to_string_lossy().into_owned()];
        let steps = plan_upload(&sources, "/var/www", &mut names).unwrap();
        let summary: Vec<_> = steps
            .iter()
            .map(|step| (step.remote.as_str(), step.is_dir, step.size))
            .collect();
        assert_eq!(
            summary,
            [
                ("/var/www/site 2", true, 0),
                ("/var/www/site 2/assets", true, 0),
                ("/var/www/site 2/assets/app.js", false, 1),
                ("/var/www/site 2/index.html", false, 2),
            ]
        );
        assert_eq!(files_and_bytes(&steps), (2, 3));
        assert!(names.contains("site 2"));
        std::fs::remove_dir_all(base).unwrap();
    }
}
