use crate::state::{AppState, ConnectionInfo, IncrementalDataResponse, MasterConnectionState, ReceivedDataPointInfo};
use iec104sim_core::log_collector::LogCollector;
use iec104sim_core::log_entry::LogEntry;
use iec104sim_core::master::{ControlResult, ControlStep, MasterConfig, MasterConnection, TlsConfig, TlsVersionPolicy};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tauri::{AppHandle, Emitter, State};

// ---------------------------------------------------------------------------
// Event Payloads
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct ConnectionStateEvent {
    pub id: String,
    pub state: String,
}

// ---------------------------------------------------------------------------
// Connection Commands
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct CreateConnectionRequest {
    pub target_address: String,
    pub port: u16,
    /// All Common Addresses to talk to over this connection. If absent or
    /// empty, falls back to `[common_address]` (legacy single-CA field) or
    /// finally `[1]`.
    #[serde(default)]
    pub common_addresses: Option<Vec<u16>>,
    /// Legacy single-CA field. Kept for backward compatibility with older
    /// frontend builds; ignored when `common_addresses` is non-empty.
    pub common_address: Option<u16>,
    pub timeout_ms: Option<u64>,
    /// TLS configuration
    pub use_tls: Option<bool>,
    pub ca_file: Option<String>,
    pub cert_file: Option<String>,
    pub key_file: Option<String>,
    pub accept_invalid_certs: Option<bool>,
    /// TLS version policy: "auto" | "tls12_only" | "tls13_only" (default: "auto")
    pub tls_version: Option<String>,
    // ---- IEC 60870-5-104 protocol parameters (all optional; defaults from
    //      MasterConfig when absent). Frontend sends these as JSON numbers. ----
    pub t0: Option<u32>,
    pub t1: Option<u32>,
    pub t2: Option<u32>,
    pub t3: Option<u32>,
    pub k: Option<u16>,
    pub w: Option<u16>,
    /// QOI for general interrogation (1..=255). 20 = global station.
    pub default_qoi: Option<u8>,
    /// QCC for counter interrogation (1..=255). 5 = total + no freeze.
    pub default_qcc: Option<u8>,
    /// Period (s) for auto general interrogation. 0 disables.
    pub interrogate_period_s: Option<u32>,
    /// Period (s) for auto counter interrogation. 0 disables.
    pub counter_interrogate_period_s: Option<u32>,
    /// 广播公共地址(默认 0xFFFF;0xFF00 是某些方言)。
    pub broadcast_address: Option<u16>,
}

impl CreateConnectionRequest {
    /// Resolve the final list of CAs from the request, applying backward-compat
    /// rules. Always returns at least one element.
    fn resolve_cas(&self) -> Vec<u16> {
        if let Some(list) = &self.common_addresses {
            if !list.is_empty() {
                return list.clone();
            }
        }
        vec![self.common_address.unwrap_or(1)]
    }
}

/// Map the core TLS version policy to the string the frontend `<select>` uses.
fn tls_version_str(p: TlsVersionPolicy) -> &'static str {
    match p {
        TlsVersionPolicy::Auto => "auto",
        TlsVersionPolicy::Tls12Only => "tls12_only",
        TlsVersionPolicy::Tls13Only => "tls13_only",
    }
}

impl ConnectionInfo {
    /// Single source of truth for building the DTO from a live connection's
    /// config — used by both `create_connection` and `list_connections` so
    /// the TLS paths/policy are echoed identically. Without this, the edit
    /// dialog had no way to read a connection's real cert paths and fell back
    /// to a shared localStorage blob, making cert-path edits appear to revert.
    fn from_config(
        id: String,
        state: String,
        common_addresses: Vec<u16>,
        cfg: &MasterConfig,
        timing_corrections: Vec<iec104sim_core::timing::TimingCorrection>,
    ) -> Self {
        ConnectionInfo {
            id,
            target_address: cfg.target_address.clone(),
            port: cfg.port,
            common_addresses,
            state,
            use_tls: cfg.tls.enabled,
            ca_file: cfg.tls.ca_file.clone(),
            cert_file: cfg.tls.cert_file.clone(),
            key_file: cfg.tls.key_file.clone(),
            accept_invalid_certs: cfg.tls.accept_invalid_certs,
            tls_version: tls_version_str(cfg.tls.version).to_string(),
            t0: cfg.t0,
            t1: cfg.t1,
            t2: cfg.t2,
            t3: cfg.t3,
            k: cfg.k,
            w: cfg.w,
            default_qoi: cfg.default_qoi,
            default_qcc: cfg.default_qcc,
            interrogate_period_s: cfg.interrogate_period_s,
            counter_interrogate_period_s: cfg.counter_interrogate_period_s,
            timing_corrections,
            broadcast_address: cfg.broadcast_address,
        }
    }
}

