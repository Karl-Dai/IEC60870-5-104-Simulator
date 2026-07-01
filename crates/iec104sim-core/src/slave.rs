use crate::data_point::{DataPoint, DataPointMap, DataPointValue, InformationObjectDef};
use crate::log_collector::LogCollector;
use crate::log_entry::{Direction, FrameLabel, LogEntry};
use crate::types::{AsduTypeId, DataCategory, QualityFlags};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::net::{SocketAddr, TcpStream};
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener as AsyncTcpListener, TcpStream as AsyncTcpStream};
use tokio::sync::RwLock;

// ---------------------------------------------------------------------------
// TLS Configuration
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SlaveTlsConfig {
    pub enabled: bool,
    #[serde(default)]
    pub cert_file: String,
    #[serde(default)]
    pub key_file: String,
    #[serde(default)]
    pub ca_file: String,
    #[serde(default)]
    pub require_client_cert: bool,
    /// Optional PKCS#12 (.p12/.pfx) identity file. When set, cert_file and
    /// key_file are ignored for identity loading. Required on macOS when using
    /// ECDSA keys (native-tls / Security framework limitation).
    #[serde(default)]
    pub pkcs12_file: String,
    /// Password for the PKCS#12 file (may be empty string).
    #[serde(default)]
    pub pkcs12_password: String,
}

// ---------------------------------------------------------------------------
// Cyclic / Spontaneous Configuration
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct CyclicConfig {
    pub enabled: bool,
    pub interval_ms: u32,
}

impl Default for CyclicConfig {
    fn default() -> Self {
        Self { enabled: false, interval_ms: 2000 }
    }
}

// ---------------------------------------------------------------------------
// Remote Operation Configuration (远动运行参数配置)
// ---------------------------------------------------------------------------
//
// 服务器级参数,运行时取一份 RwLock 快照传递给各处理函数,避免与 stations /
// connections 锁交叉。

/// IEC 60870-5-104 协议时序参数 (t0/t1/t2/t3/k/w)。
/// 一期仅持久化与 UI 展示;运行时计时器尚未严格驱动 t1/t2/t3。
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct ProtocolTimingConfig {
    pub t0: u32,
    pub t1: u32,
    pub t2: u32,
    pub t3: u32,
    pub k: u16,
    pub w: u16,
}

impl Default for ProtocolTimingConfig {
    fn default() -> Self {
        Self { t0: 30, t1: 15, t2: 10, t3: 20, k: 12, w: 8 }
    }
}

impl ProtocolTimingConfig {
    /// Normalize the timing parameters in place so they satisfy the IEC 104
    /// relationship invariants (`t2 < t1 < t3`, `w ≤ ⌊2k/3⌋`). Returns the
    /// fields that were corrected (empty ⇒ already valid).
    pub fn normalize(&mut self) -> Vec<crate::timing::TimingCorrection> {
        let (out, changes) = crate::timing::correct_timing(crate::timing::TimingParams {
            t0: self.t0, t1: self.t1, t2: self.t2, t3: self.t3, k: self.k, w: self.w,
        });
        self.t0 = out.t0;
        self.t1 = out.t1;
        self.t2 = out.t2;
        self.t3 = out.t3;
        self.k = out.k;
        self.w = out.w;
        changes
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UploadMode {
    Continuous,
    Discrete,
}

impl Default for UploadMode {
    fn default() -> Self { Self::Discrete }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[repr(u8)]
pub enum CommandAckCot {
    ActivationCon = 7,
    DeactivationCon = 9,
    ActivationTermination = 10,
}

impl CommandAckCot {
    pub fn as_u8(self) -> u8 { self as u8 }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct RandomMutationPacing {
    pub batch_size: u32,
    pub delay_ms: u32,
}

impl Default for RandomMutationPacing {
    fn default() -> Self { Self { batch_size: 2000, delay_ms: 50 } }
}

/// 按分类的「变位同步上送 TB」开关。变位/周期上送时,开启的分类会在 NA 帧之后
/// 额外派生并上送对应的带时标 (TB) 帧。累计量 (IT) 靠召唤上送而非变位,不提供此开关。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct SyncTbByCategory {
    pub sp: bool,
    pub dp: bool,
    pub st: bool,
    pub bo: bool,
    pub me_na: bool,
    pub me_nb: bool,
    pub me_nc: bool,
}

impl SyncTbByCategory {
    /// 该分类是否开启变位同步派生 TB。IntegratedTotals 永不派生(无开关)。
    pub fn enabled_for(&self, category: DataCategory) -> bool {
        match category {
            DataCategory::SinglePoint => self.sp,
            DataCategory::DoublePoint => self.dp,
            DataCategory::StepPosition => self.st,
            DataCategory::Bitstring => self.bo,
            DataCategory::NormalizedMeasured => self.me_na,
            DataCategory::ScaledMeasured => self.me_nb,
            DataCategory::FloatMeasured => self.me_nc,
            DataCategory::IntegratedTotals => false,
            DataCategory::System => false,
        }
    }
}

/// R1(显式 TB 优先于派生 TB):给定一个 NA 点,是否应为其派生 TB 帧。
/// 仅当该类型有时标变体、自身不带时标、且该 IOA 尚无显式存储的 TB 点时为真——
/// 显式 TB 会作为独立点位自行上送,跳过派生可避免同一 IOA 重复上送 TB。
fn should_derive_tb(map: &DataPointMap, na_type: AsduTypeId, ioa: u32) -> bool {
    if na_type.is_timestamped() {
        return false;
    }
    match na_type.timestamped_variant() {
        Some(tb) => !map.contains(ioa, tb),
        None => false,
    }
}

/// 远动运行参数。`#[serde(default)]` 保证旧配置缺字段时取默认。
/// 旧版扁平字段 `sp_sync_with_tb: bool` 在反序列化时被静默忽略(本结构无
/// deny_unknown_fields),其语义已由 `sync_tb_by_category` 按分类取代。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct RemoteOperationConfig {
    pub sync_tb_by_category: SyncTbByCategory,
    pub answer_general_interrogation: bool,
    pub answer_counter_interrogation: bool,
    pub answer_commands: bool,
    pub gi_include_timestamped: bool,
    pub upload_mode_untimestamped: UploadMode,
    pub upload_mode_timestamped: UploadMode,
    pub select_ack_cot: CommandAckCot,
    pub execute_ack_cot: CommandAckCot,
    pub cancel_ack_cot: CommandAckCot,
    pub random_pacing: RandomMutationPacing,
    pub auto_packing: bool,
}

impl Default for RemoteOperationConfig {
    fn default() -> Self {
        Self {
            sync_tb_by_category: SyncTbByCategory::default(),
            answer_general_interrogation: true,
            answer_counter_interrogation: true,
            answer_commands: true,
            gi_include_timestamped: false,
            upload_mode_untimestamped: UploadMode::Discrete,
            upload_mode_timestamped: UploadMode::Discrete,
            select_ack_cot: CommandAckCot::ActivationCon,
            execute_ack_cot: CommandAckCot::ActivationTermination,
            cancel_ack_cot: CommandAckCot::DeactivationCon,
            random_pacing: RandomMutationPacing::default(),
            auto_packing: false,
        }
    }
}

// ---------------------------------------------------------------------------
// Stream Abstraction (for blocking TLS path)
// ---------------------------------------------------------------------------

#[allow(dead_code)]
enum SlaveStream {
    Plain(TcpStream),
    Tls(native_tls::TlsStream<TcpStream>),
}

impl std::io::Read for SlaveStream {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        match self {
            SlaveStream::Plain(s) => s.read(buf),
            SlaveStream::Tls(s) => s.read(buf),
        }
    }
}

impl std::io::Write for SlaveStream {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        match self {
            SlaveStream::Plain(s) => s.write(buf),
            SlaveStream::Tls(s) => s.write(buf),
        }
    }
    fn flush(&mut self) -> std::io::Result<()> {
        match self {
            SlaveStream::Plain(s) => s.flush(),
            SlaveStream::Tls(s) => s.flush(),
        }
    }
}

// ---------------------------------------------------------------------------
// Station
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Station {
    pub common_address: u16,
    pub name: String,
    pub data_points: DataPointMap,
    pub object_defs: Vec<InformationObjectDef>,
    #[serde(default)]
    pub cyclic_config: CyclicConfig,
}

impl Station {
    pub fn new(common_address: u16, name: impl Into<String>) -> Self {
        Self {
            common_address,
            name: name.into(),
            data_points: DataPointMap::new(),
            object_defs: Vec::new(),
            cyclic_config: CyclicConfig::default(),
        }
    }

    pub fn with_default_points(common_address: u16, name: impl Into<String>, count_per_category: u32) -> Self {
        let mut station = Self::new(common_address, name);
        // 8 个不带时标 (NA) 类型（每物理分类一个），全部共享同一段 IOA 1..=N。
        // 默认不预建带时标 (TB) 点——TB 是同一信号的传输格式,由「变位同步上送」
        // 开关现场派生(见 sync_tb_by_category / gi_include_timestamped)。
        // 用户仍可手动添加 TB 点(NA 优先、TB 可选存在)。
        let asdu_types: [(AsduTypeId, DataCategory); 8] = [
            (AsduTypeId::MSpNa1, DataCategory::SinglePoint),
            (AsduTypeId::MDpNa1, DataCategory::DoublePoint),
            (AsduTypeId::MStNa1, DataCategory::StepPosition),
            (AsduTypeId::MBoNa1, DataCategory::Bitstring),
            (AsduTypeId::MMeNa1, DataCategory::NormalizedMeasured),
            (AsduTypeId::MMeNb1, DataCategory::ScaledMeasured),
            (AsduTypeId::MMeNc1, DataCategory::FloatMeasured),
            (AsduTypeId::MItNa1, DataCategory::IntegratedTotals),
        ];
        for (asdu_type, category) in &asdu_types {
            for i in 0..count_per_category {
                let ioa = 1 + i;
                station.data_points.insert(DataPoint::new(ioa, *asdu_type));
                station.object_defs.push(InformationObjectDef {
                    ioa,
                    asdu_type: *asdu_type,
                    category: *category,
                    name: String::new(),
                    comment: String::new(),
                });
            }
        }
        station
    }

    pub fn with_random_points(common_address: u16, name: impl Into<String>, count_per_category: u32) -> Self {
        use rand::Rng;
        let mut station = Self::with_default_points(common_address, name, count_per_category);
        let mut rng = rand::thread_rng();
        for point in station.data_points.points.values_mut() {
            point.value = match point.asdu_type.category() {
                DataCategory::SinglePoint => DataPointValue::SinglePoint { value: rng.gen() },
                DataCategory::DoublePoint => DataPointValue::DoublePoint { value: rng.gen_range(1..=2) },
                DataCategory::NormalizedMeasured => DataPointValue::Normalized { value: rng.gen_range(-1.0..1.0) },
                DataCategory::ScaledMeasured => DataPointValue::Scaled { value: rng.gen_range(-1000..1000) },
                DataCategory::FloatMeasured => DataPointValue::ShortFloat { value: rng.gen_range(-100.0..100.0) },
                DataCategory::IntegratedTotals => DataPointValue::IntegratedTotal { value: rng.gen_range(0..10000), carry: false, sequence: 0 },
                _ => DataPointValue::default_for(point.asdu_type),
            };
        }
        station
    }

    pub fn add_point(&mut self, def: InformationObjectDef) -> Result<(), SlaveError> {
        if !self.data_points.contains(def.ioa, def.asdu_type) {
            self.data_points.insert(DataPoint::new(def.ioa, def.asdu_type));
        }
        // Update or add metadata
        if let Some(existing_def) = self.object_defs.iter_mut().find(|d| d.ioa == def.ioa && d.asdu_type == def.asdu_type) {
            *existing_def = def;
        } else {
            self.object_defs.push(def);
        }
        Ok(())
    }

    pub fn remove_point(&mut self, ioa: u32, asdu_type: AsduTypeId) -> Result<(), SlaveError> {
        if !self.data_points.contains(ioa, asdu_type) { return Err(SlaveError::IoaNotFound(ioa)); }
        self.data_points.remove(ioa, asdu_type);
        self.object_defs.retain(|d| !(d.ioa == ioa && d.asdu_type == asdu_type));
        Ok(())
    }

    /// Remove multiple points in one pass. Missing (ioa, type) pairs are
    /// skipped (idempotent) rather than aborting the batch. `object_defs` is
    /// pruned once via a HashSet lookup to avoid O(n*m) retain calls.
    /// Returns the number of points actually removed.
    pub fn remove_points(&mut self, targets: &[(u32, AsduTypeId)]) -> usize {
        use std::collections::HashSet;
        let set: HashSet<(u32, AsduTypeId)> = targets.iter().copied().collect();
        let before = self.data_points.len();
        for &(ioa, asdu_type) in &set {
            self.data_points.remove(ioa, asdu_type);
        }
        self.object_defs.retain(|d| !set.contains(&(d.ioa, d.asdu_type)));
        before - self.data_points.len()
    }

    /// Batch-add data points with consecutive IOAs starting from `start_ioa`.
    /// Optimized: avoids O(n) linear search in object_defs per point.
    pub fn batch_add_points(
        &mut self,
        start_ioa: u32,
        count: u32,
        asdu_type: AsduTypeId,
        name_prefix: &str,
    ) -> Result<u32, SlaveError> {
        use std::collections::HashSet;
        let category = asdu_type.category();
        // Pre-build set of existing (ioa, type) for O(1) lookup
        let existing: HashSet<(u32, AsduTypeId)> = self.object_defs.iter()
            .map(|d| (d.ioa, d.asdu_type))
            .collect();
        for i in 0..count {
            let ioa = start_ioa + i;
            if !self.data_points.contains(ioa, asdu_type) {
                self.data_points.insert(DataPoint::new(ioa, asdu_type));
            }
            let name = if name_prefix.is_empty() {
                String::new()
            } else {
                format!("{}_{}", name_prefix, ioa)
            };
            if !existing.contains(&(ioa, asdu_type)) {
                self.object_defs.push(InformationObjectDef {
                    ioa, asdu_type, category, name, comment: String::new(),
                });
            }
        }
        Ok(count)
    }
}

// ---------------------------------------------------------------------------
// Server State
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ServerState { Stopped, Running }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlaveTransportConfig {
    pub bind_address: String,
    pub port: u16,
    #[serde(default)]
    pub tls: SlaveTlsConfig,
}

impl Default for SlaveTransportConfig {
    fn default() -> Self {
        Self { bind_address: "0.0.0.0".to_string(), port: 2404, tls: SlaveTlsConfig::default() }
    }
}

// ---------------------------------------------------------------------------
// Connection State — shared between read task and cyclic task
// ---------------------------------------------------------------------------

/// Per-connection IEC 60870-5-104 sequence state.
/// `ssn` is N(S)<<1 (own send count), `rsn` is N(R)<<1 (next expected peer N(S)).
struct SeqState {
    /// 我方下一个发送 I 帧的 N(S)（以 << 1 形式存储，每帧 += 2）。
    ssn: u16,
    /// 我方下一个发送帧（I/S）的 N(R) = 期望对方下一个 N(S)（同样 << 1）。
    rsn: u16,
    /// 对方最近一次 N(R)（同样 << 1）。代表对方已确认收到 N(S) < ack_ssn 的所有 I 帧。
    /// in_flight = (ssn - ack_ssn) / 2 用于 k 窗口流控。
    ack_ssn: u16,
    /// 自上次发出 S 帧以来累计的对端 I 帧数（每收到 1 个 I 帧 += 2，与 N(S) 步长一致）。
    /// 达到 2*w 时主动回 S 帧确认。
    unacked_recv: u16,
    /// 最后一次发 S 帧确认对端的时间，用于 t2 兜底（无数据可发时主动确认）。
    last_s_ack_at: tokio::time::Instant,
}

impl Default for SeqState {
    fn default() -> Self {
        Self {
            ssn: 0,
            rsn: 0,
            ack_ssn: 0,
            unacked_recv: 0,
            last_s_ack_at: tokio::time::Instant::now(),
        }
    }
}

type SharedSeq = Arc<tokio::sync::Mutex<SeqState>>;

/// Per-connection write queue. The async write task drains this queue.
struct ConnectionWrite {
    /// Mutex-protected byte queue. Write task drains this.
    queue: Arc<tokio::sync::Mutex<Vec<u8>>>,
    /// Shared sequence state used by all senders (read loop, cyclic, spontaneous).
    seq: SharedSeq,
    /// IEC 60870-5-104 data-transfer state. Cyclic and spontaneous I-frames may
    /// only be sent while this is `true` — i.e. after the master has issued
    /// STARTDT and before STOPDT. The read loop flips it; the cyclic task and
    /// `queue_spontaneous` honour it. Sending I-frames before STARTDT desyncs
    /// the master's receive sequence counter permanently.
    started: Arc<std::sync::atomic::AtomicBool>,
    /// Last sent value string per IOA.
    last_sent: HashMap<u32, String>,
    /// Logger.
    #[allow(dead_code)]
    log_collector: Option<Arc<LogCollector>>,
}

type SharedConnections = Arc<RwLock<HashMap<SocketAddr, ConnectionWrite>>>;

/// Update local N(R) from a just-received I-frame so that subsequent outgoing
/// frames acknowledge the master's send sequence. Also picks up the peer's
/// N(R) to advance our ack_ssn (sender-side k window) and increments
/// `unacked_recv` so the read loop can decide when to emit an S-frame.
fn observe_recv_iframe(seq: &mut SeqState, frame: &[u8]) {
    if frame.len() < 6 { return; }
    let peer_ns_shifted = u16::from_le_bytes([frame[2], frame[3]]);
    let peer_nr_shifted = u16::from_le_bytes([frame[4], frame[5]]);
    seq.rsn = peer_ns_shifted.wrapping_add(2);
    seq.ack_ssn = peer_nr_shifted;
    seq.unacked_recv = seq.unacked_recv.wrapping_add(2);
}

