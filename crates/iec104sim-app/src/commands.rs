use crate::state::{AppState, DataPointInfo, IncrementalDataResponse, ServerInfo, SlaveServerState, StationInfo};
use iec104sim_core::data_point::{DataPoint, DataPointValue, InformationObjectDef};
use iec104sim_core::log_collector::LogCollector;
use iec104sim_core::log_entry::LogEntry;
use iec104sim_core::slave::{
    MutationMode, MutationParams, ProtocolTimingConfig, RemoteOperationConfig, ServerState,
    SlaveServer, SlaveTransportConfig, Station,
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
    if let Some(ops) = request.remote_ops {
        server.set_remote_ops(ops).await;
    }

    // Auto-create default station (CA=1) with pre-filled data points
    let n = request.count_per_category.unwrap_or(10);
    let default_station = match request.init_mode.as_deref() {
        Some("random") => Station::with_random_points(1, "", n),
        _ => Station::with_default_points(1, "", n),
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
) -> Result<(), String> {
    let state_str: String;
    {
        let mut servers = state.servers.write().await;
        let srv = servers
            .get_mut(&id)
            .ok_or_else(|| format!("server {} not found", id))?;

        srv.server
            .start()
            .await
            .map_err(|e| format!("failed to start: {}", e))?;
        state_str = format!("{:?}", srv.server.state());
    }

    app_handle.emit("server-state-changed", ServerStateEvent {
        id, state: state_str,
    }).map_err(|e| e.to_string())?;

    Ok(())
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

#[tauri::command]
pub async fn delete_server(
    state: State<'_, AppState>,
    id: String,
) -> Result<(), String> {
    let mut servers = state.servers.write().await;
    servers
        .remove(&id)
        .ok_or_else(|| format!("server {} not found", id))?;
    Ok(())
}

#[tauri::command]
pub async fn list_servers(
    state: State<'_, AppState>,
) -> Result<Vec<ServerInfo>, String> {
    let servers = state.servers.read().await;
    let mut result = Vec::new();

    for (id, srv_state) in servers.iter() {
        let station_count = srv_state.server.stations.read().await.len();
        result.push(ServerInfo {
            id: id.clone(),
            bind_address: srv_state.server.transport.bind_address.clone(),
            port: srv_state.server.transport.port,
            state: format!("{:?}", srv_state.server.state()),
            station_count,
            use_tls: srv_state.server.transport.tls.enabled,
        });
    }

    Ok(result)
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

fn parse_asdu_type(s: &str) -> Result<AsduTypeId, String> {
    // 归一化: 小写 + 仅保留字母数字。涵盖三种来源:
    // PascalCase 枚举名 ("MSpNa1") / 小写下划线 ("m_sp_na_1") /
    // 前端从 list_data_points 拿到的显示名 ("M_SP_NA_1").
    let key: String = s.chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(|c| c.to_lowercase())
        .collect();
    match key.as_str() {
        "mspna1" => Ok(AsduTypeId::MSpNa1),
        "msptb1" => Ok(AsduTypeId::MSpTb1),
        "mdpna1" => Ok(AsduTypeId::MDpNa1),
        "mdptb1" => Ok(AsduTypeId::MDpTb1),
        "mstna1" => Ok(AsduTypeId::MStNa1),
        "msttb1" => Ok(AsduTypeId::MStTb1),
        "mbona1" => Ok(AsduTypeId::MBoNa1),
        "mbotb1" => Ok(AsduTypeId::MBoTb1),
        "mmena1" => Ok(AsduTypeId::MMeNa1),
        "mmend1" => Ok(AsduTypeId::MMeNd1),
        "mmenb1" => Ok(AsduTypeId::MMeNb1),
        "mmenc1" => Ok(AsduTypeId::MMeNc1),
        "mmetd1" => Ok(AsduTypeId::MMeTd1),
        "mmete1" => Ok(AsduTypeId::MMeTe1),
        "mmetf1" => Ok(AsduTypeId::MMeTf1),
        "mitna1" => Ok(AsduTypeId::MItNa1),
        "mittb1" => Ok(AsduTypeId::MItTb1),
        _ => Err(format!("unknown ASDU type: {}", s)),
    }
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
    let def = InformationObjectDef {
        ioa: request.ioa,
        asdu_type,
        category: asdu_type.category(),
        name: request.name.unwrap_or_default(),
        comment: request.comment.unwrap_or_default(),
    };

    let mut stations = srv.server.stations.write().await;
    let station = stations
        .get_mut(&request.common_address)
        .ok_or_else(|| format!("station CA={} not found", request.common_address))?;

    station.add_point(def)
        .map_err(|e| format!("failed to add point: {}", e))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct BatchAddDataPointsRequest {
    pub server_id: String,
    pub common_address: u16,
    pub start_ioa: u32,
    pub count: u32,
    pub asdu_type: String,
    pub name_prefix: Option<String>,
}

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

    let mut stations = srv.server.stations.write().await;
    let station = stations
        .get_mut(&request.common_address)
        .ok_or_else(|| format!("station CA={} not found", request.common_address))?;

    station
        .batch_add_points(
            request.start_ioa,
            request.count,
            asdu_type,
            request.name_prefix.as_deref().unwrap_or(""),
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

/// Map a core `DataPoint` to the serialisable `DataPointInfo` the UI consumes.
fn data_point_to_info(
    p: &DataPoint,
    def_map: &std::collections::HashMap<(u32, AsduTypeId), &InformationObjectDef>,
) -> DataPointInfo {
    let def = def_map.get(&(p.ioa, p.asdu_type));
    DataPointInfo {
        ioa: p.ioa,
        asdu_type: p.asdu_type.name().to_string(),
        category: p.asdu_type.category().name().to_string(),
        name: def.map(|d| d.name.clone()).unwrap_or_default(),
        comment: def.map(|d| d.comment.clone()).unwrap_or_default(),
        value: match &p.value {
            DataPointValue::Normalized { value } => normalized_raw_string(*value),
            _ => p.value.display(),
        },
        quality_ov: p.quality.ov,
        quality_bl: p.quality.bl,
        quality_sb: p.quality.sb,
        quality_nt: p.quality.nt,
        quality_iv: p.quality.iv,
        // DataPoint.timestamp 内部存 UTC 便于无歧义比较；展示给用户时转为
        // 本地时区,这样 UI 看到的"时间戳"和系统挂钟一致。
        timestamp: p.timestamp.map(|t| t.with_timezone(&chrono::Local).format("%H:%M:%S%.3f").to_string()),
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
    let servers = state.servers.read().await;
    let srv = servers
        .get(&server_id)
        .ok_or_else(|| format!("server {} not found", server_id))?;
    Ok(srv.log_collector.export_csv().await)
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

/// 解析前端传入的变位模式字符串(serde snake_case:flip/increment/decrement)。
/// 缺省或无法识别时按 flip 处理,保持旧行为。
fn parse_mutation_mode(s: Option<&str>) -> MutationMode {
    match s {
        Some("increment") => MutationMode::Increment,
        Some("decrement") => MutationMode::Decrement,
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
/// mode 为 flip/increment/decrement,供前端在数据表显示当前变位方式。
#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct PointMutationInfo {
    pub ioa: u32,
    pub asdu_type: String,
    pub mode: String,
}

fn mutation_mode_str(mode: MutationMode) -> &'static str {
    match mode {
        MutationMode::Flip => "flip",
        MutationMode::Increment => "increment",
        MutationMode::Decrement => "decrement",
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
    let active = srv.server.list_point_mutations().await;
    Ok(active
        .into_iter()
        .filter(|(ca, _, _, _)| *ca == common_address)
        .map(|(_, ioa, t, mode)| PointMutationInfo {
            ioa,
            asdu_type: t.name().to_string(),
            mode: mutation_mode_str(mode).to_string(),
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
    use std::collections::HashMap;

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