#[tauri::command]
pub async fn create_connection(
    state: State<'_, AppState>,
    app_handle: AppHandle,
    request: CreateConnectionRequest,
) -> Result<ConnectionInfo, String> {
    let id = {
        let mut counter = state.next_connection_id.write().await;
        let id = format!("conn_{}", *counter);
        *counter += 1;
        id
    };

    let common_addresses = request.resolve_cas();

    let (ca_inbox, mut flush_rx, _debouncer_handle) =
        // 安静期 1s:Goldwind 现场 GI 应答帧通常在 ~100ms 内全部到达,
        // 1s 足以聚批且让用户感觉"按一下即响应"。原 3s 在现场显得卡顿。
        iec104sim_core::ca_debouncer::spawn(std::time::Duration::from_millis(1000));

    let mut config = MasterConfig {
        target_address: request.target_address.clone(),
        port: request.port,
        // Core's MasterConfig still tracks a single "primary" CA used for
        // identification/defaults inside the protocol layer. Multi-CA fan-out
        // happens at this app's command layer, so keep the first as primary.
        common_address: common_addresses[0],
        timeout_ms: request.timeout_ms.unwrap_or(3000),
        tls: TlsConfig {
            enabled: request.use_tls.unwrap_or(false),
            ca_file: request.ca_file.unwrap_or_default(),
            cert_file: request.cert_file.unwrap_or_default(),
            key_file: request.key_file.unwrap_or_default(),
            pkcs12_file: String::new(),
            pkcs12_password: String::new(),
            accept_invalid_certs: request.accept_invalid_certs.unwrap_or(false),
            version: match request.tls_version.as_deref() {
                Some("tls12_only") => TlsVersionPolicy::Tls12Only,
                Some("tls13_only") => TlsVersionPolicy::Tls13Only,
                _ => TlsVersionPolicy::Auto,
            },
        },
        ..MasterConfig::default()
    };
    // Override the per-protocol params from the request when supplied.
    if let Some(v) = request.t0 { config.t0 = v; }
    if let Some(v) = request.t1 { config.t1 = v; }
    if let Some(v) = request.t2 { config.t2 = v; }
    if let Some(v) = request.t3 { config.t3 = v; }
    if let Some(v) = request.k { config.k = v; }
    if let Some(v) = request.w { config.w = v; }
    if let Some(v) = request.default_qoi { config.default_qoi = v; }
    if let Some(v) = request.default_qcc { config.default_qcc = v; }
    if let Some(v) = request.interrogate_period_s { config.interrogate_period_s = v; }
    if let Some(v) = request.counter_interrogate_period_s { config.counter_interrogate_period_s = v; }
    if let Some(bcast) = request.broadcast_address { config.broadcast_address = bcast; }

    // Authoritative timing normalization: enforce t2<t1<t3 and w≤⌊2k/3⌋ before
    // the config takes effect. Covers direct creation, load_config and import
    // (both funnel through here). Corrections are echoed back to the frontend.
    let timing_corrections = config.normalize_timing();

    let log_collector = Arc::new(LogCollector::new());
    // 默认关闭,LogPanel 展开时由前端通过 set_logging_enabled 打开。
    log_collector.set_enabled(false);
    let connection = MasterConnection::new(config.clone())
        .with_log_collector(log_collector.clone())
        .with_ca_inbox(ca_inbox);
    connection.set_configured_cas(common_addresses.clone());

    // Forward state-change notifications from the core connection to the frontend.
    // Exits when the connection is dropped (`delete_connection`) and its `state_tx` closes.
    let mut state_rx = connection.subscribe_state();
    let id_for_task = id.clone();
    let app_handle_for_task = app_handle.clone();
    tokio::spawn(async move {
        while state_rx.changed().await.is_ok() {
            let new_state = *state_rx.borrow_and_update();
            let _ = app_handle_for_task.emit(
                "connection-state",
                ConnectionStateEvent {
                    id: id_for_task.clone(),
                    state: format!("{:?}", new_state),
                },
            );
        }
    });

    let info = ConnectionInfo::from_config(
        id.clone(),
        format!("{:?}", connection.state()),
        common_addresses.clone(),
        &config,
        timing_corrections,
    );

    state.connections.write().await.insert(
        id.clone(),
        MasterConnectionState {
            connection,
            log_collector,
            common_addresses,
        },
    );

    // Forward flush events from the ca_debouncer to the frontend.
    {
        let app = app_handle.clone();
        let id_clone = id.clone();
        tokio::spawn(async move {
            use tauri::Manager;
            while let Some(ev) = flush_rx.recv().await {
                let state: State<'_, AppState> = app.state();
                let (added, all_cas) = {
                    // 单次 write guard:同时扩 MasterConnection.configured_cas(供广播过滤)
                    // 与 MasterConnectionState.common_addresses(供 list_connections 暴露给前端)。
                    // 两边必须同步,否则 list_connections 不会看到新学到的 CA,前端连接树不刷出新节点。
                    let mut guard = state.connections.write().await;
                    let Some(c) = guard.get_mut(&id_clone) else { break };
                    let added = c.connection.extend_configured_cas(&ev.new_cas);
                    if !added.is_empty() {
                        c.common_addresses.extend(added.iter().copied());
                    }
                    let all_cas = if added.is_empty() { Vec::new() } else { c.connection.configured_cas() };
                    (added, all_cas)
                };
                if !added.is_empty() {
                    let payload = serde_json::json!({
                        "id": id_clone,
                        "common_addresses": all_cas,
                        "added": added,
                    });
                    let _ = app.emit("connection-cas-updated", payload);
                }
            }
        });
    }

    // _debouncer_handle detaches here; its lifetime is tied to ca_inbox which
    // is held by the MasterConnection inside connections map.
    drop(_debouncer_handle);

    Ok(info)
}

// `connection-state` events are emitted by the watcher spawned in
// `create_connection`, driven by the core's state channel. These commands
// therefore do not need to emit manually.

