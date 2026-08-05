use crate::state::{AppState, DataPointInfo, DataPointValueSnapshot, IncrementalDataResponse, ServerInfo, SlaveServerState, StationInfo};
use iec104sim_core::data_point::{ControlTarget, DataPoint, DataPointValue, InformationObjectDef};
use iec104sim_core::log_collector::LogCollector;
use iec104sim_core::log_entry::LogEntry;
use iec104sim_core::slave::{
    MutationMode, MutationParams, ProtocolTimingConfig, RemoteOperationConfig, ServerState,
    SlaveError, SlaveServer, SlaveTransportConfig, Station,
};
use iec104sim_core::types::{AsduTypeId, QualityFlags};
use rand::Rng;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tauri::{AppHandle, Emitter, State};

// ---------------------------------------------------------------------------
// Event Payloads
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct ServerStateEvent {
    pub id: String,
    pub state: String,
}

// ---------------------------------------------------------------------------
// Server Commands
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct CreateServerRequest {
    pub bind_address: Option<String>,
    pub port: u16,
    /// Initial logical station created with the listener. Defaults to CA=1
    /// with an empty user-defined name for backward compatibility.
    #[serde(default)]
    pub common_address: Option<u16>,
    #[serde(default)]
    pub station_name: Option<String>,
    pub init_mode: Option<String>,
    /// 默认 station 每个 ASDU 类型分类下的点数（缺省 10）。0 = 空站。
    pub count_per_category: Option<u32>,
    pub use_tls: Option<bool>,
    pub cert_file: Option<String>,
    pub key_file: Option<String>,
    pub ca_file: Option<String>,
    pub require_client_cert: Option<bool>,
    /// 可选:创建时直接附带协议时序参数。缺省时使用 SlaveServer 默认值。
    #[serde(default)]
    pub protocol_timing: Option<ProtocolTimingConfig>,
    /// 可选:创建时直接附带远动运行参数。缺省时使用默认值。
    #[serde(default)]
    pub remote_ops: Option<RemoteOperationConfig>,
}

#[tauri::command]
pub async fn create_server(
    state: State<'_, AppState>,
    request: CreateServerRequest,
) -> Result<ServerInfo, String> {
    let id = {
        let mut counter = state.next_server_id.write().await;
        let id = format!("server_{}", *counter);
        *counter += 1;
        id
    };

    let transport = SlaveTransportConfig {
        bind_address: request.bind_address.unwrap_or_else(|| "0.0.0.0".to_string()),
        port: request.port,
        tls: iec104sim_core::slave::SlaveTlsConfig {
            enabled: request.use_tls.unwrap_or(false),
            cert_file: request.cert_file.unwrap_or_default(),
            key_file: request.key_file.unwrap_or_default(),
            ca_file: request.ca_file.unwrap_or_default(),
            require_client_cert: request.require_client_cert.unwrap_or(false),
            pkcs12_file: String::new(),
            pkcs12_password: String::new(),
        },
    };

    let log_collector = Arc::new(LogCollector::new());
    let server = SlaveServer::new(transport).with_log_collector(log_collector.clone());

    // 在加站点之前应用服务器级配置,以便首次 set 后的任何上送都按目标参数发送。
    if let Some(mut t) = request.protocol_timing {
        // 后端权威:落地前规范化,确保 t2<t1<t3、w≤⌊2k/3⌋。
        let _ = t.normalize();
        server.set_protocol_timing(t).await;
    }
    server
        .set_remote_ops(
            request
                .remote_ops
                .unwrap_or_else(RemoteOperationConfig::for_new_server),
        )
        .await;

    // Auto-create the requested initial logical station with pre-filled data points.
    let n = request.count_per_category.unwrap_or(10);
    let common_address = request.common_address.unwrap_or(1);
    let station_name = request.station_name.unwrap_or_default();
    let default_station = match request.init_mode.as_deref() {
        Some("random") => Station::with_random_points(common_address, station_name, n),
        _ => Station::with_default_points(common_address, station_name, n),
    };
    server
        .add_station(default_station)
        .await
        .map_err(|e| format!("failed to add default station: {}", e))?;

    let info = ServerInfo {
        id: id.clone(),
        bind_address: server.transport.bind_address.clone(),
        port: server.transport.port,
        state: format!("{:?}", server.state()),
        station_count: 1,
        client_count: 0,
        use_tls: server.transport.tls.enabled,
    };

    state.servers.write().await.insert(
        id,
        SlaveServerState {
            server,
            log_collector,
        },
    );

    Ok(info)
}

#[tauri::command]
pub async fn start_server(
    state: State<'_, AppState>,
    app_handle: AppHandle,
    id: String,
) -> Result<(), StartServerError> {
    let state_str: String;
    {
        let mut servers = state.servers.write().await;
        let srv = servers
            .get_mut(&id)
            .ok_or_else(|| StartServerError::new("server_not_found", format!("server {} not found", id)))?;

        if let Err(error) = srv.server.start().await {
            return Err(StartServerError::from_slave(error));
        }
        state_str = format!("{:?}", srv.server.state());
    }

    app_handle.emit("server-state-changed", ServerStateEvent {
        id, state: state_str,
    }).map_err(|e| StartServerError::new("event_failed", e.to_string()))?;

    Ok(())
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct StartServerError {
    pub code: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub addr: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub os_error: Option<i32>,
}

impl StartServerError {
    fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self { code: code.into(), message: message.into(), addr: None, os_error: None }
    }

    fn from_slave(error: SlaveError) -> Self {
        match error {
            SlaveError::BindFailed { code, addr, os_error, message } => Self {
                code: code.to_string(),
                message,
                addr: Some(addr),
                os_error: Some(os_error),
            },
            SlaveError::AlreadyRunning => Self::new("already_running", "server is already running"),
            other => Self::new("start_failed", other.to_string()),
        }
    }
}

#[tauri::command]
pub async fn stop_server(
    state: State<'_, AppState>,
    app_handle: AppHandle,
    id: String,
) -> Result<(), String> {
    let state_str: String;
    {
        let mut servers = state.servers.write().await;
        let srv = servers
            .get_mut(&id)
            .ok_or_else(|| format!("server {} not found", id))?;

        srv.server
            .stop()
            .await
            .map_err(|e| format!("failed to stop: {}", e))?;
        state_str = format!("{:?}", srv.server.state());
    }

    app_handle.emit("server-state-changed", ServerStateEvent {
        id, state: state_str,
    }).map_err(|e| e.to_string())?;

    Ok(())
}

/// 删除服务器。运行中的服务器先 stop() 再移除:直接 remove 会泄漏监听 socket
/// 与 accept/cyclic 任务,导致原端口无法立即重建(issue #28)。
/// 拆出可测主体,命令包装保持薄。
pub(crate) async fn delete_server_impl(state: &AppState, id: &str) -> Result<(), String> {
    let mut servers = state.servers.write().await;
    let srv = servers
        .get_mut(id)
        .ok_or_else(|| format!("server {} not found", id))?;
    if srv.server.state() == ServerState::Running {
        srv.server
            .stop()
            .await
            .map_err(|e| format!("failed to stop before delete: {}", e))?;
    }
    servers.remove(id);
    Ok(())
}

#[tauri::command]
pub async fn delete_server(
    state: State<'_, AppState>,
    id: String,
) -> Result<(), String> {
    delete_server_impl(state.inner(), &id).await
}

/// 本机可用的监听地址建议:0.0.0.0(全部网卡)在前,然后回环与各网卡 IPv4。
/// 供 New Server 对话框的绑定地址下拉使用;枚举失败时退化为通配 + 回环。
/// IPv6 暂不提供(bind 使用 "addr:port" 拼接,需另行处理方括号)。
#[tauri::command]
pub fn list_bind_address_suggestions() -> Vec<String> {
    let mut out = vec!["0.0.0.0".to_string(), "127.0.0.1".to_string()];
    if let Ok(ifaces) = if_addrs::get_if_addrs() {
        for iface in ifaces {
            if let std::net::IpAddr::V4(v4) = iface.ip() {
                let s = v4.to_string();
                if !out.contains(&s) {
                    out.push(s);
                }
            }
        }
    }
    out
}

#[tauri::command]
pub async fn list_servers(
    state: State<'_, AppState>,
) -> Result<Vec<ServerInfo>, String> {
    let servers = state.servers.read().await;
    let mut result = Vec::new();

    for (id, srv_state) in servers.iter() {
        let station_count = srv_state.server.stations.read().await.len();
        let client_count = srv_state.server.client_connection_count().await;
        result.push(ServerInfo {
            id: id.clone(),
            bind_address: srv_state.server.transport.bind_address.clone(),
            port: srv_state.server.transport.port,
            state: format!("{:?}", srv_state.server.state()),
            station_count,
            client_count,
            use_tls: srv_state.server.transport.tls.enabled,
        });
    }

    Ok(result)
}

/// Return the live Master-side sessions accepted by one Slave listener.
/// This is intentionally read-only: disconnecting a Master remains the
/// remote peer's responsibility (or follows from stopping the server).
#[tauri::command]
pub async fn list_client_connections(
    state: State<'_, AppState>,
    server_id: String,
) -> Result<Vec<crate::state::ClientConnectionInfo>, String> {
    let servers = state.servers.read().await;
    let srv = servers
        .get(&server_id)
        .ok_or_else(|| format!("server {} not found", server_id))?;
    Ok(srv
        .server
        .client_connections()
        .await
        .into_iter()
        .map(|snapshot| crate::state::ClientConnectionInfo {
            peer_address: snapshot.peer_addr.to_string(),
            data_transfer_active: snapshot.data_transfer_active,
        })
        .collect())
}

