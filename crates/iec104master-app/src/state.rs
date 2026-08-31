use iec104sim_core::log_collector::LogCollector;
use iec104sim_core::master::{MasterConnection, MasterError};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{watch, Mutex, RwLock};

/// Runtime state for a master connection.
pub struct MasterConnectionState {
    pub connection: MasterConnection,
    pub log_collector: Arc<LogCollector>,
    /// Runtime intent, separate from the socket state; never persisted.
    /// Enabled by a successful manual connect and cleared before manual teardown.
    pub reconnect_enabled: bool,
    /// All Common Addresses (CAs) this connection talks to. Used by the
    /// Tauri layer to fan out interrogation / clock-sync / counter-read /
    /// auto-GI to every station the user configured. Always non-empty
    /// (defaults to vec![1]).
    pub common_addresses: Vec<u16>,
}

impl MasterConnectionState {
    /// Explicit user connect. A failed attempt must not re-arm a stopped connection.
    pub async fn connect(&mut self) -> Result<(), MasterError> {
        let result = self.connection.connect().await;
        if result.is_ok() {
            self.reconnect_enabled = true;
        }
        result
    }

    pub async fn disconnect(&mut self) -> Result<(), MasterError> {
        // Stop retries before closing the socket, including when EOF was already
        // observed and core disconnect returns NotConnected after cleanup.
        self.reconnect_enabled = false;
        self.connection.disconnect().await
    }
}

/// Application state holding all active master connections.
pub struct AppState {
    pub connections: RwLock<HashMap<String, MasterConnectionState>>,
    pub next_connection_id: RwLock<u32>,
    /// Serializes workspace membership changes. In particular, a full config
    /// replacement must not race a manual create that could otherwise insert
    /// itself immediately after the replacement map is committed.
    pub workspace_mutation: Mutex<()>,
    /// Serializes automatic store snapshots. Without this, two connections
    /// discovering CAs at the same time could save older snapshots out of
    /// order and lose the later workspace change on disk.
    pub persistence_mutation: Mutex<()>,
    /// Startup restores the last workspace asynchronously. Connection commands
    /// wait on this barrier so the frontend can never observe a transient empty
    /// workspace and then miss the restored connections.
    workspace_ready: watch::Sender<bool>,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            connections: RwLock::new(HashMap::new()),
            next_connection_id: RwLock::new(1),
            workspace_mutation: Mutex::new(()),
            persistence_mutation: Mutex::new(()),
            workspace_ready: watch::channel(false).0,
        }
    }
}

impl AppState {
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn wait_workspace_ready(&self) {
        let mut ready = self.workspace_ready.subscribe();
        while !*ready.borrow() {
            if ready.changed().await.is_err() {
                break;
            }
        }
    }

    pub fn mark_workspace_ready(&self) {
        self.workspace_ready.send_replace(true);
    }
}

// ---------------------------------------------------------------------------
// DTOs
// ---------------------------------------------------------------------------

// Intentionally no `Debug`: this DTO can contain a SOCKS5 password.
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ConnectionInfo {
    pub id: String,
    pub target_address: String,
    pub port: u16,
    /// All CAs configured for this connection (always non-empty).
    pub common_addresses: Vec<u16>,
    pub state: String,
    /// SOCKS5 proxy settings echoed for connection editing.
    pub use_socks5: bool,
    pub socks5_proxy_address: String,
    pub socks5_proxy_port: u16,
    pub socks5_username: String,
    pub socks5_password: String,
    pub socks5_remote_dns: bool,
    pub use_tls: bool,
    // Echo back the TLS file paths / policy so the edit dialog can pre-fill
    // from the connection itself (the authoritative source) instead of a
    // shared localStorage blob — otherwise edits to a cert path silently
    // revert on reopen and multiple connections clobber each other.
    #[serde(default)]
    pub ca_file: String,
    #[serde(default)]
    pub cert_file: String,
    #[serde(default)]
    pub key_file: String,
    #[serde(default)]
    pub accept_invalid_certs: bool,
    /// "auto" | "tls12_only" | "tls13_only" (matches the frontend select).
    #[serde(default)]
    pub tls_version: String,
    // Echo back the protocol parameters so the frontend can pre-fill the
    // edit dialog without re-parsing the persisted form state.
    pub t0: u32,
    pub channel_retry_s: u32,
    pub t1: u32,
    pub t2: u32,
    pub t3: u32,
    pub k: u16,
    pub w: u16,
    pub default_qoi: u8,
    pub default_qcc: u8,
    pub interrogate_period_s: u32,
    pub counter_interrogate_period_s: u32,
    /// Timing fields auto-corrected during creation/import so they satisfy
    /// the IEC 104 relationship invariants. Empty ⇒ the supplied config was
    /// already valid. The frontend surfaces these to the user.
    #[serde(default)]
    pub timing_corrections: Vec<iec104sim_core::timing::TimingCorrection>,
    /// 广播公共地址,用于 GI/CI 广播帧(默认 0xFFFF)。
    pub broadcast_address: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReceivedDataPointInfo {
    pub ioa: u32,
    /// Common Address of the station that sourced this point. Required by
    /// the frontend so the tree can group "connection → CA → category" and
    /// so right-click control commands target the correct station.
    pub common_address: u16,
    pub asdu_type: String,
    /// Numeric IEC 60870-5-101/104 TypeID for `asdu_type` (e.g. M_SP_NA_1 → 1).
    /// Shown next to the type name in the data table and detail panel.
    pub asdu_type_id: u8,
    pub category: String,
    pub value: String,
    pub quality_ov: bool,
    pub quality_bl: bool,
    pub quality_sb: bool,
    pub quality_nt: bool,
    pub quality_iv: bool,
    pub timestamp: Option<String>,
    pub update_seq: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IncrementalDataResponse {
    pub seq: u64,
    pub total_count: usize,
    pub points: Vec<ReceivedDataPointInfo>,
}