/// Pick up an S-frame: it only advances our sender-side ack_ssn (no I-frame
/// counter increment). Caller must verify ctrl1 & 0x03 == 0x01 before calling.
fn observe_recv_sframe(seq: &mut SeqState, frame: &[u8]) {
    if frame.len() < 6 { return; }
    let peer_nr_shifted = u16::from_le_bytes([frame[4], frame[5]]);
    seq.ack_ssn = peer_nr_shifted;
}

/// Echo a received I-frame back with our own APCI control bytes and an
/// overridden COT. Increments our N(S).
fn build_response_frame(recv: &[u8], cot: u8, seq: &mut SeqState) -> Vec<u8> {
    let mut out = recv.to_vec();
    if out.len() >= 6 {
        out[2] = (seq.ssn & 0xFF) as u8;
        out[3] = ((seq.ssn >> 8) & 0xFF) as u8;
        out[4] = (seq.rsn & 0xFF) as u8;
        out[5] = ((seq.rsn >> 8) & 0xFF) as u8;
    }
    if out.len() > 8 { out[8] = cot; }
    seq.ssn = seq.ssn.wrapping_add(2);
    out
}


// ---------------------------------------------------------------------------
// SlaveServer
// ---------------------------------------------------------------------------

pub type SharedStations = Arc<RwLock<HashMap<u16, Station>>>;
pub type SharedRemoteOps = Arc<RwLock<RemoteOperationConfig>>;
pub type SharedProtocolTiming = Arc<RwLock<ProtocolTimingConfig>>;

pub struct SlaveServer {
    pub transport: SlaveTransportConfig,
    pub stations: SharedStations,
    pub log_collector: Option<Arc<LogCollector>>,
    /// 远动运行参数 (服务器级)。RwLock 便于 spawn 的任务克隆引用。
    pub remote_ops: SharedRemoteOps,
    /// 协议时序参数 (服务器级)。一期仅持久化,运行时尚未严格驱动 t1/t2/t3。
    pub protocol_timing: SharedProtocolTiming,
    state: ServerState,
    shutdown_flag: Arc<std::sync::atomic::AtomicBool>,
    /// 与 `shutdown_flag` 搭配:stop() 置位标志后立即唤醒周期任务,
    /// 使其无需等到下一个 `interval.tick()`(默认 2000ms)才退出。
    shutdown_notify: Arc<tokio::sync::Notify>,
    server_handle: Option<tokio::task::JoinHandle<()>>,
    cyclic_handle: Option<tokio::task::JoinHandle<()>>,
    /// 按 (ca, ioa, asdu_type) 维护的周期变位任务句柄。每个点位独立启停;
    /// `start_point_mutation` 对同一 key 重复调用会先 abort 旧任务。
    point_mutation_handles:
        tokio::sync::Mutex<HashMap<(u16, u32, AsduTypeId), (tokio::task::JoinHandle<()>, MutationMode)>>,
    connections: SharedConnections,
}

