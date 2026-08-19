use iec104sim_core::log_collector::LogCollector;
use iec104sim_core::slave::SlaveServer;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{watch, Mutex, RwLock};

/// Runtime state for a slave server.
pub struct SlaveServerState {
    pub server: SlaveServer,
    pub log_collector: Arc<LogCollector>,
}

/// Application state holding all active servers.
pub struct AppState {
    pub servers: RwLock<HashMap<String, SlaveServerState>>,
    pub next_server_id: RwLock<u32>,
    /// Serializes whole-workspace mutations whose effects span both the server
    /// table and the ID allocator (currently create and full config replace).
    pub workspace_mutation: Mutex<()>,
    /// Serializes automatic snapshots so an older concurrent save cannot
    /// overwrite a newer workspace state on disk.
    pub persistence_mutation: Mutex<()>,
    /// Startup restores the last workspace asynchronously. The frontend's
    /// first workspace read waits here instead of observing a transient empty
    /// server table.
    workspace_ready: watch::Sender<bool>,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            servers: RwLock::new(HashMap::new()),
            next_server_id: RwLock::new(1),
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
// DTOs for API responses
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ServerInfo {
    pub id: String,
    pub bind_address: String,
    pub port: u16,
    pub state: String,
    pub station_count: usize,
    pub client_count: usize,
    pub use_tls: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientConnectionInfo {
    pub peer_address: String,
    pub data_transfer_active: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StationInfo {
    pub common_address: u16,
    pub name: String,
    pub point_count: usize,
    /// Point totals grouped by the stable snake_case `DataCategory` key.
    /// Keeping this in the lightweight station snapshot lets the navigation
    /// tree render counts before the (potentially very large) point table is
    /// selected and loaded.
    #[serde(default)]
    pub category_counts: HashMap<String, usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataPointInfo {
    pub ioa: u32,
    pub asdu_type: String,
    pub category: String,
    pub name: String,
    pub comment: String,
    /// Explicit monitor-direction target for a control point.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mapping_common_address: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mapping_ioa: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mapping_asdu_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub command_qualifier: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub select_before_operate: Option<bool>,
    pub value: String,
    pub quality_ov: bool,
    pub quality_bl: bool,
    pub quality_sb: bool,
    pub quality_nt: bool,
    pub quality_iv: bool,
    pub timestamp: Option<String>,
}

/// Mutable runtime fields for targeted polling of active point mutations.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataPointValueSnapshot {
    pub ioa: u32,
    pub asdu_type: String,
    pub value: String,
    pub quality_ov: bool,
    pub quality_bl: bool,
    pub quality_sb: bool,
    pub quality_nt: bool,
    pub quality_iv: bool,
    pub timestamp: Option<String>,
}

/// Response for incremental data-point polling: only the points whose
/// `update_seq` exceeds the caller's `since_seq`, plus the current counter
/// and total count (the latter lets the frontend detect deletions).
#[derive(Debug, Clone, Serialize)]
pub struct IncrementalDataResponse {
    pub seq: u64,
    pub total_count: usize,
    pub points: Vec<DataPointInfo>,
}
