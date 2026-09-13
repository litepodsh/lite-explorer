//! Finds SMB servers on the local network: Bonjour (mDNS `_smb._tcp`) gives names for Macs
//! and NAS boxes, and a port 445 sweep of the local /24 finds Windows PCs, which don't
//! announce themselves over mDNS.
//!
//! The first scan triggers the system's network permission: the Local Network prompt on
//! macOS (declared in Info.plist), the firewall prompt on Windows. Linux needs none.

use std::{
    collections::HashSet,
    net::{IpAddr, Ipv4Addr},
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};

use mdns_sd::{ServiceDaemon, ServiceEvent};
use serde::Serialize;
use tauri::{AppHandle, Emitter, State};
use tokio::{net::TcpStream, sync::Semaphore, task::JoinSet};
use tokio_util::sync::CancellationToken;

use super::ConnectError;

const SMB_SERVICE: &str = "_smb._tcp.local.";
const SMB_PORT: u16 = 445;
const BROWSE_FOR: Duration = Duration::from_secs(4);
const PROBE_TIMEOUT: Duration = Duration::from_millis(600);
const PROBES_AT_ONCE: usize = 64;
/// macOS answers "No route to host" right away when Local Network access is denied.
#[cfg(target_os = "macos")]
const BLOCKED_WITHIN: Duration = Duration::from_millis(80);
#[cfg(target_os = "macos")]
const EHOSTUNREACH: i32 = 65;

/// The scan that is running, so a new scan or closing the dialog can stop it.
#[derive(Default)]
pub struct Discovery(Mutex<Option<CancellationToken>>);

#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DiscoveredServer {
    scan: u64,
    name: String,
    /// Host name from Bonjour (`nas.local`), or the IP address from the sweep.
    host: String,
    addresses: Vec<String>,
    port: u16,
    source: &'static str,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ScanFinished {
    scan: u64,
    permission_denied: bool,
}

/// Instance name from a Bonjour full name: `Garzon NAS._smb._tcp.local.` → `Garzon NAS`.
fn instance_name(fullname: &str, service: &str) -> String {
    let name = fullname.strip_suffix(service).unwrap_or(fullname);
    name.trim_end_matches('.')
        .replace("\\032", " ")
        .replace("\\ ", " ")
}

fn is_private(ip: Ipv4Addr) -> bool {
    ip.is_private() && !ip.is_loopback()
}

/// Addresses to probe around one interface: its /24 (or smaller subnet), without the
/// network, broadcast and own addresses. Larger subnets are capped to the /24 around `ip`
/// so a scan stays under 254 probes per interface.
fn sweep_hosts(ip: Ipv4Addr, prefix: u8) -> Vec<Ipv4Addr> {
    if !is_private(ip) || prefix > 30 {
        return Vec::new();
    }
    let prefix = prefix.max(24);
    let mask = u32::MAX << (32 - prefix);
    let network = u32::from(ip) & mask;
    let broadcast = network | !mask;
    (network + 1..broadcast)
        .map(Ipv4Addr::from)
        .filter(|host| *host != ip)
        .collect()
}

/// True when nearly every probe failed instantly with "No route to host" and none
/// connected: the pattern of macOS Local Network access being denied.
fn looks_blocked(probes: usize, open: usize, blocked: usize) -> bool {
    cfg!(target_os = "macos") && probes > 0 && open == 0 && blocked * 10 >= probes * 9
}

fn local_hosts() -> Vec<Ipv4Addr> {
    let Ok(interfaces) = if_addrs::get_if_addrs() else {
        return Vec::new();
    };
    let mut seen = HashSet::new();
    interfaces
        .into_iter()
        .filter(|interface| !interface.is_loopback())
        .filter_map(|interface| match interface.addr {
            if_addrs::IfAddr::V4(v4) => Some(sweep_hosts(v4.ip, v4.prefixlen)),
            _ => None,
        })
        .flatten()
        .filter(|host| seen.insert(*host))
        .collect()
}

async fn browse(app: AppHandle, scan: u64, token: CancellationToken) {
    let Ok(daemon) = ServiceDaemon::new() else {
        return;
    };
    let Ok(receiver) = daemon.browse(SMB_SERVICE) else {
        let _ = daemon.shutdown();
        return;
    };
    let deadline = tokio::time::Instant::now() + BROWSE_FOR;
    loop {
        tokio::select! {
            _ = token.cancelled() => break,
            event = tokio::time::timeout_at(deadline, receiver.recv_async()) => match event {
                Ok(Ok(ServiceEvent::ServiceResolved(info))) => {
                    let addresses: Vec<String> = info
                        .get_addresses_v4()
                        .into_iter()
                        .map(|address| address.to_string())
                        .collect();
                    let host = info.get_hostname().trim_end_matches('.').to_string();
                    let _ = app.emit(
                        "network-server",
                        DiscoveredServer {
                            scan,
                            name: instance_name(info.get_fullname(), SMB_SERVICE),
                            host: if host.is_empty() { addresses.first().cloned().unwrap_or_default() } else { host },
                            addresses,
                            port: info.get_port(),
                            source: "bonjour",
                        },
                    );
                }
                Ok(Ok(_)) => {}
                _ => break,
            },
        }
    }
    let _ = daemon.stop_browse(SMB_SERVICE);
    let _ = daemon.shutdown();
}