impl SlaveServer {
    pub fn new(transport: SlaveTransportConfig) -> Self {
        Self {
            transport,
            stations: Arc::new(RwLock::new(HashMap::new())),
            log_collector: None,
            remote_ops: Arc::new(RwLock::new(RemoteOperationConfig::default())),
            protocol_timing: Arc::new(RwLock::new(ProtocolTimingConfig::default())),
            state: ServerState::Stopped,
            shutdown_flag: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            shutdown_notify: Arc::new(tokio::sync::Notify::new()),
            server_handle: None,
            cyclic_handle: None,
            point_mutation_handles: tokio::sync::Mutex::new(HashMap::new()),
            connections: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn set_remote_ops(&self, new_ops: RemoteOperationConfig) -> RemoteOperationConfig {
        let mut guard = self.remote_ops.write().await;
        std::mem::replace(&mut *guard, new_ops)
    }

    pub async fn get_remote_ops(&self) -> RemoteOperationConfig {
        self.remote_ops.read().await.clone()
    }

    pub async fn set_protocol_timing(&self, new_timing: ProtocolTimingConfig) -> ProtocolTimingConfig {
        let mut guard = self.protocol_timing.write().await;
        std::mem::replace(&mut *guard, new_timing)
    }

    pub async fn get_protocol_timing(&self) -> ProtocolTimingConfig {
        *self.protocol_timing.read().await
    }

    pub fn with_log_collector(mut self, collector: Arc<LogCollector>) -> Self {
        self.log_collector = Some(collector);
        self
    }

    pub fn state(&self) -> ServerState { self.state }

    pub async fn add_station(&self, station: Station) -> Result<(), SlaveError> {
        let mut stations = self.stations.write().await;
        if stations.contains_key(&station.common_address) {
            return Err(SlaveError::DuplicateStation(station.common_address));
        }
        stations.insert(station.common_address, station);
        Ok(())
    }

    pub async fn remove_station(&self, ca: u16) -> Result<Station, SlaveError> {
        let mut stations = self.stations.write().await;
        stations.remove(&ca).ok_or(SlaveError::StationNotFound(ca))
    }

    pub async fn set_cyclic_config(&self, common_address: u16, config: CyclicConfig) -> Result<(), SlaveError> {
        let mut stations = self.stations.write().await;
        let station = stations.get_mut(&common_address).ok_or(SlaveError::StationNotFound(common_address))?;
        station.cyclic_config = config;
        Ok(())
    }

    /// Queue spontaneous I-frames (COT=3) for the given (IOA, type) pairs to all connected clients.
    pub async fn queue_spontaneous(&self, common_address: u16, changed: &[(u32, AsduTypeId)]) {
        do_queue_spontaneous(
            &self.stations,
            &self.connections,
            &self.remote_ops,
            &self.log_collector,
            common_address,
            changed,
        ).await;
    }

    /// 启动单个点位的周期变位。同 (ca, ioa, asdu_type) 已有任务则先 abort 再起新的。
    /// period_ms 下限 50ms。任务按 `params` 周期性变位(翻转 / 递增 / 递减)该点
    /// 并上送 spontaneous。递增/递减的三角波方向是任务局部状态。
    pub async fn start_point_mutation(
        &self,
        ca: u16,
        ioa: u32,
        asdu_type: AsduTypeId,
        period_ms: u32,
        params: MutationParams,
    ) {
        let key = (ca, ioa, asdu_type);
        let mut guard = self.point_mutation_handles.lock().await;
        if let Some((h, _)) = guard.remove(&key) { h.abort(); }

        let stations = self.stations.clone();
        let connections = self.connections.clone();
        let remote_ops = self.remote_ops.clone();
        let log_collector = self.log_collector.clone();
        let shutdown_flag = self.shutdown_flag.clone();
        let handle = tokio::spawn(async move {
            let period = std::time::Duration::from_millis(period_ms.max(50) as u64);
            let mut interval = tokio::time::interval(period);
            interval.tick().await; // 跳过 immediate first tick
            let mut dir = params.mode.initial_dir();
            loop {
                interval.tick().await;
                if shutdown_flag.load(std::sync::atomic::Ordering::SeqCst) { break; }
                let mutated = {
                    let mut st_guard = stations.write().await;
                    if let Some(station) = st_guard.get_mut(&ca) {
                        if let Some(p) = station.data_points.get_mut(ioa, asdu_type) {
                            let (next, next_dir) = apply_mutation(&p.value, &params, dir);
                            p.value = next;
                            dir = next_dir;
                            p.timestamp = Some(chrono::Utc::now());
                            // p 的可变借用到此结束(NLL),mark_changed 可重新借 data_points。
                            station.data_points.mark_changed(ioa, asdu_type);
                            true
                        } else {
                            false
                        }
                    } else {
                        false
                    }
                };
                if mutated {
                    do_queue_spontaneous(
                        &stations, &connections, &remote_ops, &log_collector,
                        ca, &[(ioa, asdu_type)],
                    ).await;
                }
            }
        });
        guard.insert(key, (handle, params.mode));
    }

    /// 停止单个点位的周期变位。
    pub async fn stop_point_mutation(&self, ca: u16, ioa: u32, asdu_type: AsduTypeId) {
        let mut guard = self.point_mutation_handles.lock().await;
        if let Some((h, _)) = guard.remove(&(ca, ioa, asdu_type)) { h.abort(); }
    }

    /// 返回当前活跃的周期变位点位 (ca, ioa, asdu_type, mode)。
    pub async fn list_point_mutations(&self) -> Vec<(u16, u32, AsduTypeId, MutationMode)> {
        self.point_mutation_handles
            .lock()
            .await
            .iter()
            .map(|(&(ca, ioa, t), &(_, mode))| (ca, ioa, t, mode))
            .collect()
    }

    pub async fn start(&mut self) -> Result<(), SlaveError> {
        if self.state == ServerState::Running { return Err(SlaveError::AlreadyRunning); }

        let addr_str = format!("{}:{}", self.transport.bind_address, self.transport.port);
        let listener = AsyncTcpListener::bind(&addr_str)
            .await
            .map_err(|e| SlaveError::BindError(format!("Failed to bind {}: {}", addr_str, e)))?;

        let tls_acceptor: Option<Arc<native_tls::TlsAcceptor>> = if self.transport.tls.enabled {
            let cfg = &self.transport.tls;
            let identity = if !cfg.pkcs12_file.is_empty() {
                // 剥掉 Windows「复制为路径」带来的包裹引号/空白(否则 os error 123)。
                let p12_path = crate::tls_key::sanitize_fs_path(&cfg.pkcs12_file);
                let p12 = std::fs::read(p12_path)
                    .map_err(|e| SlaveError::TlsError(format!("读取 PKCS12 {}: {}", p12_path, e)))?;
                native_tls::Identity::from_pkcs12(&p12, &cfg.pkcs12_password)
                    .map_err(|e| SlaveError::TlsError(format!("加载 PKCS12 身份: {}", e)))?
            } else {
                let cert_path = crate::tls_key::sanitize_fs_path(&cfg.cert_file);
                let cert = std::fs::read(cert_path)
                    .map_err(|e| SlaveError::TlsError(format!("读取证书 {}: {}", cert_path, e)))?;
                // PKCS#1 → PKCS#8 自动转换,详见 master.rs 同段注释。
                let key = crate::tls_key::load_key_as_pkcs8_pem(&cfg.key_file)
                    .map_err(SlaveError::TlsError)?;
                native_tls::Identity::from_pkcs8(&cert, &key)
                    .map_err(|e| SlaveError::TlsError(format!("加载身份: {}", e)))?
            };
            let mut builder = native_tls::TlsAcceptor::builder(identity);
            builder.min_protocol_version(Some(native_tls::Protocol::Tlsv12));
            Some(Arc::new(builder.build().map_err(|e| SlaveError::TlsError(format!("创建接受器: {}", e)))?))
        } else { None };

        let shutdown_flag = self.shutdown_flag.clone();
        shutdown_flag.store(false, std::sync::atomic::Ordering::SeqCst);
        // 每次启动重建停止通知句柄,避免上一轮遗留的 permit 让新任务误退出。
        self.shutdown_notify = Arc::new(tokio::sync::Notify::new());
        let stations = self.stations.clone();
        let log_collector = self.log_collector.clone();
        let is_tls = self.transport.tls.enabled;

        // Shared connections map.
        self.connections = Arc::new(RwLock::new(HashMap::new()));
        let connections = self.connections.clone();
        let cyclic_connections = connections.clone();
        let remote_ops = self.remote_ops.clone();
        let protocol_timing = self.protocol_timing.clone();

        // Start cyclic background task.
        let cyclic_stations = self.stations.clone();
        let cyclic_flag = self.shutdown_flag.clone();
        let cyclic_shutdown = self.shutdown_notify.clone();
        let cyclic_log = self.log_collector.clone();
        let cyclic_remote_ops = self.remote_ops.clone();
        let cyclic_handle = tokio::spawn(async move {
            // Use interval_ms from the first enabled station, default to 2000ms
            let get_interval_ms = || async {
                let stations = cyclic_stations.read().await;
                stations.values()
                    .find(|s| s.cyclic_config.enabled)
                    .map(|s| s.cyclic_config.interval_ms)
                    .unwrap_or(2000)
            };
            let mut interval_ms = get_interval_ms().await;
            let mut interval = tokio::time::interval(std::time::Duration::from_millis(interval_ms as u64));
            loop {
                // 竞速:正常按 interval 周期唤醒;stop() 触发时立即经 notify 唤醒退出,
                // 不必干等到下一个 tick(否则 stop 会阻塞最长一个 interval,约 2s)。
                tokio::select! {
                    _ = interval.tick() => {}
                    _ = cyclic_shutdown.notified() => break,
                }
                if cyclic_flag.load(std::sync::atomic::Ordering::SeqCst) { break; }

                // Check if interval changed
                let new_interval_ms = get_interval_ms().await;
                if new_interval_ms != interval_ms {
                    interval_ms = new_interval_ms;
                    interval = tokio::time::interval(std::time::Duration::from_millis(interval_ms as u64));
                }

                let ops_snapshot = cyclic_remote_ops.read().await.clone();
                let stations_read = cyclic_stations.read().await;
                let addrs_to_remove: Vec<SocketAddr> = {
                    let mut conns = cyclic_connections.write().await;
                    let to_remove = Vec::new();
                    for (_addr, conn) in conns.iter_mut() {
                        if !conn.started.load(std::sync::atomic::Ordering::SeqCst) { continue; }
                        for station in stations_read.values() {
                            if !station.cyclic_config.enabled { continue; }
                            for point in station.data_points.all_sorted() {
                                let value_str = point.value.display();
                                if let Some(last) = conn.last_sent.get(&point.ioa) {
                                    if last == &value_str { continue; }
                                }
                                let ca_bytes = station.common_address.to_le_bytes();
                                let asdu = {
                                    let mut s = conn.seq.lock().await;
                                    let mut bytes = encode_point_frame_ex(point, 3, &ca_bytes, &mut *s, None);
                                    if ops_snapshot.sync_tb_by_category.enabled_for(point.asdu_type.category())
                                        && should_derive_tb(&station.data_points, point.asdu_type, point.ioa)
                                    {
                                        bytes.extend(encode_point_frame_ex(point, 3, &ca_bytes, &mut *s, Some(true)));
                                    }
                                    bytes
                                };
                                conn.queue.lock().await.extend(asdu);
                                conn.last_sent.insert(point.ioa, value_str);
                            }
                        }
                    }
                    to_remove
                };
                drop(stations_read);
                if !addrs_to_remove.is_empty() {
                    let mut conns = cyclic_connections.write().await;
                    for addr in addrs_to_remove {
                        conns.remove(&addr);
                        if let Some(ref lc) = cyclic_log {
                            lc.try_add(LogEntry::new(
                                Direction::Tx, FrameLabel::ConnectionEvent,
                                format!("连接关闭 (cyclic): {}", addr),
                            ));
                        }
                    }
                }
            }
        });
        self.cyclic_handle = Some(cyclic_handle);

        let handle = tokio::spawn(async move {
            loop {
                if shutdown_flag.load(std::sync::atomic::Ordering::SeqCst) { break; }
                match listener.accept().await {
                    Ok((stream, peer_addr)) => {
                        let peer_str = format!("{}", peer_addr);
                        if let Some(ref lc) = log_collector {
                            lc.try_add(LogEntry::new(
                                Direction::Rx, FrameLabel::ConnectionEvent,
                                format!("客户端连接: {}{}", peer_str, if is_tls { " (TLS)" } else { "" }),
                            ));
                        }
                        let stations = stations.clone();
                        let lc = log_collector.clone();
                        let flag = shutdown_flag.clone();
                        let tls_acceptor = tls_acceptor.clone();
                        let connections = connections.clone();
                        let conn_remote_ops = remote_ops.clone();
                        let conn_protocol_timing = protocol_timing.clone();

                        if tls_acceptor.is_some() {
                            // TLS: blocking I/O via spawn_blocking.
                            // Create a shared queue so queue_spontaneous() can enqueue frames
                            // that the blocking loop drains to the TLS stream.
                            let tls_queue: SharedQueue = Arc::new(tokio::sync::Mutex::new(Vec::new()));
                            let tls_seq: SharedSeq = Arc::new(tokio::sync::Mutex::new(SeqState::default()));
                            let tls_started = Arc::new(std::sync::atomic::AtomicBool::new(false));
                            connections.write().await.insert(peer_addr, ConnectionWrite {
                                queue: Arc::clone(&tls_queue),
                                seq: Arc::clone(&tls_seq),
                                started: Arc::clone(&tls_started),
                                last_sent: HashMap::new(),
                                log_collector: lc.clone(),
                            });
                            let tls_connections = connections.clone();
                            tokio::task::spawn_blocking(move || {
                                let tcp_stream = stream.into_std().expect("into_std");
                                // into_std() preserves tokio's non-blocking mode; switch to
                                // blocking so native-tls can perform synchronous handshake I/O.
                                tcp_stream.set_nonblocking(false).expect("set_nonblocking(false)");
                                let acceptor = tls_acceptor.as_ref().unwrap();
                                let mut tls_stream = match acceptor.accept(tcp_stream) {
                                    Ok(s) => s,
                                    Err(e) => {
                                        if let Some(ref lc) = lc {
                                            lc.try_add(LogEntry::new(
                                                Direction::Rx, FrameLabel::ConnectionEvent,
                                                format!("TLS 握手失败: {} - {}", peer_str, e),
                                            ));
                                        }
                                        // Clean up connection entry on failure
                                        let rt = tokio::runtime::Handle::try_current();
                                        if let Ok(h) = rt { h.block_on(async { tls_connections.write().await.remove(&peer_addr); }); }
                                        return;
                                    }
                                };
                                // Set read timeout so the loop can periodically drain the write queue.
                                let _ = tls_stream.get_ref().set_read_timeout(Some(std::time::Duration::from_millis(100)));
                                if let Some(ref lc) = lc {
                                    lc.try_add(LogEntry::new(
                                        Direction::Rx, FrameLabel::ConnectionEvent,
                                        format!("TLS 握手成功: {}", peer_str),
                                    ));
                                }
                                handle_client_blocking(&mut tls_stream, stations, lc, flag, tls_queue, tls_seq, tls_started, tls_connections, peer_addr, conn_remote_ops, conn_protocol_timing);
                            });
                        } else {
                            // Plain TCP: async with queue-based cyclic writes.
                            // Split into read/write halves so we can use the write half in a
                            // dedicated write task and pass read half to the read loop.
                            let (rh, wh) = tokio::io::split(stream);

                            // 对端关闭信号:读任务检测到 EOF/读错误退出后置位,用于让
                            // 写任务一并退出并 drop WriteHalf。否则空队列时写任务会无限
                            // 空转、永不感知对端 FIN,socket 永远停在 CLOSE_WAIT(FD 泄漏,
                            // 累积到 RLIMIT_NOFILE 后 accept 失败 → 新主站连不上)。
                            let conn_closed = Arc::new(std::sync::atomic::AtomicBool::new(false));
                            let conn_closed_for_writer = Arc::clone(&conn_closed);
                            let conn_closed_for_reader = Arc::clone(&conn_closed);

                            let queue: SharedQueue = Arc::new(tokio::sync::Mutex::new(Vec::new()));
                            let seq: SharedSeq = Arc::new(tokio::sync::Mutex::new(SeqState::default()));
                            let started = Arc::new(std::sync::atomic::AtomicBool::new(false));
                            let queue_for_writer = Arc::clone(&queue);
                            let queue_for_reader = Arc::clone(&queue);
                            let seq_for_reader = Arc::clone(&seq);
                            let started_for_reader = Arc::clone(&started);
                            let lc_for_reader = lc.clone();
                            let stations_for_reader = stations.clone();
                            let addr_for_read = peer_addr;

                            // Register connection for cyclic task.
                            connections.write().await.insert(peer_addr, ConnectionWrite {
                                queue,
                                seq,
                                started,
                                last_sent: HashMap::new(),
                                log_collector: lc.clone(),
                            });

                            // Spawn async write drain task (owns WriteHalf).
                            let flag_for_writer = flag.clone();
                            let conn_for_writer = Arc::clone(&connections);
                            tokio::spawn(async move {
                                let mut wh = wh;
                                loop {
                                    if flag_for_writer.load(std::sync::atomic::Ordering::SeqCst) { break; }
                                    // 读任务已检测到对端关闭 → 退出并 drop WriteHalf,彻底关闭 socket。
                                    if conn_closed_for_writer.load(std::sync::atomic::Ordering::SeqCst) { break; }
                                    // Atomically drain pending bytes under lock, then write outside lock
                                    let snapshot = {
                                        let mut bytes = queue_for_writer.lock().await;
                                        if bytes.is_empty() { Vec::new() } else { bytes.drain(..).collect::<Vec<u8>>() }
                                    };
                                    if !snapshot.is_empty() {
                                        match wh.write_all(&snapshot).await {
                                            Ok(()) => {}
                                            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                                                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
                                            }
                                            Err(_) => {
                                                conn_for_writer.write().await.remove(&addr_for_read);
                                                return;
                                            }
                                        }
                                        // 立即回到队列检查,避免大量帧时被 50ms 节流。
                                        tokio::task::yield_now().await;
                                    } else {
                                        // 空队列时短 sleep 避免忙轮询。
                                        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
                                    }
                                }
                            });

                            // Spawn read task (owns ReadHalf + queue for enqueueing responses).
                            tokio::spawn(async move {
                                handle_client_read_loop(rh, stations_for_reader, lc_for_reader, flag, connections, queue_for_reader, seq_for_reader, started_for_reader, addr_for_read, conn_remote_ops, conn_protocol_timing).await;
                                // 读循环退出 = 对端已断开,通知写任务退出以释放 WriteHalf。
                                conn_closed_for_reader.store(true, std::sync::atomic::Ordering::SeqCst);
                            });
                        }
                    }
                    Err(_) => { tokio::time::sleep(std::time::Duration::from_millis(50)).await; }
                }
            }
        });

        self.server_handle = Some(handle);
        self.state = ServerState::Running;
        if let Some(ref lc) = self.log_collector {
            lc.try_add(LogEntry::new(
                Direction::Tx, FrameLabel::ConnectionEvent,
                format!("服务器启动: {}{}", addr_str, if is_tls { " (TLS)" } else { "" }),
            ));
        }
        Ok(())
    }

    pub async fn stop(&mut self) -> Result<(), SlaveError> {
        if self.state == ServerState::Stopped { return Err(SlaveError::NotRunning); }
        self.shutdown_flag.store(true, std::sync::atomic::Ordering::SeqCst);
        // 立即唤醒周期任务(否则它要等到下一个 interval tick 才检查退出标志,
        // 而 stop_server 命令在此期间持有全局写锁 → 前端所有轮询 IPC 被冻结)。
        self.shutdown_notify.notify_one();
        // Connect briefly to unblock listener.accept()
        let addr = format!("{}:{}", self.transport.bind_address, self.transport.port);
        let _ = tokio::net::TcpStream::connect(&addr).await;
        if let Some(h) = self.server_handle.take() { let _ = h.await; }
        if let Some(h) = self.cyclic_handle.take() { let _ = h.await; }
        {
            let mut handles = self.point_mutation_handles.lock().await;
            for (_k, (h, _mode)) in handles.drain() { h.abort(); }
        }
        self.state = ServerState::Stopped;
        if let Some(ref lc) = self.log_collector {
            lc.try_add(LogEntry::new(
                Direction::Tx, FrameLabel::ConnectionEvent,
                "服务器停止".to_string(),
            ));
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Shared Queue type alias
// ---------------------------------------------------------------------------
type SharedQueue = Arc<tokio::sync::Mutex<Vec<u8>>>;

// ---------------------------------------------------------------------------
// Async Client Read Loop
// ---------------------------------------------------------------------------

async fn handle_client_read_loop(
    mut stream: tokio::io::ReadHalf<AsyncTcpStream>,
    stations: SharedStations,
    log_collector: Option<Arc<LogCollector>>,
    shutdown_flag: Arc<std::sync::atomic::AtomicBool>,
    connections: SharedConnections,
    queue: SharedQueue,
    seq: SharedSeq,
    started: Arc<std::sync::atomic::AtomicBool>,
    peer_addr: SocketAddr,
    remote_ops: SharedRemoteOps,
    protocol_timing: SharedProtocolTiming,
) {
    let mut buf = [0u8; 8192];
    let mut reassembly_buf: Vec<u8> = Vec::with_capacity(65536);

    loop {
        if shutdown_flag.load(std::sync::atomic::Ordering::SeqCst) { break; }
        // stream.read 本身是异步阻塞,会在数据到达或对端关闭时立即唤醒。
        // 多余的 sleep(50) 会把 k 窗口的 ACK 反馈周期拉到 ≥50ms,严重拖慢大批量回送。
        let n = match stream.read(&mut buf).await {
            Ok(0) => break,
            Ok(n) => n,
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => continue,
            Err(_) => break,
        };

        reassembly_buf.extend_from_slice(&buf[..n]);

        // Extract and process complete frames from the reassembly buffer
        while reassembly_buf.len() >= 2 {
            if reassembly_buf[0] != 0x68 {
                reassembly_buf.remove(0);
                continue;
            }
            let frame_len = reassembly_buf[1] as usize + 2;
            if reassembly_buf.len() < frame_len { break; }
            let data: Vec<u8> = reassembly_buf.drain(..frame_len).collect();
            let n = data.len();

        if let Some(ref lc) = log_collector {
            if let Ok(frame) = crate::frame::parse_apci(&data) {
                let summary = crate::frame::format_frame_summary(&frame);
                lc.try_add(LogEntry::with_raw_bytes(
                    Direction::Rx, FrameLabel::IFrame(summary.clone()),
                    summary, data.to_vec(),
                ));
            }
        }

        if data.len() >= 6 && data[0] == 0x68 {
            let ctrl1 = data[2];

            if ctrl1 & 0x03 == 0x03 {
                match ctrl1 {
                    0x07 => {
                        // STARTDT_ACT → enable data transfer, then confirm.
                        started.store(true, std::sync::atomic::Ordering::SeqCst);
                        let resp = [0x68, 0x04, 0x0B, 0x00, 0x00, 0x00];
                        queue.lock().await.extend_from_slice(&resp);
                        if let Some(ref lc) = log_collector {
                            lc.try_add(LogEntry::with_raw_bytes(Direction::Tx, FrameLabel::UStartCon, "STARTDT CON", resp.to_vec()));
                        }
                    }
                    0x13 => {
                        // STOPDT_ACT → disable data transfer, then confirm.
                        started.store(false, std::sync::atomic::Ordering::SeqCst);
                        let resp = [0x68, 0x04, 0x23, 0x00, 0x00, 0x00];
                        queue.lock().await.extend_from_slice(&resp);
                        if let Some(ref lc) = log_collector {
                            lc.try_add(LogEntry::with_raw_bytes(Direction::Tx, FrameLabel::UStopCon, "STOPDT CON", resp.to_vec()));
                        }
                    }
                    0x43 => {
                        let resp = [0x68, 0x04, 0x83, 0x00, 0x00, 0x00];
                        queue.lock().await.extend_from_slice(&resp);
                        if let Some(ref lc) = log_collector {
                            lc.try_add(LogEntry::with_raw_bytes(Direction::Tx, FrameLabel::UTestCon, "TESTFR CON", resp.to_vec()));
                        }
                    }
                    _ => {}
                }
            } else if ctrl1 & 0x03 == 0x01 && data.len() >= 6 {
                // S 帧：仅承载 N(R)，推进我方 ack_ssn（k 窗口 sender 端）。
                { let mut s = seq.lock().await; observe_recv_sframe(&mut s, &data); }
                if let Some(ref lc) = log_collector {
                    let nr = u16::from_le_bytes([data[4], data[5]]) >> 1;
                    lc.try_add(LogEntry::with_raw_bytes(
                        Direction::Rx, FrameLabel::SFrame,
                        format!("S frame N(R)={}", nr), data.to_vec(),
                    ));
                }
            } else if ctrl1 & 0x01 == 0 && data.len() >= 12 {
                { let mut s = seq.lock().await; observe_recv_iframe(&mut s, &data); }
                // w 窗口：累计未确认接收 I 帧数达到 w 时主动回 S 帧。
                let w = protocol_timing.read().await.w;
                let maybe_sframe = {
                    let mut s = seq.lock().await;
                    if w > 0 && s.unacked_recv >= 2u16.saturating_mul(w) {
                        let rsn_now = s.rsn;
                        s.unacked_recv = 0;
                        s.last_s_ack_at = tokio::time::Instant::now();
                        Some([0x68u8, 0x04, 0x01, 0x00,
                              (rsn_now & 0xFF) as u8, ((rsn_now >> 8) & 0xFF) as u8])
                    } else { None }
                };
                if let Some(sframe) = maybe_sframe {
                    queue.lock().await.extend_from_slice(&sframe);
                    if let Some(ref lc) = log_collector {
                        lc.try_add(LogEntry::with_raw_bytes(
                            Direction::Tx, FrameLabel::SFrame,
                            "S frame (w window)".to_string(), sframe.to_vec(),
                        ));
                    }
                }
                let asdu_type = data[6];
                let cause = data[8];
                let ca = u16::from_le_bytes([data[10], data[11]]);

                let ops_snapshot = remote_ops.read().await.clone();
                match asdu_type {
                    100 => {
                        if !ops_snapshot.answer_general_interrogation {
                            if let Some(ref lc) = log_collector {
                                lc.try_add(LogEntry::new(
                                    Direction::Tx, FrameLabel::GeneralInterrogation,
                                    format!("GI 已抑制响应(answer_general_interrogation=false) CA={}", ca),
                                ));
                            }
                        } else {
                            // ACT_CON 立即入队，避免主站 t1 超时。
                            let con = {
                                let mut s = seq.lock().await;
                                build_response_frame(&data[..n], 7, &mut s)
                            };
                            queue.lock().await.extend_from_slice(&con);
                            if let Some(ref lc) = log_collector {
                                lc.try_add(LogEntry::new(
                                    Direction::Tx, FrameLabel::GeneralInterrogation,
                                    format!("GI 激活确认 CA={}", ca),
                                ));
                            }
                            // 拷贝点位快照，释放 stations 读锁后再去独立 task 发送。
                            let (points_snapshot, ca_bytes_opt): (Vec<DataPoint>, Option<[u8; 2]>) = {
                                let stations_read = stations.read().await;
                                if let Some(station) = stations_read.get(&ca) {
                                    // GI (C_IC_NA_1) 只召唤过程信息;累积量 (M_IT) 仅由
                                    // 计数量召唤 (C_CI_NA_1) 上送,此处排除。
                                    let pts: Vec<DataPoint> = station.data_points.all_sorted()
                                        .into_iter()
                                        .filter(|p| !matches!(p.value, DataPointValue::IntegratedTotal { .. }))
                                        .cloned().collect();
                                    (pts, Some(station.common_address.to_le_bytes()))
                                } else {
                                    (Vec::new(), None)
                                }
                            };
                            if let Some(ca_bytes) = ca_bytes_opt {
                                let k = protocol_timing.read().await.k;
                                let queue_clone = Arc::clone(&queue);
                                let seq_clone = Arc::clone(&seq);
                                let lc_clone = log_collector.clone();
                                let recv_template = data[..n].to_vec();
                                let include_ts = ops_snapshot.gi_include_timestamped;
                                tokio::spawn(async move {
                                    run_interrogation(
                                        points_snapshot, ca_bytes, 20,
                                        recv_template, include_ts,
                                        queue_clone, seq_clone, k, lc_clone,
                                        FrameLabel::GeneralInterrogation, ca,
                                    ).await;
                                });
                            }
                        }
                    }
                    101 => {
                        if !ops_snapshot.answer_counter_interrogation {
                            if let Some(ref lc) = log_collector {
                                lc.try_add(LogEntry::new(
                                    Direction::Tx, FrameLabel::CounterInterrogation,
                                    format!("累计量召唤已抑制响应(answer_counter_interrogation=false) CA={}", ca),
                                ));
                            }
                        } else {
                            let con = {
                                let mut s = seq.lock().await;
                                build_response_frame(&data[..n], 7, &mut s)
                            };
                            queue.lock().await.extend_from_slice(&con);
                            if let Some(ref lc) = log_collector {
                                lc.try_add(LogEntry::new(
                                    Direction::Tx, FrameLabel::CounterInterrogation,
                                    format!("累计量召唤 激活确认 CA={}", ca),
                                ));
                            }
                            let (points_snapshot, ca_bytes_opt): (Vec<DataPoint>, Option<[u8; 2]>) = {
                                let stations_read = stations.read().await;
                                if let Some(station) = stations_read.get(&ca) {
                                    let pts: Vec<DataPoint> = station.data_points.all_sorted()
                                        .into_iter()
                                        .filter(|p| matches!(p.value, DataPointValue::IntegratedTotal { .. }))
                                        .cloned().collect();
                                    (pts, Some(station.common_address.to_le_bytes()))
                                } else {
                                    (Vec::new(), None)
                                }
                            };
                            if let Some(ca_bytes) = ca_bytes_opt {
                                let k = protocol_timing.read().await.k;
                                let queue_clone = Arc::clone(&queue);
                                let seq_clone = Arc::clone(&seq);
                                let lc_clone = log_collector.clone();
                                let recv_template = data[..n].to_vec();
                                let include_ts = ops_snapshot.gi_include_timestamped;
                                tokio::spawn(async move {
                                    run_interrogation(
                                        points_snapshot, ca_bytes, 37,
                                        recv_template, include_ts,
                                        queue_clone, seq_clone, k, lc_clone,
                                        FrameLabel::CounterInterrogation, ca,
                                    ).await;
                                });
                            }
                        }
                    }
                    103 => {
                        let ack = { let mut s = seq.lock().await; build_response_frame(&data[..n], 7, &mut s) };
                        queue.lock().await.extend_from_slice(&ack);
                        if let Some(ref lc) = log_collector {
                            lc.try_add(LogEntry::new(
                                Direction::Tx, FrameLabel::ClockSync,
                                format!("时钟同步确认 CA={}", ca),
                            ));
                        }
                    }
                    45 => {
                        if data.len() >= 16 {
                            let ioa = u32::from_le_bytes([data[12], data[13], data[14], 0]);
                            let sco = data[15]; let value = sco & 0x01 != 0; let is_select = sco & 0x80 != 0;
                            if !is_select {
                                let mut s = stations.write().await;
                                if let Some(st) = s.get_mut(&ca) {
                                    if let Some(dp) = st.data_points.get_mut_by_category(ioa, DataCategory::SinglePoint) {
                                        dp.value = DataPointValue::SinglePoint { value };
                                        dp.timestamp = Some(chrono::Utc::now());
                                    }
                                    st.data_points.mark_changed_by_category(ioa, DataCategory::SinglePoint);
                                }
                            }
                            if ops_snapshot.answer_commands {
                            let ack_cot = if is_select { ops_snapshot.select_ack_cot.as_u8() } else { 7u8 /* ActivationCon: SBO 协议要求 execute ack 恒为 7,终止帧另用 execute_ack_cot */ };
                            let ack = { let mut s = seq.lock().await; build_response_frame(&data[..n], ack_cot, &mut s) };
                            queue.lock().await.extend_from_slice(&ack);
                            if !is_select {
                                let term = { let mut s = seq.lock().await; build_response_frame(&data[..n], ops_snapshot.execute_ack_cot.as_u8(), &mut s) };
                                queue.lock().await.extend_from_slice(&term);
                                // Send spontaneous update (COT=3) after control execution
                                let sr = stations.read().await;
                                if let Some(st) = sr.get(&ca) {
                                    if let Some(point) = st.data_points.get_by_category(ioa, DataCategory::SinglePoint) {
                                        let ca_b = ca.to_le_bytes();
                                        let _ioa_b = ioa.to_le_bytes();
                                        let spont = {
                                            let mut s = seq.lock().await;
                                            encode_point_frame_ex(point, 3, &ca_b, &mut *s, None)
                                        };
                                        queue.lock().await.extend_from_slice(&spont);
                                    }
                                }
                            }
                            }
                            if let Some(ref lc) = log_collector {
                                let mode = if is_select { "Select" } else { "Execute" };
                                lc.try_add(LogEntry::new(
                                    Direction::Tx, FrameLabel::SingleCommand,
                                    format!("单点命令确认 IOA={} val={} {} CA={}", ioa, value, mode, ca),
                                ));
                            }
                        }
                    }
                    46 => {
                        if data.len() >= 16 {
                            let ioa = u32::from_le_bytes([data[12], data[13], data[14], 0]);
                            let dco = data[15]; let value = dco & 0x03; let is_select = dco & 0x80 != 0;
                            if !is_select {
                                let mut s = stations.write().await;
                                if let Some(st) = s.get_mut(&ca) {
                                    if let Some(dp) = st.data_points.get_mut_by_category(ioa, DataCategory::DoublePoint) {
                                        dp.value = DataPointValue::DoublePoint { value };
                                        dp.timestamp = Some(chrono::Utc::now());
                                    }
                                    st.data_points.mark_changed_by_category(ioa, DataCategory::DoublePoint);
                                }
                            }
                            if ops_snapshot.answer_commands {
                            let ack_cot = if is_select { ops_snapshot.select_ack_cot.as_u8() } else { 7u8 /* ActivationCon: SBO 协议要求 execute ack 恒为 7,终止帧另用 execute_ack_cot */ };
                            let ack = { let mut s = seq.lock().await; build_response_frame(&data[..n], ack_cot, &mut s) };
                            queue.lock().await.extend_from_slice(&ack);
                            if !is_select {
                                let term = { let mut s = seq.lock().await; build_response_frame(&data[..n], ops_snapshot.execute_ack_cot.as_u8(), &mut s) };
                                queue.lock().await.extend_from_slice(&term);
                                let sr = stations.read().await;
                                if let Some(st) = sr.get(&ca) {
                                    if let Some(point) = st.data_points.get_by_category(ioa, DataCategory::DoublePoint) {
                                        let ca_b = ca.to_le_bytes();
                                        let _ioa_b = ioa.to_le_bytes();
                                        let spont = {
                                            let mut s = seq.lock().await;
                                            encode_point_frame_ex(point, 3, &ca_b, &mut *s, None)
                                        };
                                        queue.lock().await.extend_from_slice(&spont);
                                    }
                                }
                            }
                            }
                            if let Some(ref lc) = log_collector {
                                let mode = if is_select { "Select" } else { "Execute" };
                                lc.try_add(LogEntry::new(
                                    Direction::Tx, FrameLabel::DoubleCommand,
                                    format!("双点命令确认 IOA={} val={} {} CA={}", ioa, value, mode, ca),
                                ));
                            }
                        }
                    }
                    47 => {
                        if data.len() >= 16 {
                            let ioa = u32::from_le_bytes([data[12], data[13], data[14], 0]);
                            let rco = data[15]; let step_val = rco & 0x03; let is_select = rco & 0x80 != 0;
                            if !is_select {
                                let mut s = stations.write().await;
                                if let Some(st) = s.get_mut(&ca) {
                                    if let Some(dp) = st.data_points.get_mut_by_category(ioa, DataCategory::StepPosition) {
                                        if let DataPointValue::StepPosition { ref mut value, .. } = dp.value {
                                            match step_val { 1 => { if *value > -64 { *value -= 1; } } 2 => { if *value < 63 { *value += 1; } } _ => {} }
                                            dp.timestamp = Some(chrono::Utc::now());
                                        }
                                    }
                                    st.data_points.mark_changed_by_category(ioa, DataCategory::StepPosition);
                                }
                            }
                            if ops_snapshot.answer_commands {
                            let ack_cot = if is_select { ops_snapshot.select_ack_cot.as_u8() } else { 7u8 /* ActivationCon: SBO 协议要求 execute ack 恒为 7,终止帧另用 execute_ack_cot */ };
                            let ack = { let mut s = seq.lock().await; build_response_frame(&data[..n], ack_cot, &mut s) };
                            queue.lock().await.extend_from_slice(&ack);
                            if !is_select {
                                let term = { let mut s = seq.lock().await; build_response_frame(&data[..n], ops_snapshot.execute_ack_cot.as_u8(), &mut s) };
                                queue.lock().await.extend_from_slice(&term);
                                let sr = stations.read().await;
                                if let Some(st) = sr.get(&ca) {
                                    if let Some(point) = st.data_points.get_by_category(ioa, DataCategory::StepPosition) {
                                        let ca_b = ca.to_le_bytes();
                                        let _ioa_b = ioa.to_le_bytes();
                                        let spont = {
                                            let mut s = seq.lock().await;
                                            encode_point_frame_ex(point, 3, &ca_b, &mut *s, None)
                                        };
                                        queue.lock().await.extend_from_slice(&spont);
                                    }
                                }
                            }
                            }
                            if let Some(ref lc) = log_collector {
                                let mode = if is_select { "Select" } else { "Execute" };
                                let dir = match step_val { 1 => "降", 2 => "升", _ => "?" };
                                lc.try_add(LogEntry::new(
                                    Direction::Tx, FrameLabel::StepCommand,
                                    format!("步调节命令确认 IOA={} {} {} CA={}", ioa, dir, mode, ca),
                                ));
                            }
                        }
                    }
                    48 => {
                        if data.len() >= 18 {
                            let ioa = u32::from_le_bytes([data[12], data[13], data[14], 0]);
                            let nva = i16::from_le_bytes([data[15], data[16]]);
                            let qos = data[17]; let is_select = qos & 0x80 != 0;
                            let value = nva as f32 / 32767.0;
                            if !is_select {
                                let mut s = stations.write().await;
                                if let Some(st) = s.get_mut(&ca) {
                                    if let Some(dp) = st.data_points.get_mut_by_category(ioa, DataCategory::NormalizedMeasured) {
                                        dp.value = DataPointValue::Normalized { value };
                                        dp.timestamp = Some(chrono::Utc::now());
                                    }
                                    st.data_points.mark_changed_by_category(ioa, DataCategory::NormalizedMeasured);
                                }
                            }
                            if ops_snapshot.answer_commands {
                            let ack_cot = if is_select { ops_snapshot.select_ack_cot.as_u8() } else { 7u8 /* ActivationCon: SBO 协议要求 execute ack 恒为 7,终止帧另用 execute_ack_cot */ };
                            let ack = { let mut s = seq.lock().await; build_response_frame(&data[..n], ack_cot, &mut s) };
                            queue.lock().await.extend_from_slice(&ack);
                            if !is_select {
                                let term = { let mut s = seq.lock().await; build_response_frame(&data[..n], ops_snapshot.execute_ack_cot.as_u8(), &mut s) };
                                queue.lock().await.extend_from_slice(&term);
                                let sr = stations.read().await;
                                if let Some(st) = sr.get(&ca) {
                                    if let Some(point) = st.data_points.get_by_category(ioa, DataCategory::NormalizedMeasured) {
                                        let ca_b = ca.to_le_bytes();
                                        let _ioa_b = ioa.to_le_bytes();
                                        let spont = {
                                            let mut s = seq.lock().await;
                                            encode_point_frame_ex(point, 3, &ca_b, &mut *s, None)
                                        };
                                        queue.lock().await.extend_from_slice(&spont);
                                    }
                                }
                            }
                            }
                            if let Some(ref lc) = log_collector {
                                let mode = if is_select { "Select" } else { "Execute" };
                                lc.try_add(LogEntry::new(
                                    Direction::Tx, FrameLabel::SetpointNormalized,
                                    format!("归一化设定值确认 IOA={} val={:.4} {} CA={}", ioa, value, mode, ca),
                                ));
                            }
                        }
                    }
                    49 => {
                        if data.len() >= 18 {
                            let ioa = u32::from_le_bytes([data[12], data[13], data[14], 0]);
                            let sva = i16::from_le_bytes([data[15], data[16]]);
                            let qos = data[17]; let is_select = qos & 0x80 != 0;
                            if !is_select {
                                let mut s = stations.write().await;
                                if let Some(st) = s.get_mut(&ca) {
                                    if let Some(dp) = st.data_points.get_mut_by_category(ioa, DataCategory::ScaledMeasured) {
                                        dp.value = DataPointValue::Scaled { value: sva };
                                        dp.timestamp = Some(chrono::Utc::now());
                                    }
                                    st.data_points.mark_changed_by_category(ioa, DataCategory::ScaledMeasured);
                                }
                            }
                            if ops_snapshot.answer_commands {
                            let ack_cot = if is_select { ops_snapshot.select_ack_cot.as_u8() } else { 7u8 /* ActivationCon: SBO 协议要求 execute ack 恒为 7,终止帧另用 execute_ack_cot */ };
                            let ack = { let mut s = seq.lock().await; build_response_frame(&data[..n], ack_cot, &mut s) };
                            queue.lock().await.extend_from_slice(&ack);
                            if !is_select {
                                let term = { let mut s = seq.lock().await; build_response_frame(&data[..n], ops_snapshot.execute_ack_cot.as_u8(), &mut s) };
                                queue.lock().await.extend_from_slice(&term);
                                let sr = stations.read().await;
                                if let Some(st) = sr.get(&ca) {
                                    if let Some(point) = st.data_points.get_by_category(ioa, DataCategory::ScaledMeasured) {
                                        let ca_b = ca.to_le_bytes();
                                        let _ioa_b = ioa.to_le_bytes();
                                        let spont = {
                                            let mut s = seq.lock().await;
                                            encode_point_frame_ex(point, 3, &ca_b, &mut *s, None)
                                        };
                                        queue.lock().await.extend_from_slice(&spont);
                                    }
                                }
                            }
                            }
                            if let Some(ref lc) = log_collector {
                                let mode = if is_select { "Select" } else { "Execute" };
                                lc.try_add(LogEntry::new(
                                    Direction::Tx, FrameLabel::SetpointScaled,
                                    format!("标度化设定值确认 IOA={} val={} {} CA={}", ioa, sva, mode, ca),
                                ));
                            }
                        }
                    }
                    50 => {
                        if data.len() >= 20 {
                            let ioa = u32::from_le_bytes([data[12], data[13], data[14], 0]);
                            let value = f32::from_le_bytes([data[15], data[16], data[17], data[18]]);
                            let qos = data[19]; let is_select = qos & 0x80 != 0;
                            if !is_select {
                                let mut s = stations.write().await;
                                if let Some(st) = s.get_mut(&ca) {
                                    if let Some(dp) = st.data_points.get_mut_by_category(ioa, DataCategory::FloatMeasured) {
                                        dp.value = DataPointValue::ShortFloat { value };
                                        dp.timestamp = Some(chrono::Utc::now());
                                    }
                                    st.data_points.mark_changed_by_category(ioa, DataCategory::FloatMeasured);
                                }
                            }
                            if ops_snapshot.answer_commands {
                            let ack_cot = if is_select { ops_snapshot.select_ack_cot.as_u8() } else { 7u8 /* ActivationCon: SBO 协议要求 execute ack 恒为 7,终止帧另用 execute_ack_cot */ };
                            let ack = { let mut s = seq.lock().await; build_response_frame(&data[..n], ack_cot, &mut s) };
                            queue.lock().await.extend_from_slice(&ack);
                            if !is_select {
                                let term = { let mut s = seq.lock().await; build_response_frame(&data[..n], ops_snapshot.execute_ack_cot.as_u8(), &mut s) };
                                queue.lock().await.extend_from_slice(&term);
                                let sr = stations.read().await;
                                if let Some(st) = sr.get(&ca) {
                                    if let Some(point) = st.data_points.get_by_category(ioa, DataCategory::FloatMeasured) {
                                        let ca_b = ca.to_le_bytes();
                                        let _ioa_b = ioa.to_le_bytes();
                                        let spont = {
                                            let mut s = seq.lock().await;
                                            encode_point_frame_ex(point, 3, &ca_b, &mut *s, None)
                                        };
                                        queue.lock().await.extend_from_slice(&spont);
                                    }
                                }
                            }
                            }
                            if let Some(ref lc) = log_collector {
                                let mode = if is_select { "Select" } else { "Execute" };
                                lc.try_add(LogEntry::new(
                                    Direction::Tx, FrameLabel::SetpointFloat,
                                    format!("浮点设定值确认 IOA={} val={:.3} {} CA={}", ioa, value, mode, ca),
                                ));
                            }
                        }
                    }
                    _ => {
                        if let Some(ref lc) = log_collector {
                            lc.try_add(LogEntry::new(
                                Direction::Rx, FrameLabel::IFrame(format!("Type{}", asdu_type)),
                                format!("未知 ASDU 类型={} CA={} COT={}", asdu_type, ca, cause),
                            ));
                        }
                    }
                }
            }
        }
        } // end while reassembly_buf
    }

    connections.write().await.remove(&peer_addr);
    if let Some(ref lc) = log_collector {
        lc.try_add(LogEntry::new(
            Direction::Tx, FrameLabel::ConnectionEvent,
            format!("连接关闭: {}", peer_addr),
        ));
    }
}

// ---------------------------------------------------------------------------
// Blocking Client Handler (for TLS)
// ---------------------------------------------------------------------------

fn handle_client_blocking(
    stream: &mut native_tls::TlsStream<TcpStream>,
    stations: SharedStations,
    log_collector: Option<Arc<LogCollector>>,
    shutdown_flag: Arc<std::sync::atomic::AtomicBool>,
    write_queue: SharedQueue,
    seq: SharedSeq,
    started: Arc<std::sync::atomic::AtomicBool>,
    connections: SharedConnections,
    peer_addr: SocketAddr,
    remote_ops: SharedRemoteOps,
    _protocol_timing: SharedProtocolTiming,
) {
    use std::io::{Read, Write};
    let mut buf = [0u8; 512];

    // Cache the runtime handle once — this function always runs inside spawn_blocking.
    let rt = tokio::runtime::Handle::current();

    // Drain the shared write queue to the TLS stream.
    let drain_queue = |stream: &mut native_tls::TlsStream<TcpStream>, queue: &SharedQueue, rt: &tokio::runtime::Handle| {
        let pending = rt.block_on(async {
            let mut q = queue.lock().await;
            if q.is_empty() { Vec::new() } else { q.drain(..).collect::<Vec<u8>>() }
        });
        if !pending.is_empty() {
            let _ = stream.write_all(&pending);
        }
    };

    loop {
        if shutdown_flag.load(std::sync::atomic::Ordering::SeqCst) { break; }
        let n = match stream.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => n,
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock || e.kind() == std::io::ErrorKind::TimedOut => {
                // Timeout hit — drain queue and continue waiting for data.
                drain_queue(stream, &write_queue, &rt);
                continue;
            }
            Err(_) => break,
        };

        let data = &buf[..n];

        if let Some(ref lc) = log_collector {
            if let Ok(frame) = crate::frame::parse_apci(data) {
                let summary = crate::frame::format_frame_summary(&frame);
                lc.try_add(LogEntry::with_raw_bytes(
                    Direction::Rx, FrameLabel::IFrame(summary.clone()),
                    summary, data.to_vec(),
                ));
            }
        }

        if data.len() >= 6 && data[0] == 0x68 {
            let ctrl1 = data[2];

            if ctrl1 & 0x03 == 0x03 {
                match ctrl1 {
                    0x07 => {
                        // STARTDT_ACT → enable data transfer, then confirm.
                        started.store(true, std::sync::atomic::Ordering::SeqCst);
                        let resp = [0x68, 0x04, 0x0B, 0x00, 0x00, 0x00];
                        let _ = stream.write_all(&resp);
                        if let Some(ref lc) = log_collector {
                            lc.try_add(LogEntry::with_raw_bytes(Direction::Tx, FrameLabel::UStartCon, "STARTDT CON", resp.to_vec()));
                        }
                    }
                    0x13 => {
                        // STOPDT_ACT → disable data transfer, then confirm.
                        started.store(false, std::sync::atomic::Ordering::SeqCst);
                        let resp = [0x68, 0x04, 0x23, 0x00, 0x00, 0x00];
                        let _ = stream.write_all(&resp);
                        if let Some(ref lc) = log_collector {
                            lc.try_add(LogEntry::with_raw_bytes(Direction::Tx, FrameLabel::UStopCon, "STOPDT CON", resp.to_vec()));
                        }
                    }
                    0x43 => {
                        let resp = [0x68, 0x04, 0x83, 0x00, 0x00, 0x00];
                        let _ = stream.write_all(&resp);
                        if let Some(ref lc) = log_collector {
                            lc.try_add(LogEntry::with_raw_bytes(Direction::Tx, FrameLabel::UTestCon, "TESTFR CON", resp.to_vec()));
                        }
                    }
                    _ => {}
                }
            } else if ctrl1 & 0x01 == 0 && data.len() >= 12 {
                rt.block_on(async { let mut s = seq.lock().await; observe_recv_iframe(&mut s, data); });
                let asdu_type = data[6];
                let cause = data[8];
                let ca = u16::from_le_bytes([data[10], data[11]]);

                let ops_snapshot = rt.block_on(async { remote_ops.read().await.clone() });
                match asdu_type {
                    100 => {
                        if !ops_snapshot.answer_general_interrogation {
                            if let Some(ref lc) = log_collector {
                                lc.try_add(LogEntry::new(
                                    Direction::Tx, FrameLabel::GeneralInterrogation,
                                    format!("GI 已抑制响应(answer_general_interrogation=false) CA={}", ca),
                                ));
                            }
                        } else {
                        let ack = rt.block_on(async { let mut s = seq.lock().await; build_response_frame(&data[..n], 7, &mut s) });
                        let _ = stream.write_all(&ack);
                        let stations_cl = stations.clone();
                        let lc = log_collector.clone();
                        let seq_cl = seq.clone();
                        let ops_for_send = ops_snapshot.clone();
                        rt.block_on(async {
                            let stations_read = stations_cl.read().await;
                            if let Some(station) = stations_read.get(&ca) {
                                if let Some(ref lc) = lc {
                                    lc.try_add(LogEntry::new(
                                        Direction::Tx, FrameLabel::GeneralInterrogation,
                                        format!("GI 激活确认 CA={}", ca),
                                    ));
                                }
                                send_gi_response_blocking(stream, station, &seq_cl, &ops_for_send).await;
                            }
                            drop(stations_read);
                            let term = { let mut s = seq_cl.lock().await; build_response_frame(&data[..n], 10, &mut s) };
                            let _ = stream.write_all(&term);
                            if let Some(ref lc) = lc {
                                lc.try_add(LogEntry::new(
                                    Direction::Tx, FrameLabel::GeneralInterrogation,
                                    format!("GI 激活终止 CA={}", ca),
                                ));
                            }
                        });
                        }
                    }
                    101 => {
                        // Counter Interrogation (C_CI_NA_1, Type 101)
                        if !ops_snapshot.answer_counter_interrogation {
                            if let Some(ref lc) = log_collector {
                                lc.try_add(LogEntry::new(
                                    Direction::Tx, FrameLabel::CounterInterrogation,
                                    format!("累计量召唤已抑制响应(answer_counter_interrogation=false) CA={}", ca),
                                ));
                            }
                        } else {
                        let stations_cl = stations.clone();
                        let lc = log_collector.clone();
                        let seq_cl = seq.clone();
                        let ops_for_batch = ops_snapshot.clone();
                        let batch = rt.block_on(async {
                            let mut batch: Vec<u8> = Vec::new();
                            let mut s = seq_cl.lock().await;
                            batch.extend_from_slice(&build_response_frame(&data[..n], 7, &mut s));
                            let stations_read = stations_cl.read().await;
                            if let Some(station) = stations_read.get(&ca) {
                                if let Some(ref lc) = lc {
                                    lc.try_add(LogEntry::new(
                                        Direction::Tx, FrameLabel::CounterInterrogation,
                                        format!("累计量召唤 激活确认 CA={}", ca),
                                    ));
                                }
                                let ca_bytes = station.common_address.to_le_bytes();
                                for point in station.data_points.all_sorted() {
                                    if !matches!(point.value, DataPointValue::IntegratedTotal { .. }) { continue; }
                                    batch.extend_from_slice(&encode_point_frame_ex(point, 37, &ca_bytes, &mut s, Some(false)));
                                    if ops_for_batch.gi_include_timestamped
                                        && should_derive_tb(&station.data_points, point.asdu_type, point.ioa)
                                    {
                                        batch.extend_from_slice(&encode_point_frame_ex(point, 37, &ca_bytes, &mut s, Some(true)));
                                    }
                                }
                            }
                            batch.extend_from_slice(&build_response_frame(&data[..n], 10, &mut s));
                            if let Some(ref lc) = lc {
                                lc.try_add(LogEntry::new(
                                    Direction::Tx, FrameLabel::CounterInterrogation,
                                    format!("累计量召唤 激活终止 CA={}", ca),
                                ));
                            }
                            batch
                        });
                        let _ = stream.write_all(&batch);
                        }
                    }
                    103 => {
                        let ack = rt.block_on(async { let mut s = seq.lock().await; build_response_frame(&data[..n], 7, &mut s) });
                        let _ = stream.write_all(&ack);
                        if let Some(ref lc) = log_collector {
                            lc.try_add(LogEntry::new(
                                Direction::Tx, FrameLabel::ClockSync,
                                format!("时钟同步确认 CA={}", ca),
                            ));
                        }
                    }
                    45 => {
                        if data.len() >= 16 {
                            let ioa = u32::from_le_bytes([data[12], data[13], data[14], 0]);
                            let sco = data[15]; let value = sco & 0x01 != 0; let is_select = sco & 0x80 != 0;
                            if !is_select {
                                let rt = tokio::runtime::Handle::try_current();
                                if let Ok(handle) = rt {
                                    let stations = stations.clone();
                                    handle.block_on(async {
                                        let mut s = stations.write().await;
                                        if let Some(st) = s.get_mut(&ca) {
                                            if let Some(dp) = st.data_points.get_mut_by_category(ioa, DataCategory::SinglePoint) {
                                                dp.value = DataPointValue::SinglePoint { value };
                                                dp.timestamp = Some(chrono::Utc::now());
                                            }
                                            st.data_points.mark_changed_by_category(ioa, DataCategory::SinglePoint);
                                        }
                                    });
                                }
                            }
                            if ops_snapshot.answer_commands {
                            let ack_cot = if is_select { ops_snapshot.select_ack_cot.as_u8() } else { 7u8 /* ActivationCon: SBO 协议要求 execute ack 恒为 7,终止帧另用 execute_ack_cot */ };
                            let ack = rt.block_on(async { let mut s = seq.lock().await; build_response_frame(&data[..n], ack_cot, &mut s) });
                            let _ = stream.write_all(&ack);
                            if !is_select {
                                let term = rt.block_on(async { let mut s = seq.lock().await; build_response_frame(&data[..n], ops_snapshot.execute_ack_cot.as_u8(), &mut s) });
                                let _ = stream.write_all(&term);
                                if let Ok(handle) = tokio::runtime::Handle::try_current() {
                                    let stations = stations.clone();
                                    handle.block_on(async {
                                        let sr = stations.read().await;
                                        if let Some(st) = sr.get(&ca) {
                                            if let Some(point) = st.data_points.get_by_category(ioa, DataCategory::SinglePoint) {
                                                let ca_b = ca.to_le_bytes();
                                                let _ioa_b = ioa.to_le_bytes();
                                                let spont = {
                                                    let mut s = seq.lock().await;
                                                    encode_point_frame_ex(point, 3, &ca_b, &mut *s, None)
                                                };
                                                let _ = stream.write_all(&spont);
                                            }
                                        }
                                    });
                                }
                            }
                            }
                            if let Some(ref lc) = log_collector {
                                let mode = if is_select { "Select" } else { "Execute" };
                                lc.try_add(LogEntry::new(
                                    Direction::Tx, FrameLabel::SingleCommand,
                                    format!("单点命令确认 IOA={} val={} {} CA={}", ioa, value, mode, ca),
                                ));
                            }
                        }
                    }
                    46 => {
                        if data.len() >= 16 {
                            let ioa = u32::from_le_bytes([data[12], data[13], data[14], 0]);
                            let dco = data[15]; let value = dco & 0x03; let is_select = dco & 0x80 != 0;
                            if !is_select {
                                let rt = tokio::runtime::Handle::try_current();
                                if let Ok(handle) = rt {
                                    let stations = stations.clone();
                                    handle.block_on(async {
                                        let mut s = stations.write().await;
                                        if let Some(st) = s.get_mut(&ca) {
                                            if let Some(dp) = st.data_points.get_mut_by_category(ioa, DataCategory::DoublePoint) {
                                                dp.value = DataPointValue::DoublePoint { value };
                                                dp.timestamp = Some(chrono::Utc::now());
                                            }
                                            st.data_points.mark_changed_by_category(ioa, DataCategory::DoublePoint);
                                        }
                                    });
                                }
                            }
                            if ops_snapshot.answer_commands {
                            let ack_cot = if is_select { ops_snapshot.select_ack_cot.as_u8() } else { 7u8 /* ActivationCon: SBO 协议要求 execute ack 恒为 7,终止帧另用 execute_ack_cot */ };
                            let ack = rt.block_on(async { let mut s = seq.lock().await; build_response_frame(&data[..n], ack_cot, &mut s) });
                            let _ = stream.write_all(&ack);
                            if !is_select {
                                let term = rt.block_on(async { let mut s = seq.lock().await; build_response_frame(&data[..n], ops_snapshot.execute_ack_cot.as_u8(), &mut s) });
                                let _ = stream.write_all(&term);
                                if let Ok(handle) = tokio::runtime::Handle::try_current() {
                                    let stations = stations.clone();
                                    handle.block_on(async {
                                        let sr = stations.read().await;
                                        if let Some(st) = sr.get(&ca) {
                                            if let Some(point) = st.data_points.get_by_category(ioa, DataCategory::DoublePoint) {
                                                let ca_b = ca.to_le_bytes();
                                                let _ioa_b = ioa.to_le_bytes();
                                                let spont = {
                                                    let mut s = seq.lock().await;
                                                    encode_point_frame_ex(point, 3, &ca_b, &mut *s, None)
                                                };
                                                let _ = stream.write_all(&spont);
                                            }
                                        }
                                    });
                                }
                            }
                            }
                            if let Some(ref lc) = log_collector {
                                let mode = if is_select { "Select" } else { "Execute" };
                                lc.try_add(LogEntry::new(
                                    Direction::Tx, FrameLabel::DoubleCommand,
                                    format!("双点命令确认 IOA={} val={} {} CA={}", ioa, value, mode, ca),
                                ));
                            }
                        }
                    }
                    47 => {
                        if data.len() >= 16 {
                            let ioa = u32::from_le_bytes([data[12], data[13], data[14], 0]);
                            let rco = data[15]; let step_val = rco & 0x03; let is_select = rco & 0x80 != 0;
                            if !is_select {
                                let rt = tokio::runtime::Handle::try_current();
                                if let Ok(handle) = rt {
                                    let stations = stations.clone();
                                    handle.block_on(async {
                                        let mut s = stations.write().await;
                                        if let Some(st) = s.get_mut(&ca) {
                                            if let Some(dp) = st.data_points.get_mut_by_category(ioa, DataCategory::StepPosition) {
                                                if let DataPointValue::StepPosition { ref mut value, .. } = dp.value {
                                                    match step_val { 1 => { if *value > -64 { *value -= 1; } } 2 => { if *value < 63 { *value += 1; } } _ => {} }
                                                    dp.timestamp = Some(chrono::Utc::now());
                                                }
                                            }
                                            st.data_points.mark_changed_by_category(ioa, DataCategory::StepPosition);
                                        }
                                    });
                                }
                            }
                            if ops_snapshot.answer_commands {
                            let ack_cot = if is_select { ops_snapshot.select_ack_cot.as_u8() } else { 7u8 /* ActivationCon: SBO 协议要求 execute ack 恒为 7,终止帧另用 execute_ack_cot */ };
                            let ack = rt.block_on(async { let mut s = seq.lock().await; build_response_frame(&data[..n], ack_cot, &mut s) });
                            let _ = stream.write_all(&ack);
                            if !is_select {
                                let term = rt.block_on(async { let mut s = seq.lock().await; build_response_frame(&data[..n], ops_snapshot.execute_ack_cot.as_u8(), &mut s) });
                                let _ = stream.write_all(&term);
                                if let Ok(handle) = tokio::runtime::Handle::try_current() {
                                    let stations = stations.clone();
                                    handle.block_on(async {
                                        let sr = stations.read().await;
                                        if let Some(st) = sr.get(&ca) {
                                            if let Some(point) = st.data_points.get_by_category(ioa, DataCategory::StepPosition) {
                                                let ca_b = ca.to_le_bytes();
                                                let _ioa_b = ioa.to_le_bytes();
                                                let spont = {
                                                    let mut s = seq.lock().await;
                                                    encode_point_frame_ex(point, 3, &ca_b, &mut *s, None)
                                                };
                                                let _ = stream.write_all(&spont);
                                            }
                                        }
                                    });
                                }
                            }
                            }
                            if let Some(ref lc) = log_collector {
                                let mode = if is_select { "Select" } else { "Execute" };
                                let dir = match step_val { 1 => "降", 2 => "升", _ => "?" };
                                lc.try_add(LogEntry::new(
                                    Direction::Tx, FrameLabel::StepCommand,
                                    format!("步调节命令确认 IOA={} {} {} CA={}", ioa, dir, mode, ca),
                                ));
                            }
                        }
                    }
                    48 => {
                        if data.len() >= 18 {
                            let ioa = u32::from_le_bytes([data[12], data[13], data[14], 0]);
                            let nva = i16::from_le_bytes([data[15], data[16]]);
                            let qos = data[17]; let is_select = qos & 0x80 != 0;
                            let value = nva as f32 / 32767.0;
                            if !is_select {
                                let rt = tokio::runtime::Handle::try_current();
                                if let Ok(handle) = rt {
                                    let stations = stations.clone();
                                    handle.block_on(async {
                                        let mut s = stations.write().await;
                                        if let Some(st) = s.get_mut(&ca) {
                                            if let Some(dp) = st.data_points.get_mut_by_category(ioa, DataCategory::NormalizedMeasured) {
                                                dp.value = DataPointValue::Normalized { value };
                                                dp.timestamp = Some(chrono::Utc::now());
                                            }
                                            st.data_points.mark_changed_by_category(ioa, DataCategory::NormalizedMeasured);
                                        }
                                    });
                                }
                            }
                            if ops_snapshot.answer_commands {
                            let ack_cot = if is_select { ops_snapshot.select_ack_cot.as_u8() } else { 7u8 /* ActivationCon: SBO 协议要求 execute ack 恒为 7,终止帧另用 execute_ack_cot */ };
                            let ack = rt.block_on(async { let mut s = seq.lock().await; build_response_frame(&data[..n], ack_cot, &mut s) });
                            let _ = stream.write_all(&ack);
                            if !is_select {
                                let term = rt.block_on(async { let mut s = seq.lock().await; build_response_frame(&data[..n], ops_snapshot.execute_ack_cot.as_u8(), &mut s) });
                                let _ = stream.write_all(&term);
                                if let Ok(handle) = tokio::runtime::Handle::try_current() {
                                    let stations = stations.clone();
                                    handle.block_on(async {
                                        let sr = stations.read().await;
                                        if let Some(st) = sr.get(&ca) {
                                            if let Some(point) = st.data_points.get_by_category(ioa, DataCategory::NormalizedMeasured) {
                                                let ca_b = ca.to_le_bytes();
                                                let _ioa_b = ioa.to_le_bytes();
                                                let spont = {
                                                    let mut s = seq.lock().await;
                                                    encode_point_frame_ex(point, 3, &ca_b, &mut *s, None)
                                                };
                                                let _ = stream.write_all(&spont);
                                            }
                                        }
                                    });
                                }
                            }
                            }
                            if let Some(ref lc) = log_collector {
                                let mode = if is_select { "Select" } else { "Execute" };
                                lc.try_add(LogEntry::new(
                                    Direction::Tx, FrameLabel::SetpointNormalized,
                                    format!("归一化设定值确认 IOA={} val={:.4} {} CA={}", ioa, value, mode, ca),
                                ));
                            }
                        }
                    }
                    49 => {
                        if data.len() >= 18 {
                            let ioa = u32::from_le_bytes([data[12], data[13], data[14], 0]);
                            let sva = i16::from_le_bytes([data[15], data[16]]);
                            let qos = data[17]; let is_select = qos & 0x80 != 0;
                            if !is_select {
                                let rt = tokio::runtime::Handle::try_current();
                                if let Ok(handle) = rt {
                                    let stations = stations.clone();
                                    handle.block_on(async {
                                        let mut s = stations.write().await;
                                        if let Some(st) = s.get_mut(&ca) {
                                            if let Some(dp) = st.data_points.get_mut_by_category(ioa, DataCategory::ScaledMeasured) {
                                                dp.value = DataPointValue::Scaled { value: sva };
                                                dp.timestamp = Some(chrono::Utc::now());
                                            }
                                            st.data_points.mark_changed_by_category(ioa, DataCategory::ScaledMeasured);
                                        }
                                    });
                                }
                            }
                            if ops_snapshot.answer_commands {
                            let ack_cot = if is_select { ops_snapshot.select_ack_cot.as_u8() } else { 7u8 /* ActivationCon: SBO 协议要求 execute ack 恒为 7,终止帧另用 execute_ack_cot */ };
                            let ack = rt.block_on(async { let mut s = seq.lock().await; build_response_frame(&data[..n], ack_cot, &mut s) });
                            let _ = stream.write_all(&ack);
                            if !is_select {
                                let term = rt.block_on(async { let mut s = seq.lock().await; build_response_frame(&data[..n], ops_snapshot.execute_ack_cot.as_u8(), &mut s) });
                                let _ = stream.write_all(&term);
                                if let Ok(handle) = tokio::runtime::Handle::try_current() {
                                    let stations = stations.clone();
                                    handle.block_on(async {
                                        let sr = stations.read().await;
                                        if let Some(st) = sr.get(&ca) {
                                            if let Some(point) = st.data_points.get_by_category(ioa, DataCategory::ScaledMeasured) {
                                                let ca_b = ca.to_le_bytes();
                                                let _ioa_b = ioa.to_le_bytes();
                                                let spont = {
                                                    let mut s = seq.lock().await;
                                                    encode_point_frame_ex(point, 3, &ca_b, &mut *s, None)
                                                };
                                                let _ = stream.write_all(&spont);
                                            }
                                        }
                                    });
                                }
                            }
                            }
                            if let Some(ref lc) = log_collector {
                                let mode = if is_select { "Select" } else { "Execute" };
                                lc.try_add(LogEntry::new(
                                    Direction::Tx, FrameLabel::SetpointScaled,
                                    format!("标度化设定值确认 IOA={} val={} {} CA={}", ioa, sva, mode, ca),
                                ));
                            }
                        }
                    }
                    50 => {
                        if data.len() >= 20 {
                            let ioa = u32::from_le_bytes([data[12], data[13], data[14], 0]);
                            let value = f32::from_le_bytes([data[15], data[16], data[17], data[18]]);
                            let qos = data[19]; let is_select = qos & 0x80 != 0;
                            if !is_select {
                                let rt = tokio::runtime::Handle::try_current();
                                if let Ok(handle) = rt {
                                    let stations = stations.clone();
                                    handle.block_on(async {
                                        let mut s = stations.write().await;
                                        if let Some(st) = s.get_mut(&ca) {
                                            if let Some(dp) = st.data_points.get_mut_by_category(ioa, DataCategory::FloatMeasured) {
                                                dp.value = DataPointValue::ShortFloat { value };
                                                dp.timestamp = Some(chrono::Utc::now());
                                            }
                                            st.data_points.mark_changed_by_category(ioa, DataCategory::FloatMeasured);
                                        }
                                    });
                                }
                            }
                            if ops_snapshot.answer_commands {
                            let ack_cot = if is_select { ops_snapshot.select_ack_cot.as_u8() } else { 7u8 /* ActivationCon: SBO 协议要求 execute ack 恒为 7,终止帧另用 execute_ack_cot */ };
                            let ack = rt.block_on(async { let mut s = seq.lock().await; build_response_frame(&data[..n], ack_cot, &mut s) });
                            let _ = stream.write_all(&ack);
                            if !is_select {
                                let term = rt.block_on(async { let mut s = seq.lock().await; build_response_frame(&data[..n], ops_snapshot.execute_ack_cot.as_u8(), &mut s) });
                                let _ = stream.write_all(&term);
                                if let Ok(handle) = tokio::runtime::Handle::try_current() {
                                    let stations = stations.clone();
                                    handle.block_on(async {
                                        let sr = stations.read().await;
                                        if let Some(st) = sr.get(&ca) {
                                            if let Some(point) = st.data_points.get_by_category(ioa, DataCategory::FloatMeasured) {
                                                let ca_b = ca.to_le_bytes();
                                                let _ioa_b = ioa.to_le_bytes();
                                                let spont = {
                                                    let mut s = seq.lock().await;
                                                    encode_point_frame_ex(point, 3, &ca_b, &mut *s, None)
                                                };
                                                let _ = stream.write_all(&spont);
                                            }
                                        }
                                    });
                                }
                            }
                            }
                            if let Some(ref lc) = log_collector {
                                let mode = if is_select { "Select" } else { "Execute" };
                                lc.try_add(LogEntry::new(
                                    Direction::Tx, FrameLabel::SetpointFloat,
                                    format!("浮点设定值确认 IOA={} val={:.3} {} CA={}", ioa, value, mode, ca),
                                ));
                            }
                        }
                    }
                    _ => {
                        if let Some(ref lc) = log_collector {
                            lc.try_add(LogEntry::new(
                                Direction::Rx, FrameLabel::IFrame(format!("Type{}", asdu_type)),
                                format!("未知 ASDU 类型={} CA={} COT={}", asdu_type, ca, cause),
                            ));
                        }
                    }
                }
            }
        }
    }
    // Clean up the connection entry when the client disconnects.
    rt.block_on(async { connections.write().await.remove(&peer_addr); });
}

async fn send_gi_response_blocking(
    stream: &mut native_tls::TlsStream<TcpStream>,
    station: &Station,
    seq: &SharedSeq,
    ops: &RemoteOperationConfig,
) {
    use std::io::Write;
    let ca_bytes = station.common_address.to_le_bytes();
    let mut batch: Vec<u8> = Vec::new();
    {
        let mut s = seq.lock().await;
        for point in station.data_points.all_sorted() {
            // GI 排除累积量 (M_IT);累积量仅由计数量召唤 (C_CI_NA_1) 上送。
            if matches!(point.value, DataPointValue::IntegratedTotal { .. }) { continue; }
            batch.extend_from_slice(&encode_point_frame_ex(point, 20, &ca_bytes, &mut s, None));
            if ops.gi_include_timestamped
                && should_derive_tb(&station.data_points, point.asdu_type, point.ioa)
            {
                batch.extend_from_slice(&encode_point_frame_ex(point, 20, &ca_bytes, &mut s, Some(true)));
            }
        }
    }
    let _ = stream.write_all(&batch);
}

// ---------------------------------------------------------------------------
// I-Frame Builder
// ---------------------------------------------------------------------------

fn build_i_frame(
    asdu_type: u8, cause: u8, ca: &[u8], ioa: &[u8], value_bytes: &[u8],
    seq: &mut SeqState,
) -> Vec<u8> {
    let asdu_len = 6 + ioa.len() + value_bytes.len();
    let total_len = 4 + asdu_len;
    let mut frame = Vec::with_capacity(2 + total_len);
    frame.push(0x68);
    frame.push(total_len as u8);
    // 4 APCI control bytes for I-frame:
    // Bytes 2-3: N(S) << 1, 16-bit little-endian (bit 0 = 0 indicates I-frame)
    // Bytes 4-5: N(R) << 1, 16-bit little-endian
    frame.push((seq.ssn & 0xFF) as u8);
    frame.push(((seq.ssn >> 8) & 0xFF) as u8);
    frame.push((seq.rsn & 0xFF) as u8);
    frame.push(((seq.rsn >> 8) & 0xFF) as u8);
    seq.ssn = seq.ssn.wrapping_add(2);
    // N(R) is not auto-incremented per sent frame; it tracks the peer's N(S),
    // updated by observe_recv_iframe on receipt.
    frame.extend_from_slice(&[asdu_type, 0x01, cause, 0x00]);
    frame.extend_from_slice(&ca[..2]);
    frame.extend_from_slice(ioa);
    frame.extend_from_slice(value_bytes);
    frame
}

/// 把 `DataPointValue` 编码为 IEC 60870-5-101 中 NA 类型(无时标)的值字节,
/// 并按类型把品质 `q` 写入对应的品质字节:
/// - SP/DP:品质占 SIQ/DIQ 高 4 位(低位是 SPI/DPI 值,无 OV)
/// - 测量类(Normalized/Scaled/ShortFloat):完整 QDS(含 OV)
/// - Step/Bitstring:QDS 高 4 位(标准 OV 仅对测量类生效,此处不写 OV)
/// - 累计量:BCR 描述字节的 IV 位(0x80),与进位/序号共存
fn encode_na_value(value: &DataPointValue, q: &QualityFlags) -> (u8, Vec<u8>) {
    match value {
        DataPointValue::SinglePoint { value } => {
            let siq = (if *value { 0x01 } else { 0x00 }) | q.upper_bits();
            (1, vec![siq])
        }
        DataPointValue::DoublePoint { value } => {
            let diq = (*value & 0x03) | q.upper_bits();
            (3, vec![diq])
        }
        DataPointValue::StepPosition { value, transient } => {
            let vti = ((*value as u8) & 0x7F) | (if *transient { 0x80 } else { 0 });
            (5, vec![vti, q.upper_bits()])
        }
        DataPointValue::Bitstring { value } => {
            let b = value.to_le_bytes();
            (7, vec![b[0], b[1], b[2], b[3], q.upper_bits()])
        }
        DataPointValue::Normalized { value } => {
            // round(非截断)与解码 `nva / 32767` 对称,保证整数原样往返 (off-by-one 修正)。
            let nva = (*value * 32767.0).round() as i16;
            let b = nva.to_le_bytes();
            (9, vec![b[0], b[1], q.qds_byte()])
        }
        DataPointValue::Scaled { value } => {
            let b = value.to_le_bytes();
            (11, vec![b[0], b[1], q.qds_byte()])
        }
        DataPointValue::ShortFloat { value } => {
            let b = value.to_le_bytes();
            (13, vec![b[0], b[1], b[2], b[3], q.qds_byte()])
        }
        DataPointValue::IntegratedTotal { value, carry, sequence } => {
            let b = value.to_le_bytes();
            let mut bcr = *sequence & 0x1F;
            if *carry { bcr |= 0x20; }
            if q.iv { bcr |= 0x80; }
            (15, vec![b[0], b[1], b[2], b[3], bcr])
        }
    }
}

/// 旧 NA-only 路径,保留作为备用入口(目前没人调用,已被 `encode_point_frame_ex` 取代)。
#[allow(dead_code)]
fn encode_point_frame(
    value: &DataPointValue, cot: u8, ca: &[u8], ioa: &[u8],
    seq: &mut SeqState,
) -> Vec<u8> {
    let (type_id, value_bytes) = encode_na_value(value, &QualityFlags::good());
    build_i_frame(type_id, cot, ca, ioa, &value_bytes, seq)
}

/// 编码单个数据点为 I-frame,可选输出带 CP56Time2a 时标的 TB 版本。
///
/// `force_timestamped`:
/// - `Some(true)`  → 强制输出 TB 类型(若没有 TB 对应类型则回退到 NA)
/// - `Some(false)` → 强制输出 NA 类型
/// - `None`        → 按 `point.asdu_type` 自身决定(本身是 TB 的发 TB)
fn encode_point_frame_ex(
    point: &DataPoint, cot: u8, ca: &[u8],
    seq: &mut SeqState, force_timestamped: Option<bool>,
) -> Vec<u8> {
    // M_ME_ND_1 (TypeID 21): 归一化测量值,2 字节裸 NVA,无 QDS、无时标。
    // 它与 M_ME_NA_1 共用 `DataPointValue::Normalized`,只能靠 asdu_type 区分;
    // 前置拦截以免走 encode_na_value(那会按值返回 type 9 并附 QDS)。无 TB 变体,
    // force_timestamped 对其无意义。
    if point.asdu_type == AsduTypeId::MMeNd1 {
        let nva = match point.value {
            DataPointValue::Normalized { value } => (value * 32767.0).round() as i16,
            _ => 0,
        };
        let b = nva.to_le_bytes();
        let ioa_bytes = point.ioa.to_le_bytes();
        return build_i_frame(21, cot, ca, &ioa_bytes[..3], &[b[0], b[1]], seq);
    }
    let (na_type, mut value_bytes) = encode_na_value(&point.value, &point.quality);
    let want_tb = match force_timestamped {
        Some(b) => b,
        None => point.asdu_type.is_timestamped(),
    };
    let ioa_bytes = point.ioa.to_le_bytes();
    if want_tb {
        let na_id = AsduTypeId::from_u8(na_type).unwrap_or(point.asdu_type);
        if let Some(tb) = na_id.timestamped_variant() {
            let ts = point.timestamp.unwrap_or_else(chrono::Utc::now);
            let ts_bytes = crate::asdu_encode::encode_cp56time2a(ts, point.quality.iv);
            value_bytes.extend_from_slice(&ts_bytes);
            return build_i_frame(tb as u8, cot, ca, &ioa_bytes[..3], &value_bytes, seq);
        }
    }
    build_i_frame(na_type, cot, ca, &ioa_bytes[..3], &value_bytes, seq)
}

/// 把一组**连续 IOA 且同 NA 类型**的点合并到单个 ASDU 帧 (VSQ.SQ=1)。
/// 返回 None 表示无法打包,调用方应回退到逐点路径。
fn encode_points_grouped(
    points: &[&DataPoint], cot: u8, ca: &[u8],
    seq: &mut SeqState, timestamped: bool,
) -> Option<Vec<u8>> {
    if points.is_empty() { return None; }
    // M_ME_ND_1 无法用 encode_na_value 表达(它会按 Normalized 值返回 type 9 + QDS),
    // 故 ND 段不走 SQ=1 打包,返回 None 让调用方逐点回退到 encode_point_frame_ex。
    if points[0].asdu_type == AsduTypeId::MMeNd1 { return None; }
    // 品质不影响类型判定,此处仅取 type_id;逐点品质在下方值段循环中各自写入。
    let (first_type, _) = encode_na_value(&points[0].value, &points[0].quality);
    for w in points.windows(2) {
        if w[1].ioa != w[0].ioa + 1 { return None; }
        let (t, _) = encode_na_value(&w[1].value, &w[1].quality);
        if t != first_type { return None; }
    }
    let final_type = if timestamped {
        let na = AsduTypeId::from_u8(first_type)?;
        na.timestamped_variant()? as u8
    } else {
        first_type
    };
    let n = points.len() as u8;
    if n > 0x7F { return None; }
    let mut value_section: Vec<u8> = Vec::new();
    for p in points {
        let (_, bytes) = encode_na_value(&p.value, &p.quality);
        value_section.extend_from_slice(&bytes);
        if timestamped {
            let ts = p.timestamp.unwrap_or_else(chrono::Utc::now);
            let ts_bytes = crate::asdu_encode::encode_cp56time2a(ts, p.quality.iv);
            value_section.extend_from_slice(&ts_bytes);
        }
    }
    let ioa_bytes = points[0].ioa.to_le_bytes();
    let asdu_len = 6 + 3 + value_section.len();
    let total_len = 4 + asdu_len;
    if total_len > 253 { return None; }
    let mut frame = Vec::with_capacity(2 + total_len);
    frame.push(0x68);
    frame.push(total_len as u8);
    frame.push((seq.ssn & 0xFF) as u8);
    frame.push(((seq.ssn >> 8) & 0xFF) as u8);
    frame.push((seq.rsn & 0xFF) as u8);
    frame.push(((seq.rsn >> 8) & 0xFF) as u8);
    seq.ssn = seq.ssn.wrapping_add(2);
    frame.push(final_type);
    frame.push(0x80 | (n & 0x7F)); // VSQ.SQ=1
    frame.push(cot);
    frame.push(0x00);
    frame.extend_from_slice(&ca[..2]);
    frame.extend_from_slice(&ioa_bytes[..3]);
    frame.extend_from_slice(&value_section);
    Some(frame)
}

/// k 窗口流控：未确认 I 帧数 (in_flight) 达到 k 时阻塞等待对方 ACK。
/// `k = 0` 时直接放行（兼容关闭流控的配置）。
/// 等待粒度采用极短 sleep (200μs)：太长会拖慢吞吐，太短会忙轮询。
async fn wait_window(seq: &SharedSeq, k: u16) {
    if k == 0 { return; }
    loop {
        let in_flight = {
            let s = seq.lock().await;
            s.ssn.wrapping_sub(s.ack_ssn) / 2
        };
        if in_flight < k { return; }
        tokio::time::sleep(std::time::Duration::from_micros(200)).await;
    }
}

/// 把"同 NA 类型 + IOA 连续"的一段点切成 ≤253B 的 SQ=1 大帧，
/// 每帧前做 k 窗口阻塞；失败（含 total_len 超限）回退到逐点 `encode_point_frame_ex`。
/// `seg` 必须已经满足类型相同 + IOA 连续，否则会回退到逐点路径。
async fn encode_segment_and_enqueue(
    seg: &[DataPoint], cot: u8, ca: &[u8; 2],
    seq: &SharedSeq, queue: &SharedQueue, k: u16, timestamped: bool,
) -> usize {
    let mut i = 0;
    let mut frames_emitted = 0usize;
    while i < seg.len() {
        let mut chunk_size = (seg.len() - i).min(120);
        let mut packed = false;
        while chunk_size >= 2 {
            let refs: Vec<&DataPoint> = seg[i..i + chunk_size].iter().collect();
            wait_window(seq, k).await;
            let frame_opt = {
                let mut s = seq.lock().await;
                encode_points_grouped(&refs, cot, &ca[..], &mut *s, timestamped)
            };
            if let Some(frame) = frame_opt {
                queue.lock().await.extend_from_slice(&frame);
                frames_emitted += 1;
                i += chunk_size;
                packed = true;
                break;
            }
            chunk_size /= 2;
        }
        if !packed {
            // chunk_size < 2 仍未成功，或单点无法 grouped 表达，逐点回退。
            wait_window(seq, k).await;
            let frame = {
                let mut s = seq.lock().await;
                encode_point_frame_ex(
                    &seg[i], cot, &ca[..], &mut *s,
                    if timestamped { Some(true) } else { Some(false) },
                )
            };
            queue.lock().await.extend_from_slice(&frame);
            frames_emitted += 1;
            i += 1;
        }
        if frames_emitted % 16 == 0 {
            tokio::task::yield_now().await;
        }
    }
    frames_emitted
}

/// GI/CI 的独立 task 执行体：按 (point.asdu_type, 连续 IOA) 切段，
/// 每段调 `encode_segment_and_enqueue`；TB 类型段自然带时标，
/// NA 类型段若开启 `include_timestamped` 且存在 TB 变体则额外再发一份时标副本。
/// 调用方传入的 `points` 不要求特定顺序；本函数会按 (asdu_type, ioa) 二次排序，
/// 把同类型 + 连续 IOA 的点位重新聚到一起，最大化 SQ=1 打包收益。
async fn run_interrogation(
    mut points: Vec<DataPoint>,
    ca_bytes: [u8; 2],
    cot_data: u8,
    act_term_frame_template: Vec<u8>,
    include_timestamped: bool,
    queue: SharedQueue,
    seq: SharedSeq,
    k: u16,
    log_collector: Option<Arc<LogCollector>>,
    label: FrameLabel,
    ca_label: u16,
) {
    // 按 (asdu_type 数值, ioa) 排序。同类型连续段可被 encode_points_grouped 合并为
    // 一个 VSQ.SQ=1 ASDU；不同 type 之间的边界天然断开。
    points.sort_by_key(|p| (p.asdu_type as u8, p.ioa));
    // R1:快照中已显式存在的 TB 点 (ioa, tb_type)——这些 IOA 不再派生,避免重复上送。
    let explicit_tb: std::collections::HashSet<(u32, AsduTypeId)> = points
        .iter()
        .filter(|p| p.asdu_type.is_timestamped())
        .map(|p| (p.ioa, p.asdu_type))
        .collect();
    let mut i = 0;
    while i < points.len() {
        let kind0 = points[i].asdu_type;
        let ioa0 = points[i].ioa;
        let mut j = i + 1;
        while j < points.len() {
            if points[j].asdu_type != kind0 || points[j].ioa != ioa0 + (j - i) as u32 { break; }
            j += 1;
        }
        let seg = &points[i..j];
        let kind_is_timestamped = kind0.is_timestamped();
        encode_segment_and_enqueue(
            seg, cot_data, &ca_bytes, &seq, &queue, k, kind_is_timestamped,
        ).await;
        // include_timestamped 时为不带时标段派生 TB 帧;R1 让已有显式 TB 的 IOA 不再派生。
        if let Some(tb_type) = kind0
            .timestamped_variant()
            .filter(|_| include_timestamped && !kind_is_timestamped)
        {
            let any_suppressed = !explicit_tb.is_empty()
                && seg.iter().any(|p| explicit_tb.contains(&(p.ioa, tb_type)));
            if !any_suppressed {
                // 本段无 IOA 被压制,整段派生以保持 SQ=1 分组。
                encode_segment_and_enqueue(seg, cot_data, &ca_bytes, &seq, &queue, k, true).await;
            } else {
                for p in seg {
                    if explicit_tb.contains(&(p.ioa, tb_type)) { continue; }
                    encode_segment_and_enqueue(
                        std::slice::from_ref(p), cot_data, &ca_bytes, &seq, &queue, k, true,
                    ).await;
                }
            }
        }
        i = j;
    }
    // ACT_TERM：复用收到的 ACT 帧模板，仅改 N(S)/N(R)/COT。
    wait_window(&seq, k).await;
    let term = {
        let mut s = seq.lock().await;
        build_response_frame(&act_term_frame_template, 10, &mut *s)
    };
    queue.lock().await.extend_from_slice(&term);
    if let Some(lc) = log_collector {
        let kind = match cot_data {
            20 => "GI",
            37 => "累计量召唤",
            _ => "Interrogation",
        };
        lc.try_add(LogEntry::new(
            Direction::Tx, label,
            format!("{} 激活终止 CA={}", kind, ca_label),
        ));
    }
}

/// 翻转一个数据点的值,用于固定变位的周期性扰动。
fn flip_value(value: &DataPointValue) -> DataPointValue {
    match value {
        DataPointValue::SinglePoint { value } => DataPointValue::SinglePoint { value: !value },
        DataPointValue::DoublePoint { value } => {
            DataPointValue::DoublePoint { value: if *value == 1 { 2 } else { 1 } }
        }
        DataPointValue::StepPosition { value, transient } => {
            let next = match *value { -1 => 0, 0 => 1, _ => -1 };
            DataPointValue::StepPosition { value: next, transient: *transient }
        }
        DataPointValue::Bitstring { value } => DataPointValue::Bitstring { value: !value },
        // 模拟量 (ME_NA/NB/NC) 用取反实现两态振荡;但值为 0 时 -0.0 仍是零,
        // 用户会看到「值不变」,故零值翻到一个可见的非零量。
        DataPointValue::Normalized { value } => {
            DataPointValue::Normalized { value: if *value == 0.0 { 1.0 } else { -value } }
        }
        DataPointValue::Scaled { value } => {
            DataPointValue::Scaled { value: if *value == 0 { 100 } else { -*value } }
        }
        DataPointValue::ShortFloat { value } => {
            DataPointValue::ShortFloat { value: if *value == 0.0 { 1.0 } else { -value } }
        }
        DataPointValue::IntegratedTotal { value, carry, sequence } => DataPointValue::IntegratedTotal {
            value: value + 1,
            carry: *carry,
            sequence: sequence.wrapping_add(1) & 0x1F,
        },
    }
}

/// 周期变位的变位方式。Flip 为两态翻转(离散量翻转、模拟量取反);
/// Increment/Decrement 为按步长在 [min,max] 三角波来回(仅模拟量/累计量,
/// 离散量回退翻转)。
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum MutationMode {
    #[default]
    Flip,
    Increment,
    Decrement,
}

impl MutationMode {
    /// 任务启动时的初始方向:递减向下 (-1),其余向上 (+1)。Flip 不使用方向。
    fn initial_dir(self) -> f64 {
        match self {
            MutationMode::Decrement => -1.0,
            _ => 1.0,
        }
    }
}

/// 周期变位参数。step/min/max 仅 Increment/Decrement 使用(统一以 f64 传递,
/// 应用到具体类型时按需 round/clamp);Flip 时忽略。
#[derive(Debug, Clone, Copy, Default)]
pub struct MutationParams {
    pub mode: MutationMode,
    pub step: f64,
    pub min: f64,
    pub max: f64,
}

/// 按变位模式计算下一个值与方向。`dir` 为递增/递减的当前方向(+1 向上 / -1 向下),
/// 三角波到达 min/max 后掉头。返回 (新值, 新方向);Flip 模式方向原样返回。
fn apply_mutation(value: &DataPointValue, params: &MutationParams, dir: f64) -> (DataPointValue, f64) {
    match params.mode {
        MutationMode::Flip => (flip_value(value), dir),
        MutationMode::Increment | MutationMode::Decrement => step_value(value, params, dir),
    }
}

/// 三角波步进。模拟量/累计量按 step 在量程内来回;离散量无递增语义,回退翻转。
/// 归一化/标度化/累计量的上下限会再钳到各自协议量程,避免编码溢出或卡死。
fn step_value(value: &DataPointValue, params: &MutationParams, dir: f64) -> (DataPointValue, f64) {
    let (cur, lo, hi): (f64, f64, f64) = match value {
        DataPointValue::ShortFloat { value } => (*value as f64, params.min, params.max),
        DataPointValue::Normalized { value } => {
            (*value as f64, params.min.max(-1.0), params.max.min(1.0))
        }
        DataPointValue::Scaled { value } => {
            (*value as f64, params.min.max(i16::MIN as f64), params.max.min(i16::MAX as f64))
        }
        DataPointValue::IntegratedTotal { value, .. } => {
            (*value as f64, params.min.max(i32::MIN as f64), params.max.min(i32::MAX as f64))
        }
        // 离散量(单点/双点/步位/位串)无递增语义,回退翻转,方向不变。
        _ => return (flip_value(value), dir),
    };
    let (lo, hi) = (lo.min(hi), lo.max(hi)); // 容错:用户把上下限填反
    let step = params.step.abs();
    let mut next = cur + dir * step;
    let mut new_dir = dir;
    if next >= hi {
        next = hi;
        new_dir = -1.0;
    } else if next <= lo {
        next = lo;
        new_dir = 1.0;
    }
    let new_value = match value {
        DataPointValue::ShortFloat { .. } => DataPointValue::ShortFloat { value: next as f32 },
        DataPointValue::Normalized { .. } => DataPointValue::Normalized { value: next as f32 },
        // next 已在 [lo,hi] ⊆ i16/i32 量程内,round 后转换不会溢出。
        DataPointValue::Scaled { .. } => DataPointValue::Scaled { value: next.round() as i16 },
        DataPointValue::IntegratedTotal { carry, sequence, .. } => DataPointValue::IntegratedTotal {
            value: next.round() as i32,
            carry: *carry,
            sequence: *sequence,
        },
        _ => unreachable!(),
    };
    (new_value, new_dir)
}

/// 模块级 `queue_spontaneous` 实现,供 `SlaveServer.queue_spontaneous` 和
/// `start_point_mutation` 后台任务共用。
async fn do_queue_spontaneous(
    stations: &SharedStations,
    connections: &SharedConnections,
    remote_ops: &SharedRemoteOps,
    log_collector: &Option<Arc<LogCollector>>,
    common_address: u16,
    changed: &[(u32, AsduTypeId)],
) {
    if changed.is_empty() { return; }
    let ops = remote_ops.read().await.clone();
    let stations_guard = stations.read().await;
    let station = match stations_guard.get(&common_address) {
        Some(s) => s,
        None => return,
    };
    let ca_bytes = station.common_address.to_le_bytes();
    let mut conns = connections.write().await;
    let mut total_sent = 0usize;
    for (_addr, conn) in conns.iter_mut() {
        if !conn.started.load(std::sync::atomic::Ordering::SeqCst) { continue; }
        let mut batch = Vec::new();
        {
            let mut s = conn.seq.lock().await;
            let mut points: Vec<&DataPoint> = Vec::new();
            for &(ioa, asdu_type) in changed {
                if let Some(p) = station.data_points.get(ioa, asdu_type) {
                    points.push(p);
                }
            }
            if ops.auto_packing && !points.is_empty() {
                points.sort_by_key(|p| (p.asdu_type as u8, p.ioa));
                let mut start = 0usize;
                while start < points.len() {
                    let mut end = start + 1;
                    while end < points.len()
                        && points[end].asdu_type == points[start].asdu_type
                        && points[end].ioa == points[end - 1].ioa + 1
                    { end += 1; }
                    let segment = &points[start..end];
                    let want_tb_mode = ops.upload_mode_timestamped == UploadMode::Continuous
                        && segment[0].asdu_type.is_timestamped();
                    let want_na_mode = ops.upload_mode_untimestamped == UploadMode::Continuous
                        && !segment[0].asdu_type.is_timestamped();
                    if want_tb_mode || want_na_mode {
                        if let Some(frame) = encode_points_grouped(segment, 3, &ca_bytes, &mut *s, segment[0].asdu_type.is_timestamped()) {
                            batch.extend(frame);
                            for p in segment { conn.last_sent.insert(p.ioa, p.value.display()); }
                            start = end;
                            continue;
                        }
                    }
                    for p in segment {
                        batch.extend(encode_point_frame_ex(p, 3, &ca_bytes, &mut *s, None));
                        if ops.sync_tb_by_category.enabled_for(p.asdu_type.category())
                            && should_derive_tb(&station.data_points, p.asdu_type, p.ioa)
                        {
                            batch.extend(encode_point_frame_ex(p, 3, &ca_bytes, &mut *s, Some(true)));
                        }
                        conn.last_sent.insert(p.ioa, p.value.display());
                    }
                    start = end;
                }
            } else {
                for &(ioa, asdu_type) in changed {
                    let point = match station.data_points.get(ioa, asdu_type) {
                        Some(p) => p,
                        None => continue,
                    };
                    batch.extend(encode_point_frame_ex(point, 3, &ca_bytes, &mut *s, None));
                    if ops.sync_tb_by_category.enabled_for(asdu_type.category())
                        && should_derive_tb(&station.data_points, asdu_type, ioa)
                    {
                        batch.extend(encode_point_frame_ex(point, 3, &ca_bytes, &mut *s, Some(true)));
                    }
                    conn.last_sent.insert(ioa, point.value.display());
                }
            }
        }
        if !batch.is_empty() {
            total_sent += 1;
            conn.queue.lock().await.extend(batch);
        }
    }
    if total_sent > 0 {
        if let Some(ref lc) = log_collector {
            let detail = if changed.len() == 1 {
                let (ioa, asdu_type) = changed[0];
                format!("突发上送 (COT=3) IOA={} {} CA={} → {} 个客户端", ioa, asdu_type.name(), common_address, total_sent)
            } else {
                format!("突发上送 (COT=3) {} 个 IOA CA={} → {} 个客户端", changed.len(), common_address, total_sent)
            };
            let label = changed
                .first()
                .map(|(_, t)| FrameLabel::IFrame(t.name().to_string()))
                .unwrap_or_else(|| FrameLabel::IFrame(String::new()));
            lc.try_add(LogEntry::new(Direction::Tx, label, detail));
        }
    }
}

// ---------------------------------------------------------------------------
// Error Types
// ---------------------------------------------------------------------------

#[derive(Debug, thiserror::Error)]
pub enum SlaveError {
    #[error("IOA {0} already exists")] DuplicateIoa(u32),
    #[error("IOA {0} not found")] IoaNotFound(u32),
    #[error("station CA={0} already exists")] DuplicateStation(u16),
    #[error("station CA={0} not found")] StationNotFound(u16),
    #[error("server is already running")] AlreadyRunning,
    #[error("server is not running")] NotRunning,
    #[error("bind error: {0}")] BindError(String),
    #[error("TLS error: {0}")] TlsError(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_station_creation() {
        let s = Station::new(1, "测试站");
        assert_eq!(s.common_address, 1);
    }

    #[test]
    fn test_station_with_default_points() {
        let s = Station::with_default_points(1, "站1", 10);
        // 8 个 NA 类型 × 10 IOA = 80；默认不预建 TB（TB 改为派生格式）
        assert_eq!(s.data_points.len(), 80);
        // IOA=1 上挂着 8 种 NA 类型
        assert!(s.data_points.get(1, AsduTypeId::MSpNa1).is_some());
        assert!(s.data_points.get(1, AsduTypeId::MDpNa1).is_some());
        assert!(s.data_points.get(1, AsduTypeId::MStNa1).is_some());
        assert!(s.data_points.get(1, AsduTypeId::MBoNa1).is_some());
        assert!(s.data_points.get(1, AsduTypeId::MMeNc1).is_some());
        assert!(s.data_points.get(1, AsduTypeId::MItNa1).is_some());
        // 默认不应预建任何 TB 点
        assert!(s.data_points.get(1, AsduTypeId::MSpTb1).is_none());
        assert!(s.data_points.get(1, AsduTypeId::MMeTf1).is_none());
        assert!(s.data_points.get(1, AsduTypeId::MItTb1).is_none());
        // 边界 IOA=10 的 NA 点存在
        assert!(s.data_points.get(10, AsduTypeId::MSpNa1).is_some());
        assert!(s.data_points.get(10, AsduTypeId::MItNa1).is_some());
        // IOA=11 不应该存在（所有类型只到 10）
        assert!(s.data_points.get(11, AsduTypeId::MSpNa1).is_none());
    }

    #[test]
    fn test_flip_value_zero_analog_becomes_nonzero() {
        // 周期变位:浮点 (ME_NC)/归一化 (ME_NA)/标度化 (ME_NB) 值为 0 时,
        // 旧实现 `-value` 得到 -0.0,显示成 "-0.000000" 并在 0/-0 之间振荡,
        // 用户看到「值不变」。flip_value 必须让零值产生可见的非零变化。
        let cases = [
            DataPointValue::ShortFloat { value: 0.0 },
            DataPointValue::Normalized { value: 0.0 },
            DataPointValue::Scaled { value: 0 },
        ];
        for v in cases {
            let mag = match flip_value(&v) {
                DataPointValue::ShortFloat { value } => value.abs() as f64,
                DataPointValue::Normalized { value } => value.abs() as f64,
                DataPointValue::Scaled { value } => value.unsigned_abs() as f64,
                other => panic!("flip_value 改变了类型: {:?}", other),
            };
            assert!(mag > 0.0, "flip_value 对零值 {:?} 仍是零,用户看到值不变", v);
        }
        // 非零值仍取反,保持两态振荡语义。
        assert!(matches!(
            flip_value(&DataPointValue::ShortFloat { value: 5.0 }),
            DataPointValue::ShortFloat { value } if value == -5.0
        ));
        assert!(matches!(
            flip_value(&DataPointValue::Scaled { value: 100 }),
            DataPointValue::Scaled { value: -100 }
        ));
    }

    #[test]
    fn test_apply_mutation_increment_decrement() {
        use DataPointValue as V;
        let p = |mode, step, min, max| MutationParams { mode, step, min, max };

        // 递增:浮点 +step
        let (v, dir) = apply_mutation(&V::ShortFloat { value: 0.0 }, &p(MutationMode::Increment, 1.0, -100.0, 100.0), 1.0);
        assert!(matches!(v, V::ShortFloat { value } if value == 1.0));
        assert_eq!(dir, 1.0);

        // 触顶钳制并掉头(三角波)
        let (v, dir) = apply_mutation(&V::ShortFloat { value: 99.5 }, &p(MutationMode::Increment, 1.0, -100.0, 100.0), 1.0);
        assert!(matches!(v, V::ShortFloat { value } if value == 100.0));
        assert_eq!(dir, -1.0);

        // 触底钳制并掉头
        let (v, dir) = apply_mutation(&V::ShortFloat { value: -99.5 }, &p(MutationMode::Decrement, 1.0, -100.0, 100.0), -1.0);
        assert!(matches!(v, V::ShortFloat { value } if value == -100.0));
        assert_eq!(dir, 1.0);

        // 标度化四舍五入为 i16
        let (v, _) = apply_mutation(&V::Scaled { value: 0 }, &p(MutationMode::Increment, 100.0, -10000.0, 10000.0), 1.0);
        assert!(matches!(v, V::Scaled { value: 100 }));

        // 归一化:用户给超范围上下限,内部钳到协议量程 [-1,1],触顶掉头
        let (v, dir) = apply_mutation(&V::Normalized { value: 0.98 }, &p(MutationMode::Increment, 0.05, -100.0, 100.0), 1.0);
        assert!(matches!(v, V::Normalized { value } if value == 1.0));
        assert_eq!(dir, -1.0);

        // 累计量递增
        let (v, _) = apply_mutation(&V::IntegratedTotal { value: 5, carry: false, sequence: 0 }, &p(MutationMode::Increment, 10.0, 0.0, 10000.0), 1.0);
        assert!(matches!(v, V::IntegratedTotal { value: 15, .. }));

        // 离散量(单点)收到递增 → 回退翻转,方向不变
        let (v, dir) = apply_mutation(&V::SinglePoint { value: false }, &p(MutationMode::Increment, 1.0, 0.0, 10.0), 1.0);
        assert!(matches!(v, V::SinglePoint { value: true }));
        assert_eq!(dir, 1.0);

        // Flip 模式:走原翻转
        let (v, _) = apply_mutation(&V::ShortFloat { value: 5.0 }, &p(MutationMode::Flip, 1.0, -100.0, 100.0), 1.0);
        assert!(matches!(v, V::ShortFloat { value } if value == -5.0));
    }

    #[tokio::test]
    async fn test_slave_server_station_management() {
        let server = SlaveServer::new(SlaveTransportConfig::default());
        let station = Station::new(1, "站1");
        server.add_station(station).await.unwrap();
        assert!(server.add_station(Station::new(1, "重复")).await.is_err());
    }

    // 回归:停止服务器必须立即返回。周期传输任务过去只在 `interval.tick()`
    // (默认 2000ms) 之后才检查退出标志,导致 stop() 干等到下一个 tick;
    // 由于 stop_server 命令在此期间持有全局写锁,前端所有轮询 IPC 被冻结,
    // 表现为"断开服务器前端响应超级慢"。stop() 应在毫秒级返回。
    #[tokio::test]
    async fn test_stop_returns_promptly_after_start() {
        // 借一个空闲端口再释放,让 stop() 的自连接(用于唤醒 accept)可命中。
        let port = {
            let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            l.local_addr().unwrap().port()
        };
        let transport = SlaveTransportConfig {
            bind_address: "127.0.0.1".to_string(),
            port,
            tls: SlaveTlsConfig::default(),
        };
        let mut server = SlaveServer::new(transport);
        server.start().await.expect("start should succeed");

        // 预热:让周期任务越过"立即触发"的首个 tick,park 在下一个 2s tick 上。
        // 真实场景中服务器已运行一段时间,停止时任务正卡在 interval.tick() 等待。
        tokio::time::sleep(std::time::Duration::from_millis(150)).await;

        let t = std::time::Instant::now();
        server.stop().await.expect("stop should succeed");
        let elapsed = t.elapsed();

        assert!(
            elapsed < std::time::Duration::from_millis(500),
            "stop() 耗时 {:?},应 < 500ms(周期任务需能被立即唤醒退出,而非干等 2s 的 interval tick)",
            elapsed
        );
    }

    #[test]
    fn test_add_point_coexist_different_type() {
        let mut station = Station::new(1, "Test");
        let def_sp = InformationObjectDef {
            ioa: 100,
            asdu_type: AsduTypeId::MSpNa1,
            category: DataCategory::SinglePoint,
            name: "SP".to_string(),
            comment: String::new(),
        };
        station.add_point(def_sp).unwrap();
        assert_eq!(station.data_points.len(), 1);
        assert_eq!(station.data_points.get(100, AsduTypeId::MSpNa1).unwrap().asdu_type, AsduTypeId::MSpNa1);

        // Add float type at same IOA — should coexist
        let def_float = InformationObjectDef {
            ioa: 100,
            asdu_type: AsduTypeId::MMeNc1,
            category: DataCategory::FloatMeasured,
            name: "Float".to_string(),
            comment: String::new(),
        };
        station.add_point(def_float).unwrap();
        assert_eq!(station.data_points.len(), 2); // both coexist
        assert!(station.data_points.get(100, AsduTypeId::MSpNa1).is_some());
        assert!(station.data_points.get(100, AsduTypeId::MMeNc1).is_some());
        assert_eq!(station.object_defs.len(), 2);
    }

    #[test]
    fn encode_point_frame_ex_emits_na_by_default() {
        let mut point = DataPoint::new(100, AsduTypeId::MSpNa1);
        point.value = DataPointValue::SinglePoint { value: true };
        let ca = 1u16.to_le_bytes();
        let mut seq = SeqState::default();
        let frame = encode_point_frame_ex(&point, 20, &ca, &mut seq, None);
        assert_eq!(frame[6], 1, "type=1 (NA)");
        assert_eq!(frame[15], 0x01, "SIQ ON");
    }

    #[test]
    fn encode_point_frame_ex_force_timestamped_emits_tb() {
        let mut point = DataPoint::new(100, AsduTypeId::MSpNa1);
        point.value = DataPointValue::SinglePoint { value: true };
        let ca = 1u16.to_le_bytes();
        let mut seq = SeqState::default();
        let frame = encode_point_frame_ex(&point, 3, &ca, &mut seq, Some(true));
        assert_eq!(frame[6], 30);
        assert_eq!(frame.len(), 23);
    }

    #[test]
    fn encode_points_grouped_emits_sq1() {
        let pts: Vec<DataPoint> = (100..105u32)
            .map(|ioa| {
                let mut p = DataPoint::new(ioa, AsduTypeId::MSpNa1);
                p.value = DataPointValue::SinglePoint { value: ioa % 2 == 0 };
                p
            })
            .collect();
        let refs: Vec<&DataPoint> = pts.iter().collect();
        let ca = 1u16.to_le_bytes();
        let mut seq = SeqState::default();
        let frame = encode_points_grouped(&refs, 20, &ca, &mut seq, false).unwrap();
        assert_eq!(frame[6], 1);
        assert_eq!(frame[7], 0x85);
        assert_eq!(&frame[12..15], &[100, 0, 0]);
    }

    #[test]
    fn encode_m_me_nd_1_two_bytes_no_qds() {
        let mut point = DataPoint::new(100, AsduTypeId::MMeNd1);
        point.value = DataPointValue::Normalized { value: 0.5 };
        let ca = 1u16.to_le_bytes();
        let mut seq = SeqState::default();
        let frame = encode_point_frame_ex(&point, 3, &ca, &mut seq, None);
        assert_eq!(frame[6], 21, "type=21 (M_ME_ND_1)");
        // APCI(6)+ASDU头(6)+IOA(3)+NVA(2) = 17,无 QDS、无时标
        assert_eq!(frame.len(), 17, "ND 帧值段恰 2 字节");
        // 0.5*32767=16383.5,round(非截断)→ 16384=0x4000,与解码 nva/32767 对称。
        assert_eq!(&frame[15..17], &0x4000i16.to_le_bytes(), "NVA LE");
    }

    #[test]
    fn encode_m_me_nd_1_omits_quality_even_when_set() {
        let mut point = DataPoint::new(100, AsduTypeId::MMeNd1);
        point.value = DataPointValue::Normalized { value: 0.5 };
        point.quality = QualityFlags { iv: true, nt: true, ..Default::default() };
        let ca = 1u16.to_le_bytes();
        let mut seq = SeqState::default();
        let frame = encode_point_frame_ex(&point, 3, &ca, &mut seq, Some(false));
        assert_eq!(frame[6], 21);
        assert_eq!(frame.len(), 17, "即便设了品质,ND 帧仍无 QDS 字节");
        assert_eq!(&frame[15..17], &0x4000i16.to_le_bytes()); // 0.5 → round 16384
    }

    #[test]
    fn m_me_nd_1_round_trip_preserves_type() {
        let mut point = DataPoint::new(7, AsduTypeId::MMeNd1);
        point.value = DataPointValue::Normalized { value: 0.5 };
        let ca = 1u16.to_le_bytes();
        let mut seq = SeqState::default();
        let frame = encode_point_frame_ex(&point, 3, &ca, &mut seq, None);
        let parsed = crate::decode::parse_frame_full(&frame).unwrap();
        let asdu = parsed.asdu.unwrap();
        assert_eq!(asdu.type_id, 21);
        assert_eq!(AsduTypeId::from_u8(asdu.type_id), Some(AsduTypeId::MMeNd1));
        let obj = &asdu.objects[0];
        assert_eq!(obj.ioa, 7);
        match obj.value.as_ref().unwrap() {
            DataPointValue::Normalized { value } => assert!((value - 0.5).abs() < 1e-3),
            _ => panic!("expected Normalized"),
        }
    }

    #[test]
    fn m_me_nd_1_not_derived_to_tb() {
        // ND 无时标变体 → should_derive_tb 恒 false
        let map = DataPointMap::new();
        assert!(!should_derive_tb(&map, AsduTypeId::MMeNd1, 100));
        // SQ=1 打包对 ND 段返回 None(逐点回退)
        let pts: Vec<DataPoint> = (100..103u32)
            .map(|ioa| {
                let mut p = DataPoint::new(ioa, AsduTypeId::MMeNd1);
                p.value = DataPointValue::Normalized { value: 0.25 };
                p
            })
            .collect();
        let refs: Vec<&DataPoint> = pts.iter().collect();
        let ca = 1u16.to_le_bytes();
        let mut seq = SeqState::default();
        assert!(
            encode_points_grouped(&refs, 3, &ca, &mut seq, false).is_none(),
            "ND 段不走 SQ=1 打包"
        );
    }

    // ---- 品质位写入帧字节 (QDS/SIQ/DIQ/BCR) ----

    /// 构造一个带指定值与品质的单点,编码为 NA I-frame,返回帧字节。
    fn encode_na(value: DataPointValue, q: QualityFlags, ioa: u32, ty: AsduTypeId) -> Vec<u8> {
        let mut p = DataPoint::new(ioa, ty);
        p.value = value;
        p.quality = q;
        let ca = 1u16.to_le_bytes();
        let mut seq = SeqState::default();
        encode_point_frame_ex(&p, 20, &ca, &mut seq, Some(false))
    }

    #[test]
    fn quality_single_point_iv_in_siq() {
        let f = encode_na(
            DataPointValue::SinglePoint { value: true },
            QualityFlags { iv: true, ..Default::default() },
            100, AsduTypeId::MSpNa1,
        );
        // SIQ = SPI(0x01) | IV(0x80)
        assert_eq!(f[6], 1, "type SP NA");
        assert_eq!(f[15], 0x81, "SIQ = ON + IV");
    }

    #[test]
    fn quality_double_point_nt_in_diq() {
        let f = encode_na(
            DataPointValue::DoublePoint { value: 2 },
            QualityFlags { nt: true, ..Default::default() },
            100, AsduTypeId::MDpNa1,
        );
        // DIQ = DPI(2) | NT(0x40)
        assert_eq!(f[6], 3, "type DP NA");
        assert_eq!(f[15], 0x42, "DIQ = DPI2 + NT");
    }

    #[test]
    fn quality_measured_ov_in_qds() {
        let f = encode_na(
            DataPointValue::ShortFloat { value: 0.0 },
            QualityFlags { ov: true, ..Default::default() },
            100, AsduTypeId::MMeNc1,
        );
        // 短浮点 NA: [f0 f1 f2 f3 QDS],QDS 在 frame[19]
        assert_eq!(f[6], 13, "type ShortFloat NA");
        assert_eq!(f[19] & 0x01, 0x01, "QDS OV bit set");
    }

    #[test]
    fn quality_measured_iv_nt_combined_in_qds() {
        let f = encode_na(
            DataPointValue::ShortFloat { value: 0.0 },
            QualityFlags { iv: true, nt: true, ..Default::default() },
            100, AsduTypeId::MMeNc1,
        );
        assert_eq!(f[19], 0xC0, "QDS = IV(0x80) | NT(0x40)");
    }

    #[test]
    fn quality_measured_good_qds_zero() {
        // 零回归:good() 测量类 QDS 仍为 0x00
        let f = encode_na(
            DataPointValue::ShortFloat { value: 0.0 },
            QualityFlags::good(),
            100, AsduTypeId::MMeNc1,
        );
        assert_eq!(f[19], 0x00, "good QDS = 0");
    }

    #[test]
    fn quality_integrated_total_iv_in_bcr() {
        let f = encode_na(
            DataPointValue::IntegratedTotal { value: 123, carry: false, sequence: 3 },
            QualityFlags { iv: true, ..Default::default() },
            100, AsduTypeId::MItNa1,
        );
        // 累计量 NA: [v0 v1 v2 v3 BCR],BCR 在 frame[19]
        assert_eq!(f[6], 15, "type IT NA");
        assert_eq!(f[19] & 0x80, 0x80, "BCR IV bit set");
        assert_eq!(f[19] & 0x1F, 3, "序号 3 保留");
    }

    #[test]
    fn quality_single_point_does_not_emit_ov() {
        // OV 仅测量类;单点 bit1 是 SPI 值,不应被 OV 污染
        let f = encode_na(
            DataPointValue::SinglePoint { value: false },
            QualityFlags { ov: true, ..Default::default() },
            100, AsduTypeId::MSpNa1,
        );
        assert_eq!(f[15], 0x00, "SIQ = OFF,OV 不写入");
    }

    #[test]
    fn quality_grouped_sq1_per_point() {
        // SQ=1 打包三点:good / iv / nt → QDS 各自 0x00 / 0x80 / 0x40
        let qs = [
            QualityFlags::good(),
            QualityFlags { iv: true, ..Default::default() },
            QualityFlags { nt: true, ..Default::default() },
        ];
        let pts: Vec<DataPoint> = (100..103u32)
            .zip(qs)
            .map(|(ioa, q)| {
                let mut p = DataPoint::new(ioa, AsduTypeId::MMeNc1);
                p.value = DataPointValue::ShortFloat { value: 0.0 };
                p.quality = q;
                p
            })
            .collect();
        let refs: Vec<&DataPoint> = pts.iter().collect();
        let ca = 1u16.to_le_bytes();
        let mut seq = SeqState::default();
        let f = encode_points_grouped(&refs, 20, &ca, &mut seq, false).unwrap();
        assert_eq!(f[6], 13, "type ShortFloat");
        assert_eq!(f[7], 0x83, "VSQ.SQ=1,n=3");
        // 每点 5 字节,值段从 frame[15] 起;各点 QDS 在第 5 字节
        assert_eq!(f[19], 0x00, "pt0 good QDS");
        assert_eq!(f[24], 0x80, "pt1 IV QDS");
        assert_eq!(f[29], 0x40, "pt2 NT QDS");
    }

    #[test]
    fn quality_roundtrip_encode_then_decode() {
        // 端到端:子站编码带品质 → 标准解码(主站/ParseFrameDialog 同路径)还原同样品质
        let f = encode_na(
            DataPointValue::ShortFloat { value: 1.5 },
            QualityFlags { nt: true, sb: true, ..Default::default() },
            100, AsduTypeId::MMeNc1,
        );
        let parsed = crate::decode::parse_frame_full(&f).expect("decode ok");
        let asdu = parsed.asdu.expect("应有 ASDU");
        let q = asdu.objects[0].quality.expect("应解出品质");
        assert!(q.nt && q.sb, "NT/SB 编解码往返一致");
        assert!(!q.iv && !q.ov && !q.bl, "未置位品质保持 false");
    }

    #[test]
    fn command_ack_cot_values() {
        assert_eq!(CommandAckCot::ActivationCon.as_u8(), 7);
        assert_eq!(CommandAckCot::DeactivationCon.as_u8(), 9);
        assert_eq!(CommandAckCot::ActivationTermination.as_u8(), 10);
    }

    #[test]
    fn test_batch_add_points() {
        let mut station = Station::new(1, "Test");
        let added = station.batch_add_points(100, 50, AsduTypeId::MSpNa1, "SP").unwrap();
        assert_eq!(added, 50);
        assert_eq!(station.data_points.len(), 50);

        for i in 0..50u32 {
            let ioa = 100 + i;
            let point = station.data_points.get(ioa, AsduTypeId::MSpNa1).unwrap();
            assert_eq!(point.asdu_type, AsduTypeId::MSpNa1);
        }
        assert_eq!(station.object_defs.len(), 50);
        assert_eq!(station.object_defs[0].name, "SP_100");
        assert_eq!(station.object_defs[49].name, "SP_149");

        // Add different type at same IOA range — should coexist
        station.batch_add_points(100, 50, AsduTypeId::MMeNc1, "FL").unwrap();
        assert_eq!(station.data_points.len(), 100); // 50 SP + 50 FL
        for i in 0..50u32 {
            assert!(station.data_points.get(100 + i, AsduTypeId::MSpNa1).is_some());
            assert!(station.data_points.get(100 + i, AsduTypeId::MMeNc1).is_some());
        }
    }
}