#[tauri::command]
pub async fn connect_master(
    state: State<'_, AppState>,
    id: String,
) -> Result<(), String> {
    let mut connections = state.connections.write().await;
    let conn = connections
        .get_mut(&id)
        .ok_or_else(|| format!("connection {} not found", id))?;

    conn.connection
        .connect()
        .await
        .map_err(|e| format!("failed to connect: {}", e))
}

#[tauri::command]
pub async fn disconnect_master(
    state: State<'_, AppState>,
    id: String,
) -> Result<(), String> {
    let mut connections = state.connections.write().await;
    let conn = connections
        .get_mut(&id)
        .ok_or_else(|| format!("connection {} not found", id))?;

    conn.connection
        .disconnect()
        .await
        .map_err(|e| format!("failed to disconnect: {}", e))
}

#[tauri::command]
pub async fn delete_connection(
    state: State<'_, AppState>,
    id: String,
) -> Result<(), String> {
    let mut conn_state = {
        let mut connections = state.connections.write().await;
        connections
            .remove(&id)
            .ok_or_else(|| format!("connection {} not found", id))?
    };
    // Disconnect + drop the per-connection caches (15k+ point HashMap, log
    // buffer, receiver task) off the Tauri command thread. disconnect() has a
    // 2s internal timeout, so the spawned task can't leak.
    tokio::spawn(async move {
        let _ = conn_state.connection.disconnect().await;
    });
    Ok(())
}

#[tauri::command]
pub async fn list_connections(
    state: State<'_, AppState>,
) -> Result<Vec<ConnectionInfo>, String> {
    let connections = state.connections.read().await;
    let mut result = Vec::new();

    for (id, conn_state) in connections.iter() {
        // Live connections already hold normalized config; no timing
        // corrections to report on a steady-state list.
        result.push(ConnectionInfo::from_config(
            id.clone(),
            format!("{:?}", conn_state.connection.state()),
            conn_state.common_addresses.clone(),
            &conn_state.connection.config,
            Vec::new(),
        ));
    }

    Ok(result)
}

// ---------------------------------------------------------------------------
// IEC 104 Commands
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn send_interrogation(
    state: State<'_, AppState>,
    id: String,
    common_address: u16,
) -> Result<(), String> {
    let connections = state.connections.read().await;
    let conn = connections
        .get(&id)
        .ok_or_else(|| format!("connection {} not found", id))?;

    conn.connection
        .send_interrogation(common_address)
        .await
        .map_err(|e| format!("failed to send GI: {}", e))
}

#[tauri::command]
pub async fn send_clock_sync(
    state: State<'_, AppState>,
    id: String,
    common_address: u16,
) -> Result<(), String> {
    let connections = state.connections.read().await;
    let conn = connections
        .get(&id)
        .ok_or_else(|| format!("connection {} not found", id))?;

    conn.connection
        .send_clock_sync(common_address)
        .await
        .map_err(|e| format!("failed to send clock sync: {}", e))
}

#[tauri::command]
pub async fn send_counter_read(
    state: State<'_, AppState>,
    id: String,
    common_address: u16,
) -> Result<(), String> {
    let connections = state.connections.read().await;
    let conn = connections
        .get(&id)
        .ok_or_else(|| format!("connection {} not found", id))?;

    conn.connection
        .send_counter_read(common_address)
        .await
        .map_err(|e| format!("failed to send counter read: {}", e))
}

#[tauri::command]
pub async fn send_broadcast_gi(
    state: State<'_, AppState>,
    id: String,
) -> Result<(), String> {
    let connections = state.connections.read().await;
    let conn = connections
        .get(&id)
        .ok_or_else(|| format!("connection {} not found", id))?;
    let bcast = conn.connection.config().broadcast_address;
    conn.connection
        .send_interrogation(bcast)
        .await
        .map_err(|e| format!("failed to send broadcast GI: {}", e))
}

#[tauri::command]
pub async fn send_broadcast_clock_sync(
    state: State<'_, AppState>,
    id: String,
) -> Result<(), String> {
    let connections = state.connections.read().await;
    let conn = connections
        .get(&id)
        .ok_or_else(|| format!("connection {} not found", id))?;
    let bcast = conn.connection.config().broadcast_address;
    conn.connection
        .send_clock_sync(bcast)
        .await
        .map_err(|e| format!("failed to send broadcast clock sync: {}", e))
}