/// Probes port 445 on the local subnets. Returns whether access looks blocked by the system.
async fn sweep(app: AppHandle, scan: u64, token: CancellationToken) -> bool {
    let hosts = tauri::async_runtime::spawn_blocking(local_hosts)
        .await
        .unwrap_or_default();
    let probes = hosts.len();
    let open = Arc::new(AtomicUsize::new(0));
    let blocked = Arc::new(AtomicUsize::new(0));
    let limit = Arc::new(Semaphore::new(PROBES_AT_ONCE));
    let mut tasks = JoinSet::new();

    for host in hosts {
        let (app, token, open, blocked, limit) = (
            app.clone(),
            token.clone(),
            open.clone(),
            blocked.clone(),
            limit.clone(),
        );
        tasks.spawn(async move {
            let Ok(_permit) = limit.acquire().await else {
                return;
            };
            if token.is_cancelled() {
                return;
            }
            let started = Instant::now();
            let attempt = tokio::time::timeout(
                PROBE_TIMEOUT,
                TcpStream::connect((IpAddr::V4(host), SMB_PORT)),
            );
            match attempt.await {
                Ok(Ok(_)) => {
                    open.fetch_add(1, Ordering::Relaxed);
                    let _ = app.emit(
                        "network-server",
                        DiscoveredServer {
                            scan,
                            name: host.to_string(),
                            host: host.to_string(),
                            addresses: vec![host.to_string()],
                            port: SMB_PORT,
                            source: "scan",
                        },
                    );
                }
                #[cfg(target_os = "macos")]
                Ok(Err(error))
                    if error.raw_os_error() == Some(EHOSTUNREACH)
                        && started.elapsed() < BLOCKED_WITHIN =>
                {
                    blocked.fetch_add(1, Ordering::Relaxed);
                }
                _ => {
                    let _ = (started, &blocked);
                }
            }
        });
    }
    tokio::select! {
        _ = token.cancelled() => tasks.abort_all(),
        _ = async { while tasks.join_next().await.is_some() {} } => {}
    }
    looks_blocked(
        probes,
        open.load(Ordering::Relaxed),
        blocked.load(Ordering::Relaxed),
    )
}

/// Scans for SMB servers for a few seconds. Results arrive as `network-server` events and
/// the end as `network-scan-finished`, both tagged with `scan` so stale results are ignored.
#[tauri::command]
pub async fn scan_smb_servers(
    app: AppHandle,
    discovery: State<'_, Discovery>,
    scan: u64,
) -> Result<(), ConnectError> {
    let token = CancellationToken::new();
    if let Some(previous) = discovery.0.lock().unwrap().replace(token.clone()) {
        previous.cancel();
    }
    let (_, permission_denied) = tokio::join!(
        browse(app.clone(), scan, token.clone()),
        sweep(app.clone(), scan, token.clone())
    );
    let _ = app.emit(
        "network-scan-finished",
        ScanFinished {
            scan,
            permission_denied: permission_denied && !token.is_cancelled(),
        },
    );
    Ok(())
}

#[tauri::command]
pub fn stop_network_scan(discovery: State<'_, Discovery>) {
    if let Some(token) = discovery.0.lock().unwrap().take() {
        token.cancel();
    }
}

/// Opens the system page where local network access is granted.
#[tauri::command]
pub fn open_local_network_settings() -> Result<(), ConnectError> {
    #[cfg(target_os = "macos")]
    std::process::Command::new("open")
        .arg("x-apple.systempreferences:com.apple.preference.security?Privacy_LocalNetwork")
        .spawn()
        .map_err(ConnectError::other)?;
    #[cfg(target_os = "windows")]
    std::process::Command::new("control")
        .arg("firewall.cpl")
        .spawn()
        .map_err(ConnectError::other)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bonjour_instance_names() {
        assert_eq!(
            instance_name("Garzon NAS._smb._tcp.local.", SMB_SERVICE),
            "Garzon NAS"
        );
        assert_eq!(
            instance_name("Studio\\032iMac._smb._tcp.local.", SMB_SERVICE),
            "Studio iMac"
        );
        assert_eq!(instance_name("odd", SMB_SERVICE), "odd");
    }

    #[test]
    fn sweep_covers_the_local_24_only() {
        let hosts = sweep_hosts(Ipv4Addr::new(192, 168, 1, 20), 24);
        assert_eq!(hosts.len(), 253);
        assert_eq!(hosts.first(), Some(&Ipv4Addr::new(192, 168, 1, 1)));
        assert_eq!(hosts.last(), Some(&Ipv4Addr::new(192, 168, 1, 254)));
        assert!(!hosts.contains(&Ipv4Addr::new(192, 168, 1, 20)));

        let wide = sweep_hosts(Ipv4Addr::new(10, 0, 7, 9), 16);
        assert_eq!(wide.len(), 253);
        assert_eq!(wide.first(), Some(&Ipv4Addr::new(10, 0, 7, 1)));

        let small = sweep_hosts(Ipv4Addr::new(172, 16, 0, 5), 29);
        assert_eq!(
            small,
            [1, 2, 3, 4, 6].map(|last| Ipv4Addr::new(172, 16, 0, last))
        );

        assert!(sweep_hosts(Ipv4Addr::new(8, 8, 8, 8), 24).is_empty());
        assert!(sweep_hosts(Ipv4Addr::new(127, 0, 0, 1), 8).is_empty());
        assert!(sweep_hosts(Ipv4Addr::new(192, 168, 1, 20), 32).is_empty());
    }

    #[test]
    fn blocked_needs_instant_failures_and_no_open_hosts() {
        let macos = cfg!(target_os = "macos");
        assert_eq!(looks_blocked(253, 0, 253), macos);
        assert_eq!(looks_blocked(253, 0, 230), macos);
        assert!(!looks_blocked(253, 1, 252));
        assert!(!looks_blocked(253, 0, 10));
        assert!(!looks_blocked(0, 0, 0));
    }
}