/// 校验传输配置(监听地址 / 端口)改动是否被允许。纯函数,便于单测:
/// 端口 0 非法;运行中的服务器端口被监听 socket 占用,必须先停止再改。
fn validate_transport_change(state: ServerState, port: u16) -> Result<(), String> {
    if port == 0 {
        return Err("端口必须在 1–65535 之间".to_string());
    }
    if state == ServerState::Running {
        return Err("请先停止服务器再修改监听地址 / 端口".to_string());
    }
    Ok(())
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct UpdateServerTransportRequest {
    pub server_id: String,
    pub bind_address: String,
    pub port: u16,
}

/// 修改已存在服务器的监听地址 / 端口。传输配置原本只在 `create_server` 时设定,
/// 本命令让用户无需删除重建即可改端口。运行中拒绝(端口被监听占用,见
/// `validate_transport_change`),需先 `stop_server`。
#[tauri::command]
pub async fn update_server_transport(
    state: State<'_, AppState>,
    request: UpdateServerTransportRequest,
) -> Result<ServerInfo, String> {
    let mut servers = state.servers.write().await;
    let srv = servers
        .get_mut(&request.server_id)
        .ok_or_else(|| format!("server {} not found", request.server_id))?;

    validate_transport_change(srv.server.state(), request.port)?;

    // 空地址回落到 0.0.0.0(与 create_server 默认一致),避免 bind 到空串失败。
    let bind = {
        let b = request.bind_address.trim();
        if b.is_empty() { "0.0.0.0".to_string() } else { b.to_string() }
    };
    srv.server.transport.bind_address = bind;
    srv.server.transport.port = request.port;

    let station_count = srv.server.stations.read().await.len();
    Ok(ServerInfo {
        id: request.server_id.clone(),
        bind_address: srv.server.transport.bind_address.clone(),
        port: srv.server.transport.port,
        state: format!("{:?}", srv.server.state()),
        station_count,
        client_count: srv.server.client_connection_count().await,
        use_tls: srv.server.transport.tls.enabled,
    })
}

// ---------------------------------------------------------------------------
// Station Commands
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct AddStationRequest {
    pub server_id: String,
    pub common_address: u16,
    pub name: String,
    pub init_mode: Option<String>,
}

#[tauri::command]
pub async fn add_station(
    state: State<'_, AppState>,
    request: AddStationRequest,
) -> Result<StationInfo, String> {
    let servers = state.servers.read().await;
    let srv = servers
        .get(&request.server_id)
        .ok_or_else(|| format!("server {} not found", request.server_id))?;

    let station = match request.init_mode.as_deref() {
        Some("random") => Station::with_random_points(request.common_address, request.name.clone(), 10),
        Some("zero") => Station::with_default_points(request.common_address, request.name.clone(), 10),
        _ => Station::new(request.common_address, request.name.clone()),
    };
    let point_count = station.data_points.len();

    srv.server
        .add_station(station)
        .await
        .map_err(|e| format!("failed to add station: {}", e))?;

    Ok(StationInfo {
        common_address: request.common_address,
        name: request.name,
        point_count,
    })
}

#[tauri::command]
pub async fn remove_station(
    state: State<'_, AppState>,
    server_id: String,
    common_address: u16,
) -> Result<(), String> {
    let servers = state.servers.read().await;
    let srv = servers
        .get(&server_id)
        .ok_or_else(|| format!("server {} not found", server_id))?;

    srv.server
        .remove_station(common_address)
        .await
        .map_err(|e| format!("failed to remove station: {}", e))?;

    Ok(())
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct UpdateStationRequest {
    pub server_id: String,
    pub current_common_address: u16,
    pub common_address: u16,
    pub name: String,
}

#[tauri::command]
pub async fn update_station(
    state: State<'_, AppState>,
    request: UpdateStationRequest,
) -> Result<StationInfo, String> {
    let servers = state.servers.read().await;
    let srv = servers
        .get(&request.server_id)
        .ok_or_else(|| format!("server {} not found", request.server_id))?;

    srv.server
        .update_station(
            request.current_common_address,
            request.common_address,
            request.name.clone(),
        )
        .await
        .map_err(|e| format!("failed to update station: {}", e))?;

    let stations = srv.server.stations.read().await;
    let station = stations
        .get(&request.common_address)
        .ok_or_else(|| format!("station CA={} not found after update", request.common_address))?;
    Ok(StationInfo {
        common_address: station.common_address,
        name: station.name.clone(),
        point_count: station.data_points.len(),
    })
}

#[tauri::command]
pub async fn list_stations(
    state: State<'_, AppState>,
    server_id: String,
) -> Result<Vec<StationInfo>, String> {
    let servers = state.servers.read().await;
    let srv = servers
        .get(&server_id)
        .ok_or_else(|| format!("server {} not found", server_id))?;

    let stations = srv.server.stations.read().await;
    let result: Vec<StationInfo> = stations
        .values()
        .map(|s| StationInfo {
            common_address: s.common_address,
            name: s.name.clone(),
            point_count: s.data_points.len(),
        })
        .collect();

    Ok(result)
}

// ---------------------------------------------------------------------------
// Data Point Commands
// ---------------------------------------------------------------------------

pub(crate) fn parse_asdu_type(s: &str) -> Result<AsduTypeId, String> {
    // 归一化: 小写 + 仅保留字母数字。涵盖三种来源:
    // PascalCase 枚举名 ("MSpNa1") / 小写下划线 ("m_sp_na_1") /
    // 前端从 list_data_points 拿到的显示名 ("M_SP_NA_1").
    let key: String = s.chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(|c| c.to_lowercase())
        .collect();
    match key.as_str() {
        "mspna1" => Ok(AsduTypeId::MSpNa1),
        "mspta1" => Ok(AsduTypeId::MSpTa1),
        "msptb1" => Ok(AsduTypeId::MSpTb1),
        "mdpna1" => Ok(AsduTypeId::MDpNa1),
        "mdpta1" => Ok(AsduTypeId::MDpTa1),
        "mdptb1" => Ok(AsduTypeId::MDpTb1),
        "mstna1" => Ok(AsduTypeId::MStNa1),
        "mstta1" => Ok(AsduTypeId::MStTa1),
        "msttb1" => Ok(AsduTypeId::MStTb1),
        "mbona1" => Ok(AsduTypeId::MBoNa1),
        "mbotb1" => Ok(AsduTypeId::MBoTb1),
        "mmena1" => Ok(AsduTypeId::MMeNa1),
        "mmend1" => Ok(AsduTypeId::MMeNd1),
        "mmenb1" => Ok(AsduTypeId::MMeNb1),
        "mmenc1" => Ok(AsduTypeId::MMeNc1),
        "mmeta1" => Ok(AsduTypeId::MMeTa1),
        "mmetb1" => Ok(AsduTypeId::MMeTb1),
        "mmetc1" => Ok(AsduTypeId::MMeTc1),
        "mmetd1" => Ok(AsduTypeId::MMeTd1),
        "mmete1" => Ok(AsduTypeId::MMeTe1),
        "mmetf1" => Ok(AsduTypeId::MMeTf1),
        "mitna1" => Ok(AsduTypeId::MItNa1),
        "mittb1" => Ok(AsduTypeId::MItTb1),
        "cscna1" => Ok(AsduTypeId::CScNa1),
        "cdcna1" => Ok(AsduTypeId::CDcNa1),
        "crcna1" => Ok(AsduTypeId::CRcNa1),
        "csena1" => Ok(AsduTypeId::CSeNa1),
        "csenb1" => Ok(AsduTypeId::CSeNb1),
        "csenc1" => Ok(AsduTypeId::CSeNc1),
        "cbona1" => Ok(AsduTypeId::CBoNa1),
        "cscta1" => Ok(AsduTypeId::CScTa1),
        "cdcta1" => Ok(AsduTypeId::CDcTa1),
        "crcta1" => Ok(AsduTypeId::CRcTa1),
        "cseta1" => Ok(AsduTypeId::CSeTa1),
        "csetb1" => Ok(AsduTypeId::CSeTb1),
        "csetc1" => Ok(AsduTypeId::CSeTc1),
        "cbota1" => Ok(AsduTypeId::CBoTa1),
        _ => Err(format!("unknown ASDU type: {}", s)),
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ControlTargetRequest {
    pub common_address: u16,
    pub ioa: u32,
    pub asdu_type: String,
}

fn resolve_control_target(
    stations: &std::collections::HashMap<u16, Station>,
    source_type: AsduTypeId,
    request: Option<&ControlTargetRequest>,
) -> Result<Option<ControlTarget>, String> {
    let Some(request) = request else { return Ok(None) };
    if !source_type.is_control() {
        return Err("only control-direction points can define a mapping".to_string());
    }
    let target_type = parse_asdu_type(&request.asdu_type)?;
    if !source_type.allowed_target_categories().contains(&target_type.category()) {
        return Err(format!(
            "{} cannot map to {}",
            source_type.name(),
            target_type.name(),
        ));
    }
    let target_exists = stations
        .get(&request.common_address)
        .map(|station| station.data_points.contains(request.ioa, target_type))
        .unwrap_or(false);
    if !target_exists {
        return Err(format!(
            "mapping target not found: CA={} IOA={} {}",
            request.common_address,
            request.ioa,
            target_type.name(),
        ));
    }
    Ok(Some(ControlTarget {
        common_address: request.common_address,
        ioa: request.ioa,
        asdu_type: target_type,
    }))
}

pub(crate) fn validate_control_point_options(
    asdu_type: AsduTypeId,
    qualifier: Option<u8>,
    select_before_operate: Option<bool>,
) -> Result<(), String> {
    if !asdu_type.is_control() {
        if qualifier.is_some() || select_before_operate.is_some() {
            return Err("QU/QL and S/E can only be configured on control points".to_string());
        }
        return Ok(());
    }
    match asdu_type.untimestamped_variant() {
        AsduTypeId::CScNa1 | AsduTypeId::CDcNa1 | AsduTypeId::CRcNa1 => {
            if qualifier.is_some_and(|q| q > 31) {
                return Err("command qualifier QU must be in 0..31".to_string());
            }
        }
        AsduTypeId::CSeNa1 | AsduTypeId::CSeNb1 | AsduTypeId::CSeNc1 => {
            if qualifier.is_some_and(|q| q > 127) {
                return Err("set-point qualifier QL must be in 0..127".to_string());
            }
        }
        AsduTypeId::CBoNa1 => {
            if qualifier.is_some() || select_before_operate.is_some() {
                return Err("bitstring commands do not carry QU/QL or S/E".to_string());
            }
        }
        _ => {}
    }
    Ok(())
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct AddDataPointRequest {
    pub server_id: String,
    pub common_address: u16,
    pub ioa: u32,
    pub asdu_type: String,
    pub name: Option<String>,
    pub comment: Option<String>,
    pub mapping: Option<ControlTargetRequest>,
    pub command_qualifier: Option<u8>,
    pub select_before_operate: Option<bool>,
}

#[tauri::command]
pub async fn add_data_point(
    state: State<'_, AppState>,
    request: AddDataPointRequest,
) -> Result<(), String> {
    let servers = state.servers.read().await;
    let srv = servers
        .get(&request.server_id)
        .ok_or_else(|| format!("server {} not found", request.server_id))?;

    let asdu_type = parse_asdu_type(&request.asdu_type)?;
    validate_control_point_options(
        asdu_type,
        request.command_qualifier,
        request.select_before_operate,
    )?;
    let mut stations = srv.server.stations.write().await;
    let mapping = resolve_control_target(&stations, asdu_type, request.mapping.as_ref())?;
    let def = InformationObjectDef {
        ioa: request.ioa,
        asdu_type,
        category: asdu_type.category(),
        name: request.name.unwrap_or_default(),
        comment: request.comment.unwrap_or_default(),
        mapping,
        command_qualifier: request.command_qualifier,
        select_before_operate: request.select_before_operate,
    };

    let station = stations
        .get_mut(&request.common_address)
        .ok_or_else(|| format!("station CA={} not found", request.common_address))?;

    // 用户手动添加走严格语义:同 (IOA, 类型) 已存在时拒绝,而不是静默覆盖掉
    // 已有点的名称/备注/QU-QL/S-E/控制映射(issue #28)。
    station.add_point_strict(def)
        .map_err(|e| match e {
            SlaveError::DuplicateIoa(ioa) => format!(
                "IOA {} of type {} already exists in station CA={} — edit that point or pick another IOA",
                ioa, request.asdu_type, request.common_address
            ),
            other => format!("failed to add point: {}", other),
        })
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct UpdateDataPointDefinitionRequest {
    pub server_id: String,
    pub common_address: u16,
    pub ioa: u32,
    pub asdu_type: String,
    pub name: Option<String>,
    pub comment: Option<String>,
    pub mapping: Option<ControlTargetRequest>,
    pub command_qualifier: Option<u8>,
    pub select_before_operate: Option<bool>,
    /// 目标 IOA。与 `ioa` 不同时执行改址:冲突校验、运行时点位迁移、
    /// 引用该点的控制映射(含跨 CA)同步更新。
    #[serde(default)]
    pub new_ioa: Option<u32>,
    /// Optional target ASDU type. The original `asdu_type` remains the source
    /// key so changing IOA and type can be one atomic operation.
    #[serde(default)]
    pub new_asdu_type: Option<String>,
}

/// Update point metadata and its optional explicit control mapping without
/// changing the runtime value. Mapping validation is all-or-nothing.
/// `new_ioa` moves the point to a new address, keeping value/quality intact.
pub(crate) async fn update_data_point_definition_impl(
    state: &AppState,
    request: UpdateDataPointDefinitionRequest,
) -> Result<(), String> {
    let servers = state.servers.read().await;
    let srv = servers
        .get(&request.server_id)
        .ok_or_else(|| format!("server {} not found", request.server_id))?;
    let asdu_type = parse_asdu_type(&request.asdu_type)?;
    let target_asdu_type = match request.new_asdu_type.as_deref() {
        Some(value) => parse_asdu_type(value)?,
        None => asdu_type,
    };
    validate_control_point_options(
        target_asdu_type,
        request.command_qualifier,
        request.select_before_operate,
    )?;
    let active_mutation = srv
        .server
        .list_point_mutations_with_params()
        .await
        .into_iter()
        .find(|(ca, ioa, t, _, _)| {
            *ca == request.common_address && *ioa == request.ioa && *t == asdu_type
        })
        .map(|(_, _, _, params, period_ms)| (params, period_ms));
    let mut stations = srv.server.stations.write().await;
    let mapping = resolve_control_target(&stations, target_asdu_type, request.mapping.as_ref())?;
    let target_ioa = request.new_ioa.unwrap_or(request.ioa);
    {
        let station = stations
            .get_mut(&request.common_address)
            .ok_or_else(|| format!("station CA={} not found", request.common_address))?;
        station
            .migrate_point(request.ioa, asdu_type, target_ioa, target_asdu_type)
            .map_err(|e| format!("failed to migrate point: {}", e))?;
        let def = station
            .object_defs
            .iter_mut()
            .find(|d| d.ioa == target_ioa && d.asdu_type == target_asdu_type)
            .ok_or_else(|| format!("point IOA={} {} not found", request.ioa, asdu_type.name()))?;
        def.name = request.name.unwrap_or_default();
        def.comment = request.comment.unwrap_or_default();
        def.mapping = mapping;
        def.command_qualifier = request.command_qualifier;
        def.select_before_operate = request.select_before_operate;
        station
            .data_points
            .mark_changed(target_ioa, target_asdu_type);
    }
    let key_changed = target_ioa != request.ioa || target_asdu_type != asdu_type;
    if key_changed {
        // Other control points can reference this point from another CA.
        // Follow both the address and type to avoid a dangling mapping.
        for st in stations.values_mut() {
            for d in st.object_defs.iter_mut() {
                if let Some(t) = d.mapping.as_mut() {
                    if t.common_address == request.common_address
                        && t.ioa == request.ioa
                        && t.asdu_type == asdu_type
                    {
                        t.ioa = target_ioa;
                        t.asdu_type = target_asdu_type;
                    }
                }
            }
        }
    }
    drop(stations);
    if key_changed {
        // Mutation tasks capture their point key. Move an active task to the
        // new key while retaining its effective period and parameters.
        srv.server
            .stop_point_mutation(request.common_address, request.ioa, asdu_type)
            .await;
        if let Some((params, period_ms)) = active_mutation {
            srv.server
                .start_point_mutation(
                    request.common_address,
                    target_ioa,
                    target_asdu_type,
                    period_ms,
                    params,
                )
                .await;
        }
    }
    Ok(())
}

#[tauri::command]
pub async fn update_data_point_definition(
    state: State<'_, AppState>,
    request: UpdateDataPointDefinitionRequest,
) -> Result<(), String> {
    update_data_point_definition_impl(state.inner(), request).await
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct ControlMappingTargetInfo {
    pub common_address: u16,
    pub ioa: u32,
    pub asdu_type: String,
    pub name: String,
}

/// Return only monitor points compatible with the selected control type.
#[tauri::command]
pub async fn list_control_mapping_targets(
    state: State<'_, AppState>,
    server_id: String,
    source_asdu_type: String,
) -> Result<Vec<ControlMappingTargetInfo>, String> {
    let source_type = parse_asdu_type(&source_asdu_type)?;
    if !source_type.is_control() {
        return Ok(Vec::new());
    }
    let stations = {
        let servers = state.servers.read().await;
        let srv = servers
            .get(&server_id)
            .ok_or_else(|| format!("server {} not found", server_id))?;
        srv.server.stations.clone()
    };
    let guard = stations.read().await;
    let allowed = source_type.allowed_target_categories();
    let mut result = Vec::new();
    for (&ca, station) in guard.iter() {
        let names: std::collections::HashMap<(u32, AsduTypeId), &str> = station
            .object_defs
            .iter()
            .map(|d| ((d.ioa, d.asdu_type), d.name.as_str()))
            .collect();
        for point in station.data_points.all_sorted() {
            if allowed.contains(&point.asdu_type.category()) {
                result.push(ControlMappingTargetInfo {
                    common_address: ca,
                    ioa: point.ioa,
                    asdu_type: point.asdu_type.name().to_string(),
                    name: names
                        .get(&(point.ioa, point.asdu_type))
                        .copied()
                        .unwrap_or_default()
                        .to_string(),
                });
            }
        }
    }
    result.sort_unstable_by(|a, b| {
        (a.common_address, a.ioa, a.asdu_type.as_str())
            .cmp(&(b.common_address, b.ioa, b.asdu_type.as_str()))
    });
    Ok(result)
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct BatchAddDataPointsRequest {
    pub server_id: String,
    pub common_address: u16,
    #[serde(default)]
    pub start_ioa: u32,
    #[serde(default)]
    pub count: u32,
    pub asdu_type: String,
    pub name_prefix: Option<String>,
    /// 显式 IOA 列表(支持 "6001-6050" / "6001,6003" 前端解析结果)。
    /// 提供时忽略 start_ioa/count。
    #[serde(default)]
    pub ioas: Option<Vec<u32>>,
    /// 名称模板带 Type ID:`{prefix}_{typeid}_{ioa}`。
    #[serde(default)]
    pub name_with_type_id: bool,
    /// 控制点批量创建时统一配置的 QU/QL 限定词。
    #[serde(default)]
    pub command_qualifier: Option<u8>,
    /// 控制点批量创建时统一配置的 S/E 执行模式。
    #[serde(default)]
    pub select_before_operate: Option<bool>,
}

const BATCH_ADD_MAX: usize = 100_000;

#[tauri::command]
pub async fn batch_add_data_points(
    state: State<'_, AppState>,
    request: BatchAddDataPointsRequest,
) -> Result<u32, String> {
    let servers = state.servers.read().await;
    let srv = servers
        .get(&request.server_id)
        .ok_or_else(|| format!("server {} not found", request.server_id))?;

    let asdu_type = parse_asdu_type(&request.asdu_type)?;
    validate_control_point_options(
        asdu_type,
        request.command_qualifier,
        request.select_before_operate,
    )?;

    let ioas: Vec<u32> = match &request.ioas {
        Some(list) => list.clone(),
        None => (0..request.count).map(|i| request.start_ioa + i).collect(),
    };
    if ioas.is_empty() {
        return Err("no IOA to add".to_string());
    }
    if ioas.len() > BATCH_ADD_MAX {
        return Err(format!("batch too large: {} > {}", ioas.len(), BATCH_ADD_MAX));
    }

    let mut stations = srv.server.stations.write().await;
    let station = stations
        .get_mut(&request.common_address)
        .ok_or_else(|| format!("station CA={} not found", request.common_address))?;

    station
        .batch_add_points_list(
            &ioas,
            asdu_type,
            request.name_prefix.as_deref().unwrap_or(""),
            request.name_with_type_id,
            request.command_qualifier,
            request.select_before_operate,
        )
        .map_err(|e| format!("failed to batch add points: {}", e))
}

#[tauri::command]
pub async fn remove_data_point(
    state: State<'_, AppState>,
    server_id: String,
    common_address: u16,
    ioa: u32,
    asdu_type: String,
) -> Result<(), String> {
    let servers = state.servers.read().await;
    let srv = servers
        .get(&server_id)
        .ok_or_else(|| format!("server {} not found", server_id))?;

    let asdu = parse_asdu_type(&asdu_type)?;

    let mut stations = srv.server.stations.write().await;
    let station = stations
        .get_mut(&common_address)
        .ok_or_else(|| format!("station CA={} not found", common_address))?;

    station.remove_point(ioa, asdu)
        .map_err(|e| format!("failed to remove point: {}", e))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct RemovePointTarget {
    pub ioa: u32,
    pub asdu_type: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct BatchMigrateDataPointTypesRequest {
    pub server_id: String,
    pub common_address: u16,
    pub points: Vec<RemovePointTarget>,
    pub target_asdu_type: String,
}

/// Atomically migrate selected monitor points to a compatible ASDU type.
/// A staged station clone ensures that a missing source, incompatible type,
/// or target collision leaves every point and mapping untouched.
pub(crate) async fn batch_migrate_data_point_types_impl(
    state: &AppState,
    request: BatchMigrateDataPointTypesRequest,
) -> Result<usize, String> {
    let target_type = parse_asdu_type(&request.target_asdu_type)?;
    if target_type.is_control()
        || target_type.category() == iec104sim_core::types::DataCategory::System
    {
        return Err("batch ASDU type migration supports monitor points only".to_string());
    }

    let mut sources = Vec::with_capacity(request.points.len());
    let mut seen = std::collections::HashSet::new();
    for point in &request.points {
        let source_type = parse_asdu_type(&point.asdu_type)?;
        if source_type.is_control()
            || source_type.category() == iec104sim_core::types::DataCategory::System
        {
            return Err(format!(
                "batch ASDU type migration does not support {}",
                source_type.name()
            ));
        }
        if source_type.category() != target_type.category() {
            return Err(format!(
                "incompatible ASDU type migration: {} to {}",
                source_type.name(),
                target_type.name()
            ));
        }
        if seen.insert((point.ioa, source_type)) && source_type != target_type {
            sources.push((point.ioa, source_type));
        }
    }
    if sources.is_empty() {
        return Ok(0);
    }

    let servers = state.servers.read().await;
    let srv = servers
        .get(&request.server_id)
        .ok_or_else(|| format!("server {} not found", request.server_id))?;
    let active_tasks = srv.server.list_point_mutations_with_params().await;
    let migrations: Vec<(u32, AsduTypeId, u32, AsduTypeId)> = sources
        .iter()
        .map(|&(ioa, source_type)| (ioa, source_type, ioa, target_type))
        .collect();

    let mut stations = srv.server.stations.write().await;
    let current = stations
        .get(&request.common_address)
        .ok_or_else(|| format!("station CA={} not found", request.common_address))?;
    let mut staged = current.clone();
    for &(ioa, source_type, target_ioa, target_type) in &migrations {
        staged
            .migrate_point(ioa, source_type, target_ioa, target_type)
            .map_err(|e| format!("failed to migrate IOA {ioa}: {e}"))?;
    }
    stations.insert(request.common_address, staged);

    // Update cross-CA control references only after the whole staged station
    // has passed validation.
    for station in stations.values_mut() {
        for def in &mut station.object_defs {
            if let Some(mapping) = def.mapping.as_mut() {
                if mapping.common_address != request.common_address {
                    continue;
                }
                if let Some((_, _, target_ioa, target_type)) =
                    migrations.iter().find(|(ioa, source_type, _, _)| {
                        mapping.ioa == *ioa && mapping.asdu_type == *source_type
                    })
                {
                    mapping.ioa = *target_ioa;
                    mapping.asdu_type = *target_type;
                }
            }
        }
    }
    drop(stations);

    for &(ioa, source_type, target_ioa, target_type) in &migrations {
        let active = active_tasks.iter().find(|(ca, task_ioa, task_type, _, _)| {
            *ca == request.common_address && *task_ioa == ioa && *task_type == source_type
        });
        srv.server
            .stop_point_mutation(request.common_address, ioa, source_type)
            .await;
        if let Some((_, _, _, params, period_ms)) = active {
            srv.server
                .start_point_mutation(
                    request.common_address,
                    target_ioa,
                    target_type,
                    *period_ms,
                    *params,
                )
                .await;
        }
    }
    Ok(migrations.len())
}

#[tauri::command]
pub async fn batch_migrate_data_point_types(
    state: State<'_, AppState>,
    request: BatchMigrateDataPointTypesRequest,
) -> Result<usize, String> {
    batch_migrate_data_point_types_impl(state.inner(), request).await
}

/// Remove several points in one locked write. Returns the count removed.
/// Unknown (ioa, type) pairs are skipped, so the call is idempotent.
#[tauri::command]
pub async fn batch_remove_data_points(
    state: State<'_, AppState>,
    server_id: String,
    common_address: u16,
    points: Vec<RemovePointTarget>,
) -> Result<usize, String> {
    let servers = state.servers.read().await;
    let srv = servers
        .get(&server_id)
        .ok_or_else(|| format!("server {} not found", server_id))?;

    // Resolve all ASDU types up front so a bad type aborts before any removal.
    let mut targets = Vec::with_capacity(points.len());
    for p in &points {
        targets.push((p.ioa, parse_asdu_type(&p.asdu_type)?));
    }

    let mut stations = srv.server.stations.write().await;
    let station = stations
        .get_mut(&common_address)
        .ok_or_else(|| format!("station CA={} not found", common_address))?;

    Ok(station.remove_points(&targets))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct BatchControlOptionsRequest {
    pub server_id: String,
    pub common_address: u16,
    pub points: Vec<RemovePointTarget>,
    /// 应用到每个点的 QU/QL 限定词(None = 不限制/任意)。仅 set_qualifier 时生效。
    #[serde(default)]
    pub command_qualifier: Option<u8>,
    #[serde(default)]
    pub set_qualifier: bool,
    /// 应用到每个点的 S/E 执行模式。仅 set_select_before_operate 时生效。
    #[serde(default)]
    pub select_before_operate: Option<bool>,
    #[serde(default)]
    pub set_select_before_operate: bool,
}

/// 批量设置控制点的 QU/QL 与 S/E(issue #28)。两个字段可独立选择是否应用;
/// 非控制点与位串命令点跳过(它们不携带 QU/QL/S-E),返回实际更新的点数。
pub(crate) async fn batch_update_control_options_impl(
    state: &AppState,
    request: BatchControlOptionsRequest,
) -> Result<usize, String> {
    if !request.set_qualifier && !request.set_select_before_operate {
        return Err("nothing to apply".to_string());
    }
    let servers = state.servers.read().await;
    let srv = servers
        .get(&request.server_id)
        .ok_or_else(|| format!("server {} not found", request.server_id))?;

    let mut targets = Vec::with_capacity(request.points.len());
    for p in &request.points {
        targets.push((p.ioa, parse_asdu_type(&p.asdu_type)?));
    }

    let mut stations = srv.server.stations.write().await;
    let station = stations
        .get_mut(&request.common_address)
        .ok_or_else(|| format!("station CA={} not found", request.common_address))?;

    let set: std::collections::HashSet<(u32, AsduTypeId)> = targets.into_iter().collect();
    let mut updated_keys: Vec<(u32, AsduTypeId)> = Vec::new();
    for def in station.object_defs.iter_mut() {
        if !set.contains(&(def.ioa, def.asdu_type)) {
            continue;
        }
        let qualifier = if request.set_qualifier {
            request.command_qualifier
        } else {
            def.command_qualifier
        };
        let sbo = if request.set_select_before_operate {
            request.select_before_operate
        } else {
            def.select_before_operate
        };
        // 逐点校验:位串命令与非控制点不携带这些字段,跳过而非中断整批。
        if validate_control_point_options(def.asdu_type, qualifier, sbo).is_err() {
            continue;
        }
        def.command_qualifier = qualifier;
        def.select_before_operate = sbo;
        updated_keys.push((def.ioa, def.asdu_type));
    }
    // def 修改必须 mark_changed,增量轮询(list_data_points_since)才看得到;
    // 否则前端缓存的 QU/SE 保持旧值,再次打开编辑框会静默回滚本次批量修改。
    for (ioa, t) in &updated_keys {
        station.data_points.mark_changed(*ioa, *t);
    }
    Ok(updated_keys.len())
}

#[tauri::command]
pub async fn batch_update_control_options(
    state: State<'_, AppState>,
    request: BatchControlOptionsRequest,
) -> Result<usize, String> {
    batch_update_control_options_impl(state.inner(), request).await
}

/// 按 `point` 当前值的类型把值串解析为 `DataPointValue`。单点改值与批量改值共用,
/// 解析失败返回 Err(不写入)。
fn parse_value_for(point: &DataPoint, value: &str) -> Result<DataPointValue, String> {
    let new_value = match &point.value {
        DataPointValue::SinglePoint { .. } => {
            let v = value.parse::<bool>().or_else(|_| {
                match value {
                    "1" | "true" | "ON" | "on" => Ok(true),
                    "0" | "false" | "OFF" | "off" => Ok(false),
                    _ => Err(format!("invalid bool: {}", value)),
                }
            }).map_err(|e| format!("{}", e))?;
            DataPointValue::SinglePoint { value: v }
        }
        DataPointValue::DoublePoint { .. } => {
            let v = value.parse::<u8>().map_err(|e| format!("{}", e))?;
            DataPointValue::DoublePoint { value: v }
        }
        DataPointValue::Normalized { .. } => {
            // 输入与显示对齐:用户输入原始 NVA 整数 (-32768..32767),内部仍存 [-1,1) f32。
            // 上送编码用 `value * 32767`(见 slave.rs),故此处反向换算 `nva / 32767`。
            let nva = value.trim().parse::<i16>().map_err(|e| format!("{}", e))?;
            DataPointValue::Normalized { value: nva as f32 / 32767.0 }
        }
        DataPointValue::Scaled { .. } => {
            let v = value.parse::<i16>().map_err(|e| format!("{}", e))?;
            DataPointValue::Scaled { value: v }
        }
        DataPointValue::ShortFloat { .. } => {
            let v = value.parse::<f32>().map_err(|e| format!("{}", e))?;
            DataPointValue::ShortFloat { value: v }
        }
        DataPointValue::IntegratedTotal { carry, sequence, .. } => {
            let v = value.parse::<i32>().map_err(|e| format!("{}", e))?;
            DataPointValue::IntegratedTotal { value: v, carry: *carry, sequence: *sequence }
        }
        _ => return Err("unsupported value type".to_string()),
    };
    Ok(new_value)
}

#[tauri::command]
pub async fn update_data_point(
    state: State<'_, AppState>,
    server_id: String,
    common_address: u16,
    ioa: u32,
    asdu_type: String,
    value: String,
) -> Result<(), String> {
    let servers = state.servers.read().await;
    let srv = servers
        .get(&server_id)
        .ok_or_else(|| format!("server {} not found", server_id))?;

    let asdu = parse_asdu_type(&asdu_type)?;

    let mut stations = srv.server.stations.write().await;
    let station = stations
        .get_mut(&common_address)
        .ok_or_else(|| format!("station CA={} not found", common_address))?;

    let point = station.data_points.get_mut(ioa, asdu)
        .ok_or_else(|| format!("IOA {} type {} not found", ioa, asdu_type))?;

    let new_value = parse_value_for(point, &value)?;

    point.value = new_value;
    point.timestamp = Some(chrono::Utc::now());
    // Stamp the change so incremental polling (list_data_points_since) sees it —
    // a bare get_mut value write does not bump update_seq.
    station.data_points.mark_changed(ioa, asdu);

    drop(stations);
    srv.server.queue_spontaneous(common_address, &[(ioa, asdu)]).await;

    Ok(())
}

/// 设置点位的品质描述词(IV/NT/SB/BL/OV)。与 `update_data_point` 解耦:
/// 后者只改值,本命令只改品质,改后触发一次自发上送让主站及时收到。
#[tauri::command]
pub async fn set_data_point_quality(
    state: State<'_, AppState>,
    server_id: String,
    common_address: u16,
    ioa: u32,
    asdu_type: String,
    ov: bool,
    bl: bool,
    sb: bool,
    nt: bool,
    iv: bool,
) -> Result<(), String> {
    let servers = state.servers.read().await;
    let srv = servers
        .get(&server_id)
        .ok_or_else(|| format!("server {} not found", server_id))?;

    let asdu = parse_asdu_type(&asdu_type)?;

    let mut stations = srv.server.stations.write().await;
    let station = stations
        .get_mut(&common_address)
        .ok_or_else(|| format!("station CA={} not found", common_address))?;

    let point = station.data_points.get_mut(ioa, asdu)
        .ok_or_else(|| format!("IOA {} type {} not found", ioa, asdu_type))?;

    point.quality = QualityFlags { ov, bl, sb, nt, iv };
    point.timestamp = Some(chrono::Utc::now());
    // 与 update_data_point 一致:get_mut 写入不会自动 bump update_seq,
    // 必须 mark_changed,增量轮询(list_data_points_since)才看得到品质变化。
    station.data_points.mark_changed(ioa, asdu);

    drop(stations);
    srv.server.queue_spontaneous(common_address, &[(ioa, asdu)]).await;

    Ok(())
}

/// 站级批量写品质(无 async/锁/Tauri,便于单测)。OV 仅测量类;未知 (ioa,type) 跳过。
fn apply_batch_quality(
    station: &mut Station,
    targets: &[(u32, AsduTypeId)],
    ov: bool,
    bl: bool,
    sb: bool,
    nt: bool,
    iv: bool,
) -> Vec<(u32, AsduTypeId)> {
    let mut changed = Vec::new();
    for (ioa, asdu) in targets {
        if let Some(point) = station.data_points.get_mut(*ioa, *asdu) {
            let measured = asdu.category().is_measured();
            point.quality = QualityFlags { ov: ov && measured, bl, sb, nt, iv };
            point.timestamp = Some(chrono::Utc::now());
            station.data_points.mark_changed(*ioa, *asdu);
            changed.push((*ioa, *asdu));
        }
    }
    changed
}

/// 站级批量写值(无 async/锁/Tauri):同分类 + 全或无。先全量校验再全量写入,
/// 任一步出错返回 Err 且不修改任何点。
fn apply_batch_value(
    station: &mut Station,
    targets: &[(u32, AsduTypeId)],
    value: &str,
) -> Result<Vec<(u32, AsduTypeId)>, String> {
    if targets.is_empty() {
        return Ok(Vec::new());
    }
    let first_cat = targets[0].1.category();
    let mut parsed: Vec<(u32, AsduTypeId, DataPointValue)> = Vec::with_capacity(targets.len());
    for (ioa, asdu) in targets {
        if asdu.category() != first_cat {
            return Err(format!(
                "批量写值要求同分类:{} 与 {} 不同类",
                first_cat.name(),
                asdu.category().name()
            ));
        }
        let point = station
            .data_points
            .get_mut(*ioa, *asdu)
            .ok_or_else(|| format!("IOA {} type {} not found", ioa, asdu.name()))?;
        let nv = parse_value_for(point, value)?;
        parsed.push((*ioa, *asdu, nv));
    }
    let mut changed = Vec::with_capacity(parsed.len());
    for (ioa, asdu, nv) in parsed {
        if let Some(point) = station.data_points.get_mut(ioa, asdu) {
            point.value = nv;
            point.timestamp = Some(chrono::Utc::now());
            station.data_points.mark_changed(ioa, asdu);
            changed.push((ioa, asdu));
        }
    }
    Ok(changed)
}

/// 批量设置一组点位的品质(IV/NT/SB/BL/OV,绝对覆盖)。OV 仅落到测量类目标,
/// 非测量类忽略 OV。未知 (ioa,type) 跳过(幂等)。返回实际改动的点数。
#[tauri::command]
pub async fn batch_set_data_point_quality(
    state: State<'_, AppState>,
    server_id: String,
    common_address: u16,
    points: Vec<RemovePointTarget>,
    ov: bool,
    bl: bool,
    sb: bool,
    nt: bool,
    iv: bool,
) -> Result<usize, String> {
    let servers = state.servers.read().await;
    let srv = servers
        .get(&server_id)
        .ok_or_else(|| format!("server {} not found", server_id))?;

    // 先把类型解析齐,坏类型在改动前就中止。
    let mut targets = Vec::with_capacity(points.len());
    for p in &points {
        targets.push((p.ioa, parse_asdu_type(&p.asdu_type)?));
    }

    let changed = {
        let mut stations = srv.server.stations.write().await;
        let station = stations
            .get_mut(&common_address)
            .ok_or_else(|| format!("station CA={} not found", common_address))?;
        apply_batch_quality(station, &targets, ov, bl, sb, nt, iv)
    };
    srv.server.queue_spontaneous(common_address, &changed).await;
    Ok(changed.len())
}

/// 批量为一组点位写入同一个值。要求所有目标同分类;跨分类或任一解析失败 → 返回
/// 错误且不修改任何点(先全量校验,后全量写入)。返回实际改动的点数。
#[tauri::command]
pub async fn batch_update_data_points(
    state: State<'_, AppState>,
    server_id: String,
    common_address: u16,
    points: Vec<RemovePointTarget>,
    value: String,
) -> Result<usize, String> {
    if points.is_empty() {
        return Ok(0);
    }
    let servers = state.servers.read().await;
    let srv = servers
        .get(&server_id)
        .ok_or_else(|| format!("server {} not found", server_id))?;

    let mut targets = Vec::with_capacity(points.len());
    for p in &points {
        targets.push((p.ioa, parse_asdu_type(&p.asdu_type)?));
    }

    let changed = {
        let mut stations = srv.server.stations.write().await;
        let station = stations
            .get_mut(&common_address)
            .ok_or_else(|| format!("station CA={} not found", common_address))?;
        apply_batch_value(station, &targets, &value)?
    };
    srv.server.queue_spontaneous(common_address, &changed).await;
    Ok(changed.len())
}

/// 子站把归一化值显示为线上原始 NVA 整数 (-32768..32767),而非 [-1,1) 小数,
/// 与主站数据表一致、便于和报文逐字节对照。编解码用 `nva as f32 / 32767.0`
/// (见 decode.rs / slave.rs),故 `round(value * 32767)` 反向无损还原:f32 往返
/// 误差 < 0.002,远小于 0.5。
fn normalized_raw_string(value: f32) -> String {
    ((value * 32767.0).round() as i16).to_string()
}

fn data_point_value_string(p: &DataPoint) -> String {
    match &p.value {
        DataPointValue::Normalized { value } => normalized_raw_string(*value),
        // UI localization belongs to the frontend. Returning the stable DPI
        // code also keeps inline editing numeric instead of feeding Chinese
        // display labels back into the u8 parser.
        DataPointValue::DoublePoint { value } => value.to_string(),
        _ => p.value.display(),
    }
}

fn data_point_timestamp_string(p: &DataPoint) -> Option<String> {
    // DataPoint.timestamp 内部存 UTC 便于无歧义比较；展示给用户时转为
    // 本地时区,这样 UI 看到的"时间戳"和系统挂钟一致。
    p.timestamp.map(|t| t.with_timezone(&chrono::Local).format("%H:%M:%S%.3f").to_string())
}

/// Map a core `DataPoint` to the serialisable `DataPointInfo` the UI consumes.
fn data_point_to_info(
    p: &DataPoint,
    def_map: &std::collections::HashMap<(u32, AsduTypeId), &InformationObjectDef>,
) -> DataPointInfo {
    let def = def_map.get(&(p.ioa, p.asdu_type));
    DataPointInfo {
        ioa: p.ioa,
        asdu_type: p.asdu_type.name().to_string(),
        category: p.asdu_type.category().key().to_string(),
        name: def.map(|d| d.name.clone()).unwrap_or_default(),
        comment: def.map(|d| d.comment.clone()).unwrap_or_default(),
        mapping_common_address: def.and_then(|d| d.mapping.map(|m| m.common_address)),
        mapping_ioa: def.and_then(|d| d.mapping.map(|m| m.ioa)),
        mapping_asdu_type: def.and_then(|d| d.mapping.map(|m| m.asdu_type.name().to_string())),
        command_qualifier: def.and_then(|d| d.command_qualifier),
        select_before_operate: def.and_then(|d| d.select_before_operate),
        value: data_point_value_string(p),
        quality_ov: p.quality.ov,
        quality_bl: p.quality.bl,
        quality_sb: p.quality.sb,
        quality_nt: p.quality.nt,
        quality_iv: p.quality.iv,
        timestamp: data_point_timestamp_string(p),
    }
}

fn data_point_to_value_snapshot(p: &DataPoint) -> DataPointValueSnapshot {
    DataPointValueSnapshot {
        ioa: p.ioa,
        asdu_type: p.asdu_type.name().to_string(),
        value: data_point_value_string(p),
        quality_ov: p.quality.ov,
        quality_bl: p.quality.bl,
        quality_sb: p.quality.sb,
        quality_nt: p.quality.nt,
        quality_iv: p.quality.iv,
        timestamp: data_point_timestamp_string(p),
    }
}

#[tauri::command]
pub async fn list_data_points(
    state: State<'_, AppState>,
    server_id: String,
    common_address: u16,
    _category: Option<String>,
) -> Result<Vec<DataPointInfo>, String> {
    // 仅在极短时间内持有全局 servers 锁:克隆该服务器的 stations 句柄(Arc)后立即释放,
    // 避免把下面 O(N) 的 def_map 构建与序列化压在全局锁内,拖住 start_server/stop_server
    // 等写操作(它们需要 servers 的写锁)。
    let stations_arc = {
        let servers = state.servers.read().await;
        let srv = servers
            .get(&server_id)
            .ok_or_else(|| format!("server {} not found", server_id))?;
        srv.server.stations.clone()
    };
    let stations = stations_arc.read().await;
    let station = stations
        .get(&common_address)
        .ok_or_else(|| format!("station CA={} not found", common_address))?;

    let points = station.data_points.all_sorted();
    let defs = &station.object_defs;

    // Build O(1) lookup map instead of O(n) linear search per point
    let def_map: std::collections::HashMap<(u32, AsduTypeId), &InformationObjectDef> = defs.iter()
        .map(|d| ((d.ioa, d.asdu_type), d))
        .collect();

    let result: Vec<DataPointInfo> = points
        .into_iter()
        .map(|p| data_point_to_info(p, &def_map))
        .collect();

    Ok(result)
}

/// 单点查询:按 (server, CA, IOA, asdu_type) 返回一个点的详情,或 `None`。
/// 替代 ValuePanel 选点时全量拉取 `list_data_points` 再前端 `find` ——
/// 大点位场景(上万点)那样单次序列化耗时数百 ms 且压着全局锁。
/// 与 #22 一致:短暂持锁克隆 stations 句柄后释放,再做查询。
#[tauri::command]
pub async fn get_data_point(
    state: State<'_, AppState>,
    server_id: String,
    common_address: u16,
    ioa: u32,
    asdu_type: String,
) -> Result<Option<DataPointInfo>, String> {
    let ty = parse_asdu_type(&asdu_type)?;
    let stations_arc = {
        let servers = state.servers.read().await;
        let srv = servers
            .get(&server_id)
            .ok_or_else(|| format!("server {} not found", server_id))?;
        srv.server.stations.clone()
    };
    let stations = stations_arc.read().await;
    let station = stations
        .get(&common_address)
        .ok_or_else(|| format!("station CA={} not found", common_address))?;

    let Some(p) = station.data_points.get(ioa, ty) else {
        return Ok(None);
    };
    // 只取该点对应的 def(若有),复用 data_point_to_info。
    let def_map: std::collections::HashMap<(u32, AsduTypeId), &InformationObjectDef> =
        station.object_defs.iter()
            .filter(|d| d.ioa == ioa && d.asdu_type == ty)
            .map(|d| ((d.ioa, d.asdu_type), d))
            .collect();
    Ok(Some(data_point_to_info(p, &def_map)))
}

/// Fetch only the mutable runtime fields for an explicit set of point keys.
///
/// This is the fast path used while periodic mutations are active. It performs
/// O(k) hash lookups and deliberately avoids building the O(N) definition map
/// used by the full/incremental table queries.
pub(crate) async fn get_data_point_values_impl(
    state: &AppState,
    server_id: &str,
    common_address: u16,
    points: Vec<RemovePointTarget>,
) -> Result<Vec<DataPointValueSnapshot>, String> {
    let mut targets = Vec::with_capacity(points.len());
    let mut seen = std::collections::HashSet::with_capacity(points.len());
    for point in points {
        let key = (point.ioa, parse_asdu_type(&point.asdu_type)?);
        if seen.insert(key) {
            targets.push(key);
        }
    }

    let stations_arc = {
        let servers = state.servers.read().await;
        let srv = servers
            .get(server_id)
            .ok_or_else(|| format!("server {} not found", server_id))?;
        srv.server.stations.clone()
    };
    let stations = stations_arc.read().await;
    let station = stations
        .get(&common_address)
        .ok_or_else(|| format!("station CA={} not found", common_address))?;

    Ok(targets
        .into_iter()
        .filter_map(|(ioa, asdu_type)| station.data_points.get(ioa, asdu_type))
        .map(data_point_to_value_snapshot)
        .collect())
}

#[tauri::command]
pub async fn get_data_point_values(
    state: State<'_, AppState>,
    server_id: String,
    common_address: u16,
    points: Vec<RemovePointTarget>,
) -> Result<Vec<DataPointValueSnapshot>, String> {
    get_data_point_values_impl(state.inner(), &server_id, common_address, points).await
}

/// Incremental variant of `list_data_points`: returns only points whose
/// `update_seq` exceeds `since_seq`, so a polling UI transfers a handful of
/// changed rows instead of the whole (potentially 80k-row) table each tick.
/// `total_count` lets the caller detect deletions via a size mismatch.
#[tauri::command]
pub async fn list_data_points_since(
    state: State<'_, AppState>,
    server_id: String,
    common_address: u16,
    since_seq: u64,
) -> Result<IncrementalDataResponse, String> {
    // 同 list_data_points:短暂持锁克隆 stations 句柄后释放,O(N) 的 changed_since
    // 序列化在全局锁外进行,不阻塞 start_server/stop_server 的写锁。
    let stations_arc = {
        let servers = state.servers.read().await;
        let srv = servers
            .get(&server_id)
            .ok_or_else(|| format!("server {} not found", server_id))?;
        srv.server.stations.clone()
    };
    let stations = stations_arc.read().await;
    let station = stations
        .get(&common_address)
        .ok_or_else(|| format!("station CA={} not found", common_address))?;

    let def_map: std::collections::HashMap<(u32, AsduTypeId), &InformationObjectDef> =
        station.object_defs.iter()
            .map(|d| ((d.ioa, d.asdu_type), d))
            .collect();

    let points: Vec<DataPointInfo> = station.data_points
        .changed_since(since_seq)
        .into_iter()
        .map(|p| data_point_to_info(p, &def_map))
        .collect();

    Ok(IncrementalDataResponse {
        seq: station.data_points.current_seq(),
        total_count: station.data_points.len(),
        points,
    })
}

// ---------------------------------------------------------------------------
// Log Commands
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn get_communication_logs(
    state: State<'_, AppState>,
    server_id: String,
) -> Result<Vec<LogEntry>, String> {
    // 短暂持锁克隆 log_collector 句柄后释放:日志可达上万条,get_all 的克隆 + 序列化
    // 不应压在全局 servers 锁内(该命令每 2s 被日志面板轮询)。
    let log_collector = {
        let servers = state.servers.read().await;
        let srv = servers
            .get(&server_id)
            .ok_or_else(|| format!("server {} not found", server_id))?;
        srv.log_collector.clone()
    };
    Ok(log_collector.get_all().await)
}

#[tauri::command]
pub async fn clear_communication_logs(
    state: State<'_, AppState>,
    server_id: String,
) -> Result<(), String> {
    let servers = state.servers.read().await;
    let srv = servers
        .get(&server_id)
        .ok_or_else(|| format!("server {} not found", server_id))?;
    srv.log_collector.clear().await;
    Ok(())
}

#[tauri::command]
pub async fn export_logs_csv(
    state: State<'_, AppState>,
    server_id: String,
) -> Result<String, String> {
    let log_collector = {
        let servers = state.servers.read().await;
        let srv = servers
            .get(&server_id)
            .ok_or_else(|| format!("server {} not found", server_id))?;
        srv.log_collector.clone()
    };
    Ok(log_collector.export_csv().await)
}

fn selected_logs_csv(entries: Option<Vec<LogEntry>>) -> Option<String> {
    entries.map(|entries| LogCollector::export_entries_csv(&entries))
}

/// 将日志直接写入用户通过原生保存对话框选择的路径。WebView 中使用 Blob +
/// `<a download>` 在 Tauri/Windows WebView2 下不会可靠触发系统下载，因此文件写入
/// 必须由 Rust 后端完成。UTF-8 BOM 让 Windows Excel 能正确识别中英文详情。
#[tauri::command]
pub async fn save_logs_csv(
    state: State<'_, AppState>,
    server_id: String,
    path: String,
    entries: Option<Vec<LogEntry>>,
) -> Result<(), String> {
    let csv = match selected_logs_csv(entries) {
        Some(csv) => csv,
        None => export_logs_csv(state, server_id).await?,
    };
    let content = format!("\u{FEFF}{}", csv);
    std::fs::write(&path, content).map_err(|e| format!("写入 CSV 失败: {e}"))
}

// ---------------------------------------------------------------------------
// Simulation Commands
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct RandomMutateRequest {
    pub server_id: String,
    pub common_address: u16,
}

#[tauri::command]
pub async fn random_mutate_data_points(
    state: State<'_, AppState>,
    request: RandomMutateRequest,
) -> Result<u32, String> {
    let servers = state.servers.read().await;
    let srv = servers
        .get(&request.server_id)
        .ok_or_else(|| format!("server {} not found", request.server_id))?;

    let mut stations = srv.server.stations.write().await;
    let station = stations
        .get_mut(&request.common_address)
        .ok_or_else(|| format!("station CA={} not found", request.common_address))?;

    let (mutated, changed_ioas) = {
        let mut rng = rand::rng();
        let mut mutated = 0u32;
        let mut changed_ioas: Vec<(u32, AsduTypeId)> = Vec::new();

        let keys: Vec<(u32, AsduTypeId)> = station.data_points.points.keys().copied().collect();
        let count = (keys.len() * 30 / 100).max(3).min(keys.len());

        let mut pick = keys;
        for i in (1..pick.len()).rev() {
            let j = rng.random_range(0..=i);
            pick.swap(i, j);
        }

        for &(ioa, asdu_type) in &pick[..count] {
            if let Some(point) = station.data_points.get_mut(ioa, asdu_type) {
                point.value = match &point.value {
                    DataPointValue::SinglePoint { value } => {
                        DataPointValue::SinglePoint { value: !value }
                    }
                    DataPointValue::DoublePoint { value } => {
                        DataPointValue::DoublePoint { value: if *value == 1 { 2 } else { 1 } }
                    }
                    DataPointValue::Normalized { value } => {
                        let delta: f32 = rng.random_range(-0.1..0.1);
                        DataPointValue::Normalized { value: (*value + delta).clamp(-1.0, 1.0) }
                    }
                    DataPointValue::Scaled { value } => {
                        let delta: i16 = rng.random_range(-100..100);
                        DataPointValue::Scaled { value: value.saturating_add(delta) }
                    }
                    DataPointValue::ShortFloat { value } => {
                        let delta: f32 = rng.random_range(-10.0..10.0);
                        DataPointValue::ShortFloat { value: value + delta }
                    }
                    DataPointValue::IntegratedTotal { value, carry, sequence } => {
                        let delta: i32 = rng.random_range(0..100);
                        DataPointValue::IntegratedTotal {
                            value: value + delta,
                            carry: *carry,
                            sequence: *sequence,
                        }
                    }
                    other => other.clone(),
                };
                point.timestamp = Some(chrono::Utc::now());
                changed_ioas.push((ioa, asdu_type));
                mutated += 1;
            }
        }
        (mutated, changed_ioas)
    }; // rng dropped here

    // Stamp every mutated point for incremental polling.
    for &(ioa, asdu_type) in &changed_ioas {
        station.data_points.mark_changed(ioa, asdu_type);
    }

    drop(stations);

    // 按 RemoteOperationConfig.random_pacing 分批 queue_spontaneous,
    // 每发 batch_size 个 IOA 后 sleep delay_ms。batch_size=0 视为一次性发送。
    let pacing = srv.server.get_remote_ops().await.random_pacing;
    let batch_size = pacing.batch_size.max(1) as usize;
    let delay = std::time::Duration::from_millis(pacing.delay_ms as u64);
    let mut idx = 0;
    while idx < changed_ioas.len() {
        let end = (idx + batch_size).min(changed_ioas.len());
        srv.server.queue_spontaneous(request.common_address, &changed_ioas[idx..end]).await;
        idx = end;
        if idx < changed_ioas.len() && pacing.delay_ms > 0 {
            tokio::time::sleep(delay).await;
        }
    }

    Ok(mutated)
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct CyclicConfigRequest {
    pub server_id: String,
    pub common_address: u16,
    pub enabled: bool,
    pub interval_ms: u32,
}

#[tauri::command]
pub async fn set_cyclic_config(
    state: State<'_, AppState>,
    request: CyclicConfigRequest,
) -> Result<(), String> {
    use iec104sim_core::slave::CyclicConfig;
    let servers = state.servers.read().await;
    let srv = servers
        .get(&request.server_id)
        .ok_or_else(|| format!("server {} not found", request.server_id))?;
    srv.server
        .set_cyclic_config(
            request.common_address,
            CyclicConfig { enabled: request.enabled, interval_ms: request.interval_ms },
        )
        .await
        .map_err(|e| format!("{:?}", e))
}

// ---------------------------------------------------------------------------
// Remote Operation Configuration Commands (远动运行参数)
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ProtocolTimingRequest {
    pub server_id: String,
    pub timing: ProtocolTimingConfig,
}

#[tauri::command]
pub async fn set_protocol_timing(
    state: State<'_, AppState>,
    request: ProtocolTimingRequest,
) -> Result<Vec<iec104sim_core::timing::TimingCorrection>, String> {
    let servers = state.servers.read().await;
    let srv = servers
        .get(&request.server_id)
        .ok_or_else(|| format!("server {} not found", request.server_id))?;
    // 后端权威规范化。前端已做编辑感知 C3,正常情况下这里返回空;
    // 仅当调用方(脚本/旧值)绕过前端时才会产生 corrections。
    let mut timing = request.timing;
    let corrections = timing.normalize();
    srv.server.set_protocol_timing(timing).await;
    Ok(corrections)
}

#[tauri::command]
pub async fn get_protocol_timing(
    state: State<'_, AppState>,
    server_id: String,
) -> Result<ProtocolTimingConfig, String> {
    let servers = state.servers.read().await;
    let srv = servers
        .get(&server_id)
        .ok_or_else(|| format!("server {} not found", server_id))?;
    Ok(srv.server.get_protocol_timing().await)
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct RemoteOpsRequest {
    pub server_id: String,
    pub ops: RemoteOperationConfig,
}

#[tauri::command]
pub async fn set_remote_operation_config(
    state: State<'_, AppState>,
    request: RemoteOpsRequest,
) -> Result<(), String> {
    let servers = state.servers.read().await;
    let srv = servers
        .get(&request.server_id)
        .ok_or_else(|| format!("server {} not found", request.server_id))?;
    srv.server.set_remote_ops(request.ops).await;
    Ok(())
}

#[tauri::command]
pub async fn get_remote_operation_config(
    state: State<'_, AppState>,
    server_id: String,
) -> Result<RemoteOperationConfig, String> {
    let servers = state.servers.read().await;
    let srv = servers
        .get(&server_id)
        .ok_or_else(|| format!("server {} not found", server_id))?;
    Ok(srv.server.get_remote_ops().await)
}

/// 解析前端传入的变位模式字符串(serde snake_case:flip/increment/decrement/random)。
/// 缺省或无法识别时按 flip 处理,保持旧行为。
fn parse_mutation_mode(s: Option<&str>) -> MutationMode {
    match s {
        Some("increment") => MutationMode::Increment,
        Some("decrement") => MutationMode::Decrement,
        Some("random") => MutationMode::Random,
        _ => MutationMode::Flip,
    }
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn start_point_mutation(
    state: State<'_, AppState>,
    server_id: String,
    common_address: u16,
    ioa: u32,
    asdu_type: String,
    period_ms: u32,
    mode: Option<String>,
    step: Option<f64>,
    min: Option<f64>,
    max: Option<f64>,
) -> Result<(), String> {
    let asdu = parse_asdu_type(&asdu_type)?;
    let params = MutationParams {
        mode: parse_mutation_mode(mode.as_deref()),
        step: step.unwrap_or(0.0),
        min: min.unwrap_or(0.0),
        max: max.unwrap_or(0.0),
    };
    let servers = state.servers.read().await;
    let srv = servers
        .get(&server_id)
        .ok_or_else(|| format!("server {} not found", server_id))?;
    srv.server
        .start_point_mutation(common_address, ioa, asdu, period_ms, params)
        .await;
    Ok(())
}

#[tauri::command]
pub async fn stop_point_mutation(
    state: State<'_, AppState>,
    server_id: String,
    common_address: u16,
    ioa: u32,
    asdu_type: String,
) -> Result<(), String> {
    let asdu = parse_asdu_type(&asdu_type)?;
    let servers = state.servers.read().await;
    let srv = servers
        .get(&server_id)
        .ok_or_else(|| format!("server {} not found", server_id))?;
    srv.server
        .stop_point_mutation(common_address, ioa, asdu)
        .await;
    Ok(())
}

/// list_point_mutations 返回项。asdu_type 用 .name() 大写显示名,
/// 与 list_data_points 的 DataPointInfo.asdu_type 一致,前端可直接拼 key。
/// 返回完整任务参数，供模拟设置面板回显和更新。
#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct PointMutationInfo {
    pub ioa: u32,
    pub asdu_type: String,
    pub mode: String,
    pub period_ms: u32,
    pub step: f64,
    pub min: f64,
    pub max: f64,
}

fn mutation_mode_str(mode: MutationMode) -> &'static str {
    match mode {
        MutationMode::Flip => "flip",
        MutationMode::Increment => "increment",
        MutationMode::Decrement => "decrement",
        MutationMode::Random => "random",
    }
}

#[tauri::command]
pub async fn list_point_mutations(
    state: State<'_, AppState>,
    server_id: String,
    common_address: u16,
) -> Result<Vec<PointMutationInfo>, String> {
    let servers = state.servers.read().await;
    let srv = servers
        .get(&server_id)
        .ok_or_else(|| format!("server {} not found", server_id))?;
    let active = srv.server.list_point_mutations_with_params().await;
    Ok(active
        .into_iter()
        .filter(|(ca, _, _, _, _)| *ca == common_address)
        .map(|(_, ioa, t, params, period_ms)| PointMutationInfo {
            ioa,
            asdu_type: t.name().to_string(),
            mode: mutation_mode_str(params.mode).to_string(),
            period_ms,
            step: params.step,
            min: params.min,
            max: params.max,
        })
        .collect())
}

// ---------------------------------------------------------------------------
// State Persistence Commands
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn save_config(
    state: State<'_, AppState>,
    path: String,
) -> Result<(), String> {
    use iec104sim_core::config::{SlaveConfigFile, SlaveServerConfig, SlaveStationConfig};

    let json = {
        let servers = state.servers.read().await;
        let mut out = Vec::new();
        for (_id, srv_state) in servers.iter() {
            let stations = srv_state.server.stations.read().await;
            let mut st = Vec::new();
            for (_ca, station) in stations.iter() {
                st.push(SlaveStationConfig {
                    common_address: station.common_address,
                    name: station.name.clone(),
                    object_defs: station.object_defs.clone(),
                });
            }
            out.push(SlaveServerConfig {
                bind_address: srv_state.server.transport.bind_address.clone(),
                port: srv_state.server.transport.port,
                stations: st,
                protocol_timing: srv_state.server.get_protocol_timing().await,
                remote_ops: srv_state.server.get_remote_ops().await,
            });
        }
        SlaveConfigFile::new(out).to_json()?
    };
    std::fs::write(&path, json).map_err(|e| format!("写入文件失败: {e}"))
}

#[tauri::command]
pub async fn load_config(
    state: State<'_, AppState>,
    app_handle: AppHandle,
    path: String,
) -> Result<usize, String> {
    use iec104sim_core::config::SlaveConfigFile;

    let content = std::fs::read_to_string(&path)
        .map_err(|e| format!("读取文件失败: {e}"))?;
    let file = SlaveConfigFile::from_json(&content)?;

    let mut imported = 0usize;
    let mut corrected_events: Vec<TimingCorrectedEvent> = Vec::new();
    for srv in file.servers {
        let id = {
            let mut counter = state.next_server_id.write().await;
            let id = format!("server_{}", *counter);
            *counter += 1;
            id
        };
        let endpoint = format!("{}:{}", srv.bind_address, srv.port);
        let transport = SlaveTransportConfig {
            bind_address: srv.bind_address,
            port: srv.port,
            tls: Default::default(),
        };
        let log_collector = Arc::new(LogCollector::new());
        let server = SlaveServer::new(transport).with_log_collector(log_collector.clone());
        // 加站点前先恢复服务器级配置,确保后续突发上送按目标参数走。
        // 后端权威:规范化旧配置,收集纠正以提示用户。
        let mut timing = srv.protocol_timing;
        let corrections = timing.normalize();
        server.set_protocol_timing(timing).await;
        if !corrections.is_empty() {
            corrected_events.push(TimingCorrectedEvent { endpoint, corrections });
        }
        server.set_remote_ops(srv.remote_ops).await;
        for st in srv.stations {
            let mut station = Station::new(st.common_address, st.name);
            for def in st.object_defs {
                let _ = station.add_point(def);
            }
            let _ = server.add_station(station).await;
        }
        state.servers.write().await.insert(
            id,
            SlaveServerState { server, log_collector },
        );
        imported += 1;
    }
    // 把导入时的时序纠正上抛,让用户知道加载的配置被调整过。
    if !corrected_events.is_empty() {
        let _ = app_handle.emit("config-timing-corrected", &corrected_events);
    }
    Ok(imported)
}

/// `config-timing-corrected` 事件载荷:slave `load_config` 导入时发生的时序纠正。
#[derive(Clone, serde::Serialize)]
struct TimingCorrectedEvent {
    endpoint: String,
    corrections: Vec<iec104sim_core::timing::TimingCorrection>,
}

// ---------------------------------------------------------------------------
// Tool Commands
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn parse_hex(data: String) -> Result<Vec<u8>, String> {
    iec104sim_core::tools::parse_hex_string(&data)
        .map_err(|e| format!("{}", e))
}

#[tauri::command]
pub fn parse_apci(data: String) -> Result<String, String> {
    let bytes = iec104sim_core::tools::parse_hex_string(&data)
        .map_err(|e| format!("{}", e))?;
    let frame = iec104sim_core::frame::parse_apci(&bytes)
        .map_err(|e| format!("{}", e))?;
    Ok(iec104sim_core::frame::format_frame_summary(&frame))
}

#[tauri::command]
pub fn parse_frame_full(data: String) -> Result<iec104sim_core::decode::ParsedFrame, String> {
    let bytes = iec104sim_core::tools::parse_hex_string(&data)
        .map_err(|e| format!("{}", e))?;
    iec104sim_core::decode::parse_frame_full(&bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use iec104sim_core::slave::SlaveTlsConfig;
    use std::collections::HashMap;

    #[test]
    fn selected_log_export_distinguishes_missing_and_empty_entries() {
        assert!(selected_logs_csv(None).is_none());
        assert_eq!(
            selected_logs_csv(Some(Vec::new())).unwrap(),
            "Timestamp,Direction,FrameType,Detail,RawBytes\n",
        );
    }

    fn free_port() -> u16 {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.local_addr().unwrap().port()
    }

    fn test_transport(port: u16) -> SlaveTransportConfig {
        SlaveTransportConfig {
            bind_address: "127.0.0.1".to_string(),
            port,
            tls: SlaveTlsConfig {
                enabled: false,
                cert_file: String::new(),
                key_file: String::new(),
                ca_file: String::new(),
                require_client_cert: false,
                pkcs12_file: String::new(),
                pkcs12_password: String::new(),
            },
        }
    }

    fn ctl_def(ioa: u32, asdu_type: AsduTypeId) -> InformationObjectDef {
        InformationObjectDef {
            ioa,
            asdu_type,
            category: asdu_type.category(),
            name: String::new(),
            comment: String::new(),
            mapping: None,
            command_qualifier: None,
            select_before_operate: None,
        }
    }

    async fn state_with_server(server: SlaveServer, id: &str) -> AppState {
        let state = AppState::new();
        state.servers.write().await.insert(
            id.to_string(),
            SlaveServerState {
                server,
                log_collector: Arc::new(LogCollector::new()),
            },
        );
        state
    }

    #[test]
    fn data_point_dtos_use_stable_numeric_dpi_codes() {
        let def_map: HashMap<(u32, AsduTypeId), &InformationObjectDef> = HashMap::new();
        let mut point = DataPoint::new(1, AsduTypeId::MDpNa1);

        for dpi in [0, 1, 2, 3] {
            point.value = DataPointValue::DoublePoint { value: dpi };
            let expected = dpi.to_string();
            assert_eq!(data_point_to_info(&point, &def_map).value, expected);
            assert_eq!(data_point_to_value_snapshot(&point).value, expected);
        }
    }

    #[test]
    fn point_mutation_mode_round_trips_random() {
        assert_eq!(parse_mutation_mode(Some("random")), MutationMode::Random);
        assert_eq!(mutation_mode_str(MutationMode::Random), "random");
        assert_eq!(parse_mutation_mode(Some("unknown")), MutationMode::Flip);
    }

    #[tokio::test]
    async fn get_data_point_values_fetches_only_requested_runtime_snapshots() {
        let server = SlaveServer::new(test_transport(free_port()));
        let mut station = Station::new(1, "st");
        station.add_point(ctl_def(1, AsduTypeId::MSpNa1)).unwrap();
        station.add_point(ctl_def(2, AsduTypeId::MMeNc1)).unwrap();
        {
            let point = station.data_points.get_mut(1, AsduTypeId::MSpNa1).unwrap();
            point.value = DataPointValue::SinglePoint { value: true };
            point.quality = QualityFlags { iv: true, sb: true, ..Default::default() };
            point.timestamp = Some(chrono::Utc::now());
        }
        server.add_station(station).await.unwrap();
        let state = state_with_server(server, "s1").await;

        let snapshots = get_data_point_values_impl(
            &state,
            "s1",
            1,
            vec![
                RemovePointTarget { ioa: 1, asdu_type: "M_SP_NA_1".to_string() },
                RemovePointTarget { ioa: 1, asdu_type: "MSpNa1".to_string() },
                RemovePointTarget { ioa: 999, asdu_type: "M_SP_NA_1".to_string() },
            ],
        ).await.unwrap();

        assert_eq!(snapshots.len(), 1, "duplicate keys are collapsed and unknown points skipped");
        let snapshot = &snapshots[0];
        assert_eq!(snapshot.ioa, 1);
        assert_eq!(snapshot.asdu_type, "M_SP_NA_1");
        assert_eq!(snapshot.value, "ON");
        assert!(snapshot.quality_iv && snapshot.quality_sb);
        assert!(!snapshot.quality_ov && !snapshot.quality_bl && !snapshot.quality_nt);
        assert!(snapshot.timestamp.is_some());
    }

    // issue #28 复现:运行中直接删除服务器后,同端口必须能立即重建。
    // 旧实现只 remove 不 stop(),accept 任务被分离,监听 socket 泄漏。
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn delete_running_server_releases_listen_port() {
        let port = free_port();
        let mut server = SlaveServer::new(test_transport(port));
        server.start().await.expect("start");
        let state = state_with_server(server, "server_1").await;

        delete_server_impl(&state, "server_1").await.expect("delete");

        std::net::TcpListener::bind(("127.0.0.1", port))
            .expect("port must be released after deleting a running server");
        assert!(state.servers.read().await.is_empty());
        assert!(delete_server_impl(&state, "server_1").await.is_err());
    }

    // issue #28:编辑点位改址(new_ioa)——保值改键、重复拒绝、遥控映射跟随。
    #[tokio::test]
    async fn update_definition_rekeys_ioa_and_follows_mappings() {
        let server = SlaveServer::new(test_transport(free_port()));
        let mut station = Station::new(1, "st");
        station.add_point(ctl_def(2, AsduTypeId::MDpNa1)).unwrap();
        station.add_point(ctl_def(3, AsduTypeId::MDpNa1)).unwrap();
        let mut ctrl = ctl_def(2, AsduTypeId::CDcNa1);
        ctrl.mapping = Some(ControlTarget {
            common_address: 1,
            ioa: 2,
            asdu_type: AsduTypeId::MDpNa1,
        });
        station.add_point(ctrl).unwrap();
        if let Some(p) = station.data_points.get_mut(2, AsduTypeId::MDpNa1) {
            p.value = DataPointValue::DoublePoint { value: 2 };
        }
        server.add_station(station).await.unwrap();
        let state = state_with_server(server, "s1").await;

        let request = |new_ioa: Option<u32>| UpdateDataPointDefinitionRequest {
            server_id: "s1".to_string(),
            common_address: 1,
            ioa: 2,
            new_ioa,
            new_asdu_type: None,
            asdu_type: "MDpNa1".to_string(),
            name: None,
            comment: None,
            mapping: None,
            command_qualifier: None,
            select_before_operate: None,
        };

        // 与同类型已有 IOA 冲突 → 整体拒绝
        let err = update_data_point_definition_impl(&state, request(Some(3)))
            .await
            .unwrap_err();
        assert!(err.contains("already exists"), "unexpected error: {err}");

        // 正常改址 2 → 200
        update_data_point_definition_impl(&state, request(Some(200)))
            .await
            .unwrap();

        let servers = state.servers.read().await;
        let stations = servers.get("s1").unwrap().server.stations.read().await;
        let st = stations.get(&1).unwrap();
        assert!(st.object_defs.iter().any(|d| d.ioa == 200 && d.asdu_type == AsduTypeId::MDpNa1));
        assert!(!st.data_points.contains(2, AsduTypeId::MDpNa1));
        let p = st.data_points.get(200, AsduTypeId::MDpNa1).unwrap();
        assert!(matches!(p.value, DataPointValue::DoublePoint { value: 2 }), "改址须保值");
        let ctrl = st
            .object_defs
            .iter()
            .find(|d| d.asdu_type == AsduTypeId::CDcNa1)
            .unwrap();
        assert_eq!(ctrl.mapping.unwrap().ioa, 200, "遥控映射须跟随改址");
    }

    // 改址须把旧 (ca, ioa, type) 键控的周期变位任务迁到新键，既不能
    // 留下会误改旧 IOA 的孤儿任务，也不能让正在运行的模拟静默丢失。
    #[tokio::test]
    async fn rename_migrates_point_mutation_to_new_key() {
        let server = SlaveServer::new(test_transport(free_port()));
        let mut station = Station::new(1, "st");
        station.add_point(ctl_def(5, AsduTypeId::MSpNa1)).unwrap();
        server.add_station(station).await.unwrap();
        server
            .start_point_mutation(
                1,
                5,
                AsduTypeId::MSpNa1,
                1000,
                MutationParams { mode: MutationMode::Flip, step: 1.0, min: 0.0, max: 1.0 },
            )
            .await;
        let state = state_with_server(server, "s1").await;

        update_data_point_definition_impl(&state, UpdateDataPointDefinitionRequest {
            server_id: "s1".to_string(),
            common_address: 1,
            ioa: 5,
            new_ioa: Some(50),
            new_asdu_type: None,
            asdu_type: "MSpNa1".to_string(),
            name: None,
            comment: None,
            mapping: None,
            command_qualifier: None,
            select_before_operate: None,
        })
        .await
        .unwrap();

        let servers = state.servers.read().await;
        let mutations = servers.get("s1").unwrap().server.list_point_mutations().await;
        assert_eq!(mutations.len(), 1, "改址后活动任务须保留: {mutations:?}");
        assert_eq!(mutations[0].1, 50, "活动任务须迁到新 IOA");
        assert_eq!(mutations[0].2, AsduTypeId::MSpNa1);
    }

    #[tokio::test]
    async fn update_definition_migrates_type_mapping_value_and_active_task() {
        let server = SlaveServer::new(test_transport(free_port()));
        let mut monitor_station = Station::new(1, "monitor");
        let mut monitor = ctl_def(5, AsduTypeId::MSpNa1);
        monitor.name = "breaker".to_string();
        monitor.comment = "bay 1".to_string();
        monitor_station.add_point(monitor).unwrap();
        {
            let point = monitor_station
                .data_points
                .get_mut(5, AsduTypeId::MSpNa1)
                .unwrap();
            point.value = DataPointValue::SinglePoint { value: true };
            point.quality = QualityFlags {
                iv: true,
                ..Default::default()
            };
        }
        server.add_station(monitor_station).await.unwrap();

        let mut control_station = Station::new(2, "control");
        let mut control = ctl_def(50, AsduTypeId::CScNa1);
        control.mapping = Some(ControlTarget {
            common_address: 1,
            ioa: 5,
            asdu_type: AsduTypeId::MSpNa1,
        });
        control_station.add_point(control).unwrap();
        server.add_station(control_station).await.unwrap();
        server
            .start_point_mutation(
                1,
                5,
                AsduTypeId::MSpNa1,
                750,
                MutationParams {
                    mode: MutationMode::Flip,
                    step: 1.0,
                    min: 0.0,
                    max: 1.0,
                },
            )
            .await;
        let state = state_with_server(server, "s1").await;

        update_data_point_definition_impl(
            &state,
            UpdateDataPointDefinitionRequest {
                server_id: "s1".to_string(),
                common_address: 1,
                ioa: 5,
                new_ioa: None,
                asdu_type: "MSpNa1".to_string(),
                new_asdu_type: Some("MSpTb1".to_string()),
                name: Some("breaker renamed".to_string()),
                comment: Some("migrated".to_string()),
                mapping: None,
                command_qualifier: None,
                select_before_operate: None,
            },
        )
        .await
        .unwrap();

        let servers = state.servers.read().await;
        let srv = &servers.get("s1").unwrap().server;
        {
            let stations = srv.stations.read().await;
            let monitor_station = stations.get(&1).unwrap();
            assert!(!monitor_station.data_points.contains(5, AsduTypeId::MSpNa1));
            let point = monitor_station
                .data_points
                .get(5, AsduTypeId::MSpTb1)
                .unwrap();
            assert_eq!(point.value, DataPointValue::SinglePoint { value: true });
            assert!(point.quality.iv);
            let def = monitor_station
                .object_defs
                .iter()
                .find(|d| d.ioa == 5 && d.asdu_type == AsduTypeId::MSpTb1)
                .unwrap();
            assert_eq!(def.name, "breaker renamed");
            assert_eq!(def.comment, "migrated");

            let control = stations
                .get(&2)
                .unwrap()
                .object_defs
                .iter()
                .find(|d| d.asdu_type == AsduTypeId::CScNa1)
                .unwrap();
            assert_eq!(control.mapping.unwrap().asdu_type, AsduTypeId::MSpTb1);
        }
        let tasks = srv.list_point_mutations_with_params().await;
        assert!(tasks.iter().any(|(ca, ioa, t, _, period)| {
            *ca == 1 && *ioa == 5 && *t == AsduTypeId::MSpTb1 && *period == 750
        }));
        assert!(!tasks.iter().any(|(_, _, t, _, _)| *t == AsduTypeId::MSpNa1));
    }

    #[tokio::test]
    async fn incompatible_single_type_migration_is_rejected_without_changes() {
        let server = SlaveServer::new(test_transport(free_port()));
        let mut station = Station::new(1, "st");
        station.add_point(ctl_def(5, AsduTypeId::MSpNa1)).unwrap();
        server.add_station(station).await.unwrap();
        let state = state_with_server(server, "s1").await;

        let error = update_data_point_definition_impl(
            &state,
            UpdateDataPointDefinitionRequest {
                server_id: "s1".to_string(),
                common_address: 1,
                ioa: 5,
                new_ioa: None,
                asdu_type: "MSpNa1".to_string(),
                new_asdu_type: Some("MMeNc1".to_string()),
                name: Some("must not change".to_string()),
                comment: None,
                mapping: None,
                command_qualifier: None,
                select_before_operate: None,
            },
        )
        .await
        .unwrap_err();
        assert!(error.contains("incompatible"), "unexpected error: {error}");

        let servers = state.servers.read().await;
        let stations = servers.get("s1").unwrap().server.stations.read().await;
        let station = stations.get(&1).unwrap();
        assert!(station.data_points.contains(5, AsduTypeId::MSpNa1));
        assert!(!station.data_points.contains(5, AsduTypeId::MMeNc1));
        assert_eq!(
            station
                .object_defs
                .iter()
                .find(|d| d.ioa == 5 && d.asdu_type == AsduTypeId::MSpNa1)
                .unwrap()
                .name,
            ""
        );
    }

    #[tokio::test]
    async fn batch_type_migration_rolls_back_on_collision() {
        let server = SlaveServer::new(test_transport(free_port()));
        let mut station = Station::new(1, "st");
        station.add_point(ctl_def(1, AsduTypeId::MSpNa1)).unwrap();
        station.add_point(ctl_def(2, AsduTypeId::MSpNa1)).unwrap();
        station.add_point(ctl_def(2, AsduTypeId::MSpTb1)).unwrap();
        server.add_station(station).await.unwrap();
        let state = state_with_server(server, "s1").await;

        let error = batch_migrate_data_point_types_impl(
            &state,
            BatchMigrateDataPointTypesRequest {
                server_id: "s1".to_string(),
                common_address: 1,
                points: vec![
                    RemovePointTarget {
                        ioa: 1,
                        asdu_type: "MSpNa1".to_string(),
                    },
                    RemovePointTarget {
                        ioa: 2,
                        asdu_type: "MSpNa1".to_string(),
                    },
                ],
                target_asdu_type: "MSpTb1".to_string(),
            },
        )
        .await
        .unwrap_err();
        assert!(
            error.contains("already exists"),
            "unexpected error: {error}"
        );
        let servers = state.servers.read().await;
        let stations = servers.get("s1").unwrap().server.stations.read().await;
        let station = stations.get(&1).unwrap();
        assert!(station.data_points.contains(1, AsduTypeId::MSpNa1));
        assert!(!station.data_points.contains(1, AsduTypeId::MSpTb1));
        assert!(station.data_points.contains(2, AsduTypeId::MSpNa1));
        assert!(station.data_points.contains(2, AsduTypeId::MSpTb1));
    }

    #[tokio::test]
    async fn batch_type_migration_updates_cross_ca_mapping_and_tasks() {
        let server = SlaveServer::new(test_transport(free_port()));
        let mut monitor_station = Station::new(1, "monitor");
        monitor_station
            .add_point(ctl_def(10, AsduTypeId::MMeNc1))
            .unwrap();
        monitor_station
            .add_point(ctl_def(11, AsduTypeId::MMeTc1))
            .unwrap();
        server.add_station(monitor_station).await.unwrap();
        let mut control_station = Station::new(2, "control");
        let mut control = ctl_def(50, AsduTypeId::CSeNc1);
        control.mapping = Some(ControlTarget {
            common_address: 1,
            ioa: 10,
            asdu_type: AsduTypeId::MMeNc1,
        });
        control_station.add_point(control).unwrap();
        server.add_station(control_station).await.unwrap();
        server
            .start_point_mutation(
                1,
                10,
                AsduTypeId::MMeNc1,
                900,
                MutationParams {
                    mode: MutationMode::Increment,
                    step: 1.5,
                    min: -5.0,
                    max: 5.0,
                },
            )
            .await;
        let state = state_with_server(server, "s1").await;

        let changed = batch_migrate_data_point_types_impl(
            &state,
            BatchMigrateDataPointTypesRequest {
                server_id: "s1".to_string(),
                common_address: 1,
                points: vec![
                    RemovePointTarget {
                        ioa: 10,
                        asdu_type: "MMeNc1".to_string(),
                    },
                    RemovePointTarget {
                        ioa: 11,
                        asdu_type: "MMeTc1".to_string(),
                    },
                ],
                target_asdu_type: "MMeTf1".to_string(),
            },
        )
        .await
        .unwrap();
        assert_eq!(changed, 2);

        let servers = state.servers.read().await;
        let srv = &servers.get("s1").unwrap().server;
        {
            let stations = srv.stations.read().await;
            let station = stations.get(&1).unwrap();
            for ioa in [10, 11] {
                assert!(station.data_points.contains(ioa, AsduTypeId::MMeTf1));
            }
            let control = stations
                .get(&2)
                .unwrap()
                .object_defs
                .iter()
                .find(|d| d.asdu_type == AsduTypeId::CSeNc1)
                .unwrap();
            assert_eq!(control.mapping.unwrap().asdu_type, AsduTypeId::MMeTf1);
        }
        let tasks = srv.list_point_mutations_with_params().await;
        assert!(tasks.iter().any(|(ca, ioa, t, params, period)| {
            *ca == 1
                && *ioa == 10
                && *t == AsduTypeId::MMeTf1
                && *period == 900
                && params.mode == MutationMode::Increment
                && params.step == 1.5
        }));
    }

    // issue #28:批量修改遥控 QU/SE——非控制点逐点跳过,更新须对增量轮询可见。
    #[tokio::test]
    async fn batch_update_control_options_applies_and_marks_changed() {
        let server = SlaveServer::new(test_transport(free_port()));
        let mut station = Station::new(1, "st");
        station.add_point(ctl_def(10, AsduTypeId::CScNa1)).unwrap();
        station.add_point(ctl_def(11, AsduTypeId::CScNa1)).unwrap();
        station.add_point(ctl_def(12, AsduTypeId::MSpNa1)).unwrap();
        let seq_before = station.data_points.current_seq();
        server.add_station(station).await.unwrap();
        let state = state_with_server(server, "s1").await;

        // 混入非控制点:该点被跳过,控制点正常写入(逐点跳过语义)。
        let n = batch_update_control_options_impl(&state, BatchControlOptionsRequest {
            server_id: "s1".to_string(),
            common_address: 1,
            points: vec![
                RemovePointTarget { ioa: 10, asdu_type: "CScNa1".to_string() },
                RemovePointTarget { ioa: 12, asdu_type: "MSpNa1".to_string() },
            ],
            command_qualifier: Some(1),
            set_qualifier: true,
            select_before_operate: None,
            set_select_before_operate: false,
        })
        .await
        .unwrap();
        assert_eq!(n, 1, "非控制点跳过,只更新 CScNa1");

        let n = batch_update_control_options_impl(&state, BatchControlOptionsRequest {
            server_id: "s1".to_string(),
            common_address: 1,
            points: vec![
                RemovePointTarget { ioa: 10, asdu_type: "CScNa1".to_string() },
                RemovePointTarget { ioa: 11, asdu_type: "CScNa1".to_string() },
            ],
            command_qualifier: Some(2),
            set_qualifier: true,
            select_before_operate: Some(true),
            set_select_before_operate: true,
        })
        .await
        .unwrap();
        assert_eq!(n, 2);

        let servers = state.servers.read().await;
        let stations = servers.get("s1").unwrap().server.stations.read().await;
        let st = stations.get(&1).unwrap();
        for ioa in [10u32, 11] {
            let d = st
                .object_defs
                .iter()
                .find(|d| d.ioa == ioa && d.asdu_type == AsduTypeId::CScNa1)
                .unwrap();
            assert_eq!(d.command_qualifier, Some(2));
            assert_eq!(d.select_before_operate, Some(true));
        }
        // 被跳过的非控制点不受影响
        let sp = st.object_defs.iter().find(|d| d.ioa == 12).unwrap();
        assert_eq!(sp.command_qualifier, None);
        assert_eq!(sp.select_before_operate, None);
        // def 修改须对增量轮询可见(mark_changed),否则前端缓存静默保持旧值
        let changed = st.data_points.changed_since(seq_before);
        assert!(
            changed.iter().any(|p| p.ioa == 10) && changed.iter().any(|p| p.ioa == 11),
            "批量修改后两点须出现在 changed_since 里: {changed:?}"
        );
    }

    #[test]
    fn bind_address_suggestions_include_wildcard_and_loopback() {
        let list = list_bind_address_suggestions();
        assert_eq!(list[0], "0.0.0.0", "0.0.0.0 是默认建议");
        assert!(list.contains(&"127.0.0.1".to_string()));
        // 探测到的出口 IP(如有)不与前两项重复。
        let unique: std::collections::HashSet<_> = list.iter().collect();
        assert_eq!(unique.len(), list.len());
    }

    #[test]
    fn parse_asdu_type_accepts_cp24_variants() {
        assert_eq!(parse_asdu_type("MSpTa1").unwrap(), AsduTypeId::MSpTa1);
        assert_eq!(parse_asdu_type("M_SP_TA_1").unwrap(), AsduTypeId::MSpTa1);
        assert_eq!(parse_asdu_type("m_me_tc_1").unwrap(), AsduTypeId::MMeTc1);
        assert_eq!(parse_asdu_type("MMeTb1").unwrap(), AsduTypeId::MMeTb1);
    }

    // 任务 2.4: DTO 映射携带全部 5 个品质位(本 crate 的实质改动)。
    // set_data_point_quality / update_data_point 命令体仅 get_mut 后写单字段,
    // 数据层行为已由 core 单测覆盖,此处聚焦 DTO 透传正确性。
    #[test]
    fn data_point_to_info_maps_all_quality_bits() {
        let mut p = DataPoint::new(100, AsduTypeId::MMeNc1);
        p.quality = QualityFlags { nt: true, sb: true, ..Default::default() };
        let def_map: HashMap<(u32, AsduTypeId), &InformationObjectDef> = HashMap::new();
        let info = data_point_to_info(&p, &def_map);
        assert!(info.quality_nt, "nt 透传");
        assert!(info.quality_sb, "sb 透传");
        assert!(!info.quality_iv && !info.quality_ov && !info.quality_bl, "未置位为 false");
    }

    #[test]
    fn data_point_to_info_good_all_false() {
        let p = DataPoint::new(100, AsduTypeId::MSpNa1);
        let def_map: HashMap<(u32, AsduTypeId), &InformationObjectDef> = HashMap::new();
        let info = data_point_to_info(&p, &def_map);
        assert!(!info.quality_iv && !info.quality_nt && !info.quality_sb && !info.quality_bl && !info.quality_ov);
    }

    #[test]
    fn control_point_options_validate_by_wire_type() {
        assert!(validate_control_point_options(AsduTypeId::CScNa1, Some(31), Some(false)).is_ok());
        assert!(validate_control_point_options(AsduTypeId::CSeNc1, Some(127), Some(true)).is_ok());
        assert!(validate_control_point_options(AsduTypeId::CScNa1, Some(32), None).is_err());
        assert!(validate_control_point_options(AsduTypeId::CSeNc1, Some(128), None).is_err());
        assert!(validate_control_point_options(AsduTypeId::CBoNa1, Some(1), None).is_err());
        assert!(validate_control_point_options(AsduTypeId::MSpNa1, None, Some(true)).is_err());
    }

    // ---- parse_value_for / 批量 helper(任务 2.3 / 3.3)----

    fn sp(ioa: u32, v: bool) -> DataPoint {
        DataPoint::with_value(ioa, AsduTypeId::MSpNa1, DataPointValue::SinglePoint { value: v })
    }
    fn me(ioa: u32, v: f32) -> DataPoint {
        DataPoint::with_value(ioa, AsduTypeId::MMeNc1, DataPointValue::ShortFloat { value: v })
    }

    #[test]
    fn parse_value_for_handles_types_and_errors() {
        assert!(matches!(parse_value_for(&sp(1, false), "ON"), Ok(DataPointValue::SinglePoint { value: true })));
        assert!(matches!(parse_value_for(&sp(1, true), "0"), Ok(DataPointValue::SinglePoint { value: false })));
        assert!(parse_value_for(&sp(1, false), "abc").is_err());
        assert!(matches!(parse_value_for(&me(1, 0.0), "1.5"), Ok(DataPointValue::ShortFloat { .. })));
        assert!(parse_value_for(&me(1, 0.0), "x").is_err());
    }

    fn norm(ioa: u32, v: f32) -> DataPoint {
        DataPoint::with_value(ioa, AsduTypeId::MMeNa1, DataPointValue::Normalized { value: v })
    }

    #[test]
    fn normalized_displays_and_parses_as_raw_nva_integer() {
        let def_map: HashMap<(u32, AsduTypeId), &InformationObjectDef> = HashMap::new();
        // 显示:内部 [-1,1) f32 → 原始 NVA 整数,与主站/线上一致。
        for nva in [-32767i16, -16384, -1, 0, 1, 16384, 32766, 32767] {
            let p = norm(1, nva as f32 / 32767.0);
            assert_eq!(data_point_to_info(&p, &def_map).value, nva.to_string(), "display nva={}", nva);
        }
        // 输入:用户输原始整数 → 内部 f32;再显示应原样还原(往返无损)。
        for nva in [-32767i16, -1, 0, 1, 16384, 32767] {
            let parsed = parse_value_for(&norm(1, 0.0), &nva.to_string()).unwrap();
            let p = DataPoint::with_value(1, AsduTypeId::MMeNa1, parsed);
            assert_eq!(data_point_to_info(&p, &def_map).value, nva.to_string(), "roundtrip nva={}", nva);
        }
        // 小数输入应被拒(已改为整数语义)。
        assert!(parse_value_for(&norm(1, 0.0), "0.5").is_err());
    }

    #[test]
    fn batch_quality_sets_all_and_filters_ov_to_measured() {
        let mut st = iec104sim_core::slave::Station::new(1, "t");
        st.data_points.insert(sp(100, false));
        st.data_points.insert(me(200, 0.0));
        let targets = [(100, AsduTypeId::MSpNa1), (200, AsduTypeId::MMeNc1)];

        // nt=true 落到混类型两点
        let changed = apply_batch_quality(&mut st, &targets, false, false, false, true, false);
        assert_eq!(changed.len(), 2);
        assert!(st.data_points.get(100, AsduTypeId::MSpNa1).unwrap().quality.nt);
        assert!(st.data_points.get(200, AsduTypeId::MMeNc1).unwrap().quality.nt);

        // ov=true 仅落测量类
        apply_batch_quality(&mut st, &targets, true, false, false, false, false);
        assert!(!st.data_points.get(100, AsduTypeId::MSpNa1).unwrap().quality.ov, "SP 忽略 OV");
        assert!(st.data_points.get(200, AsduTypeId::MMeNc1).unwrap().quality.ov, "ME 写入 OV");
    }

    #[test]
    fn batch_value_same_category_writes_all() {
        let mut st = iec104sim_core::slave::Station::new(1, "t");
        for ioa in 100..103u32 { st.data_points.insert(sp(ioa, false)); }
        let targets = [(100, AsduTypeId::MSpNa1), (101, AsduTypeId::MSpNa1), (102, AsduTypeId::MSpNa1)];
        let changed = apply_batch_value(&mut st, &targets, "ON").unwrap();
        assert_eq!(changed.len(), 3);
        for ioa in 100..103u32 {
            assert!(matches!(st.data_points.get(ioa, AsduTypeId::MSpNa1).unwrap().value, DataPointValue::SinglePoint { value: true }));
        }
    }

    #[test]
    fn batch_value_cross_category_rejected_no_side_effect() {
        let mut st = iec104sim_core::slave::Station::new(1, "t");
        st.data_points.insert(sp(100, false));
        st.data_points.insert(me(200, 0.0));
        // "1" 对 SP 可解析,但因含 ME(跨类)整体应被拒,且 SP 不被改动
        let res = apply_batch_value(&mut st, &[(100, AsduTypeId::MSpNa1), (200, AsduTypeId::MMeNc1)], "1");
        assert!(res.is_err());
        assert!(matches!(st.data_points.get(100, AsduTypeId::MSpNa1).unwrap().value, DataPointValue::SinglePoint { value: false }), "SP 未被改动");
    }

    #[test]
    fn batch_value_parse_failure_rejected_no_side_effect() {
        let mut st = iec104sim_core::slave::Station::new(1, "t");
        st.data_points.insert(sp(103, false));
        let res = apply_batch_value(&mut st, &[(103, AsduTypeId::MSpNa1)], "abc");
        assert!(res.is_err());
        assert!(matches!(st.data_points.get(103, AsduTypeId::MSpNa1).unwrap().value, DataPointValue::SinglePoint { value: false }), "解析失败不改动");
    }

    // ---- update_server_transport 的端口/运行态守卫(纯函数) ----

    #[test]
    fn validate_transport_ok_when_stopped_and_valid_port() {
        assert!(validate_transport_change(ServerState::Stopped, 2404).is_ok());
    }

    #[test]
    fn validate_transport_rejects_zero_port() {
        assert!(validate_transport_change(ServerState::Stopped, 0).is_err());
    }

    #[test]
    fn validate_transport_rejects_running_server() {
        // 运行中端口被监听占用,必须先停止
        assert!(validate_transport_change(ServerState::Running, 2404).is_err());
    }
}