#[tauri::command]
pub async fn send_broadcast_counter_read(
    state: State<'_, AppState>,
    id: String,
) -> Result<(), String> {
    let connections = state.connections.read().await;
    let conn = connections
        .get(&id)
        .ok_or_else(|| format!("connection {} not found", id))?;
    let bcast = conn.connection.config().broadcast_address;
    conn.connection
        .send_counter_read(bcast)
        .await
        .map_err(|e| format!("failed to send broadcast counter read: {}", e))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ControlCommandRequest {
    pub connection_id: String,
    pub ioa: u32,
    pub common_address: u16,
    pub command_type: String,
    pub value: String,
    pub select: Option<bool>,
    /// QU (single/double/step, occupies bits 2..6 of the command byte) or QL (setpoint, bits 0..6 of QOS).
    /// Bitstring(51) ignores this field.
    pub qualifier: Option<u8>,
    /// Cause Of Transmission. Defaults to 6 (Activation).
    pub cot: Option<u8>,
    /// 32-bit payload for C_BO_NA_1 (51). Required when command_type == "bitstring".
    pub bitstring: Option<u32>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct RawApduRequest {
    pub connection_id: String,
    pub hex_payload: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct RawSendResult {
    pub sent_hex: String,
    pub byte_len: usize,
    pub timestamp: String,
}

fn default_qualifier(command_type: &str) -> u8 {
    // 0 means "no additional definition" for QU, and "default" for QL.
    let _ = command_type;
    0
}

#[tauri::command]
pub async fn send_control_command(
    state: State<'_, AppState>,
    request: ControlCommandRequest,
) -> Result<ControlResult, String> {
    let t0 = std::time::Instant::now();
    let connections = state.connections.read().await;
    let t_lock = t0.elapsed();
    let conn = connections
        .get(&request.connection_id)
        .ok_or_else(|| format!("connection {} not found", request.connection_id))?;

    let select = request.select.unwrap_or(false);
    let ca = request.common_address;
    let ioa = request.ioa;
    let qu = request.qualifier.unwrap_or_else(|| default_qualifier(&request.command_type));
    let cot = request.cot.unwrap_or(6);

    eprintln!(
        "[send_control_command] enter type={} ioa={} ca={} select={} | connections_read_lock={}ms",
        request.command_type, ioa, ca, select, t_lock.as_millis()
    );

    // Direct execute: send command and return immediately
    if !select {
        let start = std::time::Instant::now();
        match request.command_type.as_str() {
            "single" => {
                let value = parse_bool(&request.value)?;
                conn.connection.send_single_command(ioa, value, false, ca, qu, cot).await
                    .map_err(|e| format!("failed to send command: {}", e))?;
            }
            "double" => {
                let value = request.value.parse::<u8>().map_err(|e| format!("{}", e))?;
                conn.connection.send_double_command(ioa, value, false, ca, qu, cot).await
                    .map_err(|e| format!("failed to send command: {}", e))?;
            }
            "step" => {
                let value = request.value.parse::<u8>().map_err(|e| format!("{}", e))?;
                conn.connection.send_step_command(ioa, value, false, ca, qu, cot).await
                    .map_err(|e| format!("failed to send command: {}", e))?;
            }
            "setpoint_normalized" => {
                let value = request.value.parse::<i16>().map_err(|e| format!("{}", e))?;
                conn.connection.send_setpoint_normalized(ioa, value, false, ca, qu, cot).await
                    .map_err(|e| format!("failed to send command: {}", e))?;
            }
            "setpoint_scaled" => {
                let value = request.value.parse::<i16>().map_err(|e| format!("{}", e))?;
                conn.connection.send_setpoint_scaled(ioa, value, false, ca, qu, cot).await
                    .map_err(|e| format!("failed to send command: {}", e))?;
            }
            "setpoint_float" => {
                let value = request.value.parse::<f32>().map_err(|e| format!("{}", e))?;
                let t_send = std::time::Instant::now();
                conn.connection.send_setpoint_float(ioa, value, false, ca, qu, cot).await
                    .map_err(|e| format!("failed to send command: {}", e))?;
                eprintln!("[send_control_command] setpoint_float send_frame={}ms", t_send.elapsed().as_millis());
            }
            "bitstring" => {
                let value = request.bitstring
                    .or_else(|| parse_u32_value(&request.value))
                    .ok_or_else(|| "bitstring 命令需要提供 32 位数值 (bitstring 字段或 value)".to_string())?;
                conn.connection.send_bitstring_command(ioa, value, ca, cot).await
                    .map_err(|e| format!("failed to send command: {}", e))?;
            }
            _ => return Err(format!("unknown command type: {}", request.command_type)),
        }
        return Ok(ControlResult {
            steps: vec![ControlStep {
                action: "execute_sent".to_string(),
                timestamp: chrono::Local::now().format("%H:%M:%S%.3f").to_string(),
            }],
            duration_ms: start.elapsed().as_millis() as u64,
        });
    }

    // SbO mode: delegate to send_control_with_sbo_event
    use iec104sim_core::log_entry::{DetailEvent, FrameLabel};

    match request.command_type.as_str() {
        "single" => {
            let value = parse_bool(&request.value)?;
            let select_frame = build_control_frames_single(ca, ioa, value, true, qu, cot);
            let execute_frame = build_control_frames_single(ca, ioa, value, false, qu, cot);
            let event = DetailEvent {
                kind: "single_command".to_string(),
                payload: serde_json::json!({ "ioa": ioa, "val": value, "qu": qu, "cot": cot }),
            };
            conn.connection.send_control_with_sbo_event(
                select_frame, execute_frame, ioa,
                &format!("单点命令 IOA={} val={} QU={} COT={}", ioa, value, qu, cot),
                FrameLabel::SingleCommand, ca, Some(event),
            ).await.map_err(|e| format!("{}", e))
        }
        "double" => {
            let value = request.value.parse::<u8>().map_err(|e| format!("{}", e))?;
            let select_frame = build_control_frames_double(ca, ioa, value, true, qu, cot);
            let execute_frame = build_control_frames_double(ca, ioa, value, false, qu, cot);
            let event = DetailEvent {
                kind: "double_command".to_string(),
                payload: serde_json::json!({ "ioa": ioa, "val": value, "qu": qu, "cot": cot }),
            };
            conn.connection.send_control_with_sbo_event(
                select_frame, execute_frame, ioa,
                &format!("双点命令 IOA={} val={} QU={} COT={}", ioa, value, qu, cot),
                FrameLabel::DoubleCommand, ca, Some(event),
            ).await.map_err(|e| format!("{}", e))
        }
        "step" => {
            let value = request.value.parse::<u8>().map_err(|e| format!("{}", e))?;
            let select_frame = build_control_frames_step(ca, ioa, value, true, qu, cot);
            let execute_frame = build_control_frames_step(ca, ioa, value, false, qu, cot);
            let event = DetailEvent {
                kind: "step_command".to_string(),
                payload: serde_json::json!({ "ioa": ioa, "val": value, "qu": qu, "cot": cot }),
            };
            conn.connection.send_control_with_sbo_event(
                select_frame, execute_frame, ioa,
                &format!("步调节命令 IOA={} val={} QU={} COT={}", ioa, value, qu, cot),
                FrameLabel::StepCommand, ca, Some(event),
            ).await.map_err(|e| format!("{}", e))
        }
        "setpoint_normalized" => {
            let value = request.value.parse::<i16>().map_err(|e| format!("{}", e))?;
            let select_frame = build_control_frames_setpoint_norm(ca, ioa, value, true, qu, cot);
            let execute_frame = build_control_frames_setpoint_norm(ca, ioa, value, false, qu, cot);
            let event = DetailEvent {
                kind: "setpoint_normalized".to_string(),
                payload: serde_json::json!({ "ioa": ioa, "val": value, "ql": qu, "cot": cot }),
            };
            conn.connection.send_control_with_sbo_event(
                select_frame, execute_frame, ioa,
                &format!("归一化设定值 IOA={} val={} QL={} COT={}", ioa, value, qu, cot),
                FrameLabel::SetpointNormalized, ca, Some(event),
            ).await.map_err(|e| format!("{}", e))
        }
        "setpoint_scaled" => {
            let value = request.value.parse::<i16>().map_err(|e| format!("{}", e))?;
            let select_frame = build_control_frames_setpoint_scaled(ca, ioa, value, true, qu, cot);
            let execute_frame = build_control_frames_setpoint_scaled(ca, ioa, value, false, qu, cot);
            let event = DetailEvent {
                kind: "setpoint_scaled".to_string(),
                payload: serde_json::json!({ "ioa": ioa, "val": value, "ql": qu, "cot": cot }),
            };
            conn.connection.send_control_with_sbo_event(
                select_frame, execute_frame, ioa,
                &format!("标度化设定值 IOA={} val={} QL={} COT={}", ioa, value, qu, cot),
                FrameLabel::SetpointScaled, ca, Some(event),
            ).await.map_err(|e| format!("{}", e))
        }
        "setpoint_float" => {
            let value = request.value.parse::<f32>().map_err(|e| format!("{}", e))?;
            let select_frame = build_control_frames_setpoint_float(ca, ioa, value, true, qu, cot);
            let execute_frame = build_control_frames_setpoint_float(ca, ioa, value, false, qu, cot);
            let event = DetailEvent {
                kind: "setpoint_float".to_string(),
                payload: serde_json::json!({ "ioa": ioa, "val": value, "ql": qu, "cot": cot }),
            };
            conn.connection.send_control_with_sbo_event(
                select_frame, execute_frame, ioa,
                &format!("浮点设定值 IOA={} val={:.3} QL={} COT={}", ioa, value, qu, cot),
                FrameLabel::SetpointFloat, ca, Some(event),
            ).await.map_err(|e| format!("{}", e))
        }
        "bitstring" => {
            // C_BO_NA_1 has no SbO bit; treat select-mode requests as direct execute with a clear error.
            Err("位串命令 (C_BO_NA_1) 不支持 选择-执行 模式,请关闭 SbO 后再发送".to_string())
        }
        _ => Err(format!("unknown command type: {}", request.command_type)),
    }
}

fn parse_u32_value(s: &str) -> Option<u32> {
    let s = s.trim();
    if let Some(rest) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        u32::from_str_radix(rest, 16).ok()
    } else {
        s.parse::<u32>().ok()
    }
}

#[tauri::command]
pub async fn send_raw_apdu(
    state: State<'_, AppState>,
    request: RawApduRequest,
) -> Result<RawSendResult, String> {
    let connections = state.connections.read().await;
    let conn = connections
        .get(&request.connection_id)
        .ok_or_else(|| format!("connection {} not found", request.connection_id))?;

    let bytes = parse_hex_payload(&request.hex_payload)?;
    if bytes.len() < 6 {
        return Err(format!(
            "APDU 长度过短 ({} 字节),至少需要 6 字节(STARTBYTE+LEN+4 字节控制域)",
            bytes.len()
        ));
    }
    if bytes[0] != 0x68 {
        return Err(format!(
            "APDU 起始字节应为 0x68,实际为 0x{:02X}",
            bytes[0]
        ));
    }
    let declared_len = bytes[1] as usize;
    let expected_total = declared_len + 2;
    if expected_total != bytes.len() {
        return Err(format!(
            "APDU 长度字段不匹配: LEN={} (期望总长 {}),实际总长 {}",
            declared_len, expected_total, bytes.len()
        ));
    }

    conn.connection
        .send_raw_apdu(bytes.clone())
        .await
        .map_err(|e| format!("发送失败: {}", e))?;

    Ok(RawSendResult {
        sent_hex: bytes
            .iter()
            .map(|b| format!("{:02X}", b))
            .collect::<Vec<_>>()
            .join(" "),
        byte_len: bytes.len(),
        timestamp: chrono::Local::now().format("%H:%M:%S%.3f").to_string(),
    })
}

fn parse_hex_payload(s: &str) -> Result<Vec<u8>, String> {
    let mut compact = String::with_capacity(s.len());
    for c in s.chars() {
        if c.is_ascii_hexdigit() {
            compact.push(c);
        } else if c.is_whitespace() || c == ',' || c == '-' || c == ':' {
            continue;
        } else {
            return Err(format!("十六进制串包含非法字符 '{}'", c));
        }
    }
    if compact.is_empty() {
        return Err("十六进制串为空".to_string());
    }
    if compact.len() % 2 != 0 {
        return Err(format!("十六进制位数为奇数 ({} 位),需为偶数", compact.len()));
    }
    let mut out = Vec::with_capacity(compact.len() / 2);
    for i in (0..compact.len()).step_by(2) {
        let byte = u8::from_str_radix(&compact[i..i + 2], 16)
            .map_err(|e| format!("解析字节 '{}' 失败: {}", &compact[i..i + 2], e))?;
        out.push(byte);
    }
    Ok(out)
}

fn parse_bool(s: &str) -> Result<bool, String> {
    match s {
        "1" | "true" | "ON" => Ok(true),
        "0" | "false" | "OFF" => Ok(false),
        _ => s.parse::<bool>().map_err(|_| format!("invalid bool: {}", s)),
    }
}

// Frame builders for SbO (need raw frames before SSN/RSN patching)
fn build_control_frames_single(ca: u16, ioa: u32, value: bool, select: bool, qu: u8, cot: u8) -> Vec<u8> {
    let ca_bytes = ca.to_le_bytes();
    let ioa_bytes = ioa.to_le_bytes();
    let mut sco = (qu & 0x1F) << 2;
    if value { sco |= 0x01; }
    if select { sco |= 0x80; }
    vec![0x68, 0x0E, 0x00, 0x00, 0x00, 0x00, 45, 0x01, cot, 0x00,
         ca_bytes[0], ca_bytes[1], ioa_bytes[0], ioa_bytes[1], ioa_bytes[2], sco]
}

fn build_control_frames_double(ca: u16, ioa: u32, value: u8, select: bool, qu: u8, cot: u8) -> Vec<u8> {
    let ca_bytes = ca.to_le_bytes();
    let ioa_bytes = ioa.to_le_bytes();
    let mut dco = (value & 0x03) | ((qu & 0x1F) << 2);
    if select { dco |= 0x80; }
    vec![0x68, 0x0E, 0x00, 0x00, 0x00, 0x00, 46, 0x01, cot, 0x00,
         ca_bytes[0], ca_bytes[1], ioa_bytes[0], ioa_bytes[1], ioa_bytes[2], dco]
}

fn build_control_frames_step(ca: u16, ioa: u32, value: u8, select: bool, qu: u8, cot: u8) -> Vec<u8> {
    let ca_bytes = ca.to_le_bytes();
    let ioa_bytes = ioa.to_le_bytes();
    let mut rco = (value & 0x03) | ((qu & 0x1F) << 2);
    if select { rco |= 0x80; }
    vec![0x68, 0x0E, 0x00, 0x00, 0x00, 0x00, 47, 0x01, cot, 0x00,
         ca_bytes[0], ca_bytes[1], ioa_bytes[0], ioa_bytes[1], ioa_bytes[2], rco]
}

fn build_control_frames_setpoint_norm(ca: u16, ioa: u32, value: i16, select: bool, ql: u8, cot: u8) -> Vec<u8> {
    let ca_bytes = ca.to_le_bytes();
    let ioa_bytes = ioa.to_le_bytes();
    let nva_bytes = value.to_le_bytes();
    let mut qos = ql & 0x7F;
    if select { qos |= 0x80; }
    vec![0x68, 0x10, 0x00, 0x00, 0x00, 0x00, 48, 0x01, cot, 0x00,
         ca_bytes[0], ca_bytes[1], ioa_bytes[0], ioa_bytes[1], ioa_bytes[2],
         nva_bytes[0], nva_bytes[1], qos]
}

fn build_control_frames_setpoint_scaled(ca: u16, ioa: u32, value: i16, select: bool, ql: u8, cot: u8) -> Vec<u8> {
    let ca_bytes = ca.to_le_bytes();
    let ioa_bytes = ioa.to_le_bytes();
    let sva_bytes = value.to_le_bytes();
    let mut qos = ql & 0x7F;
    if select { qos |= 0x80; }
    vec![0x68, 0x10, 0x00, 0x00, 0x00, 0x00, 49, 0x01, cot, 0x00,
         ca_bytes[0], ca_bytes[1], ioa_bytes[0], ioa_bytes[1], ioa_bytes[2],
         sva_bytes[0], sva_bytes[1], qos]
}

fn build_control_frames_setpoint_float(ca: u16, ioa: u32, value: f32, select: bool, ql: u8, cot: u8) -> Vec<u8> {
    let ca_bytes = ca.to_le_bytes();
    let ioa_bytes = ioa.to_le_bytes();
    let val_bytes = value.to_le_bytes();
    let mut qos = ql & 0x7F;
    if select { qos |= 0x80; }
    vec![0x68, 0x12, 0x00, 0x00, 0x00, 0x00, 50, 0x01, cot, 0x00,
         ca_bytes[0], ca_bytes[1], ioa_bytes[0], ioa_bytes[1], ioa_bytes[2],
         val_bytes[0], val_bytes[1], val_bytes[2], val_bytes[3], qos]
}

// ---------------------------------------------------------------------------
// Data Commands
// ---------------------------------------------------------------------------

fn point_to_info(ca: u16, p: &iec104sim_core::data_point::DataPoint) -> ReceivedDataPointInfo {
    ReceivedDataPointInfo {
        ioa: p.ioa,
        common_address: ca,
        asdu_type: p.asdu_type.name().to_string(),
        asdu_type_id: p.asdu_type as u8,
        category: p.asdu_type.category().name().to_string(),
        value: p.value.display(),
        quality_ov: p.quality.ov,
        quality_bl: p.quality.bl,
        quality_sb: p.quality.sb,
        quality_nt: p.quality.nt,
        quality_iv: p.quality.iv,
        timestamp: p.timestamp.map(|t| t.with_timezone(&chrono::Local).format("%H:%M:%S%.3f").to_string()),
        update_seq: p.update_seq,
    }
}

#[tauri::command]
pub async fn get_received_data(
    state: State<'_, AppState>,
    id: String,
) -> Result<Vec<ReceivedDataPointInfo>, String> {
    let connections = state.connections.read().await;
    let conn = connections
        .get(&id)
        .ok_or_else(|| format!("connection {} not found", id))?;

    let data = conn.connection.received_data.read().await;
    let result: Vec<ReceivedDataPointInfo> = data
        .all_sorted()
        .iter()
        .map(|(ca, p)| point_to_info(*ca, p))
        .collect();

    Ok(result)
}

#[tauri::command]
pub async fn get_received_data_since(
    state: State<'_, AppState>,
    id: String,
    since_seq: u64,
) -> Result<IncrementalDataResponse, String> {
    let connections = state.connections.read().await;
    let conn = connections
        .get(&id)
        .ok_or_else(|| format!("connection {} not found", id))?;

    let data = conn.connection.received_data.read().await;
    let points: Vec<ReceivedDataPointInfo> = data
        .changed_since(since_seq)
        .iter()
        .map(|(ca, p)| point_to_info(*ca, p))
        .collect();

    Ok(IncrementalDataResponse {
        seq: data.current_seq(),
        total_count: data.total_len(),
        points,
    })
}

// ---------------------------------------------------------------------------
// Log Commands
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn get_communication_logs(
    state: State<'_, AppState>,
    connection_id: String,
) -> Result<Vec<LogEntry>, String> {
    let connections = state.connections.read().await;
    let conn = connections
        .get(&connection_id)
        .ok_or_else(|| format!("connection {} not found", connection_id))?;
    Ok(conn.log_collector.get_all().await)
}

#[tauri::command]
pub async fn clear_communication_logs(
    state: State<'_, AppState>,
    connection_id: String,
) -> Result<(), String> {
    let connections = state.connections.read().await;
    let conn = connections
        .get(&connection_id)
        .ok_or_else(|| format!("connection {} not found", connection_id))?;
    conn.log_collector.clear().await;
    Ok(())
}

#[tauri::command]
pub async fn set_logging_enabled(
    state: State<'_, AppState>,
    connection_id: String,
    enabled: bool,
) -> Result<(), String> {
    let connections = state.connections.read().await;
    let conn = connections
        .get(&connection_id)
        .ok_or_else(|| format!("connection {} not found", connection_id))?;
    conn.log_collector.set_enabled(enabled);
    Ok(())
}

#[tauri::command]
pub async fn export_logs_csv(
    state: State<'_, AppState>,
    connection_id: String,
) -> Result<String, String> {
    let connections = state.connections.read().await;
    let conn = connections
        .get(&connection_id)
        .ok_or_else(|| format!("connection {} not found", connection_id))?;
    Ok(conn.log_collector.export_csv().await)
}

// ---------------------------------------------------------------------------
// Tool Commands — frame parsing
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn parse_hex(data: String) -> Result<Vec<u8>, String> {
    iec104sim_core::tools::parse_hex_string(&data)
        .map_err(|e| format!("{}", e))
}

#[tauri::command]
pub fn parse_frame_full(data: String) -> Result<iec104sim_core::decode::ParsedFrame, String> {
    let bytes = iec104sim_core::tools::parse_hex_string(&data)
        .map_err(|e| format!("{}", e))?;
    iec104sim_core::decode::parse_frame_full(&bytes)
}

// ---------------------------------------------------------------------------
// Config file save/open
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn save_config(
    state: State<'_, AppState>,
    path: String,
) -> Result<(), String> {
    use iec104sim_core::config::{MasterConfigFile, MasterConnectionConfig, MasterSnapshotPoint};

    let json = {
        let connections = state.connections.read().await;
        let mut out = Vec::new();
        for (_id, cs) in connections.iter() {
            let cfg = &cs.connection.config;
            let data = cs.connection.received_data.read().await;
            let snapshot: Vec<MasterSnapshotPoint> = data
                .all_sorted()
                .into_iter()
                .map(|(ca, p)| MasterSnapshotPoint { ca, point: p.clone() })
                .collect();
            out.push(MasterConnectionConfig {
                target_address: cfg.target_address.clone(),
                port: cfg.port,
                common_addresses: cs.common_addresses.clone(),
                timeout_ms: cfg.timeout_ms,
                t0: cfg.t0,
                t1: cfg.t1,
                t2: cfg.t2,
                t3: cfg.t3,
                k: cfg.k,
                w: cfg.w,
                default_qoi: cfg.default_qoi,
                default_qcc: cfg.default_qcc,
                interrogate_period_s: cfg.interrogate_period_s,
                counter_interrogate_period_s: cfg.counter_interrogate_period_s,
                use_tls: cfg.tls.enabled,
                ca_file: cfg.tls.ca_file.clone(),
                cert_file: cfg.tls.cert_file.clone(),
                key_file: cfg.tls.key_file.clone(),
                accept_invalid_certs: cfg.tls.accept_invalid_certs,
                tls_version: tls_version_str(cfg.tls.version).to_string(),
                broadcast_address: Some(cfg.broadcast_address),
                snapshot,
            });
        }
        MasterConfigFile::new(out).to_json()?
    };
    std::fs::write(&path, json).map_err(|e| format!("写入文件失败: {e}"))
}

#[tauri::command]
pub async fn load_config(
    state: State<'_, AppState>,
    app_handle: AppHandle,
    path: String,
) -> Result<usize, String> {
    use iec104sim_core::config::MasterConfigFile;

    let content = std::fs::read_to_string(&path)
        .map_err(|e| format!("读取文件失败: {e}"))?;
    let file = MasterConfigFile::from_json(&content)?;

    let mut imported = 0usize;
    let mut corrected_events: Vec<TimingCorrectedEvent> = Vec::new();
    for conn in file.connections {
        let request = CreateConnectionRequest {
            target_address: conn.target_address,
            port: conn.port,
            common_addresses: Some(conn.common_addresses),
            common_address: None,
            timeout_ms: Some(conn.timeout_ms),
            use_tls: Some(conn.use_tls),
            ca_file: Some(conn.ca_file),
            cert_file: Some(conn.cert_file),
            key_file: Some(conn.key_file),
            accept_invalid_certs: Some(conn.accept_invalid_certs),
            tls_version: Some(conn.tls_version),
            t0: Some(conn.t0),
            t1: Some(conn.t1),
            t2: Some(conn.t2),
            t3: Some(conn.t3),
            k: Some(conn.k),
            w: Some(conn.w),
            default_qoi: Some(conn.default_qoi),
            default_qcc: Some(conn.default_qcc),
            interrogate_period_s: Some(conn.interrogate_period_s),
            counter_interrogate_period_s: Some(conn.counter_interrogate_period_s),
            broadcast_address: conn.broadcast_address,
        };
        let info = create_connection(state.clone(), app_handle.clone(), request).await?;

        if !info.timing_corrections.is_empty() {
            corrected_events.push(TimingCorrectedEvent {
                target_address: info.target_address.clone(),
                corrections: info.timing_corrections.clone(),
            });
        }

        if !conn.snapshot.is_empty() {
            let connections = state.connections.read().await;
            let cs = connections
                .get(&info.id)
                .ok_or_else(|| format!("新建连接 {} 已不存在,无法注入快照", info.id))?;
            let mut data = cs.connection.received_data.write().await;
            for sp in conn.snapshot {
                data.insert(sp.ca, sp.point);
            }
        }
        imported += 1;
    }
    // Surface any import-time timing corrections so the user knows the loaded
    // config was adjusted to satisfy the IEC 104 invariants.
    if !corrected_events.is_empty() {
        let _ = app_handle.emit("config-timing-corrected", &corrected_events);
    }
    Ok(imported)
}

/// Payload for the `config-timing-corrected` event emitted by `load_config`.
#[derive(Clone, serde::Serialize)]
struct TimingCorrectedEvent {
    target_address: String,
    corrections: Vec<iec104sim_core::timing::TimingCorrection>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_request_deserializes_broadcast_address() {
        let json = r#"{
            "target_address": "127.0.0.1",
            "port": 2404,
            "common_addresses": [1],
            "broadcast_address": 65280
        }"#;
        let req: CreateConnectionRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.broadcast_address, Some(0xFF00));
    }

    #[test]
    fn create_request_missing_broadcast_address_is_none() {
        let json = r#"{"target_address":"127.0.0.1","port":2404,"common_addresses":[1]}"#;
        let req: CreateConnectionRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.broadcast_address, None);
    }

    #[test]
    fn tls_version_str_maps_all_variants() {
        assert_eq!(tls_version_str(TlsVersionPolicy::Auto), "auto");
        assert_eq!(tls_version_str(TlsVersionPolicy::Tls12Only), "tls12_only");
        assert_eq!(tls_version_str(TlsVersionPolicy::Tls13Only), "tls13_only");
    }

    // Regression: ConnectionInfo must echo the TLS file paths/policy so the
    // edit dialog reads them from the connection itself. Before this, the DTO
    // dropped them and cert-path edits silently reverted on reopen.
    #[test]
    fn connection_info_echoes_tls_paths() {
        let cfg = MasterConfig {
            target_address: "10.0.0.1".into(),
            port: 2404,
            tls: TlsConfig {
                enabled: true,
                ca_file: "/etc/ca.pem".into(),
                cert_file: "/etc/client.pem".into(),
                key_file: "/etc/client-key.pem".into(),
                accept_invalid_certs: true,
                version: TlsVersionPolicy::Tls13Only,
                ..Default::default()
            },
            ..MasterConfig::default()
        };
        let info = ConnectionInfo::from_config(
            "conn_1".into(),
            "Disconnected".into(),
            vec![1, 2],
            &cfg,
            Vec::new(),
        );
        assert!(info.use_tls);
        assert_eq!(info.ca_file, "/etc/ca.pem");
        assert_eq!(info.cert_file, "/etc/client.pem");
        assert_eq!(info.key_file, "/etc/client-key.pem");
        assert!(info.accept_invalid_certs);
        assert_eq!(info.tls_version, "tls13_only");
    }
}
