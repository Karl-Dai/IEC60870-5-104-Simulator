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
    /// 该分类是否开启变位同步派生 TB。IntegratedTotals 永不派生(无开关);
    /// 控制方向分类只应答不上送,亦不派生。
    pub fn enabled_for(&self, category: DataCategory) -> bool {
        match category {
            DataCategory::SinglePoint => self.sp,
            DataCategory::DoublePoint => self.dp,
            DataCategory::StepPosition => self.st,
            DataCategory::Bitstring => self.bo,
            DataCategory::NormalizedMeasured => self.me_na,
            DataCategory::ScaledMeasured => self.me_nb,
            DataCategory::FloatMeasured => self.me_nc,
            DataCategory::IntegratedTotals
            | DataCategory::System
            | DataCategory::SingleCommand
            | DataCategory::DoubleCommand
            | DataCategory::StepCommand
            | DataCategory::BitstringCommand
            | DataCategory::NormalizedSetpoint
            | DataCategory::ScaledSetpoint
            | DataCategory::FloatSetpoint => false,
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
    /// 无显式映射的控制点按「同 CA + 同 IOA + 对应监视分类」自动写回
    /// (兼容旧行为)。关闭后仅显式映射生效。
    pub auto_map_commands: bool,
    /// 已声明但没有映射目标的控制点仍按 COT 6→7→10 正常应答。
    /// 关闭后未映射控制点回 COT=47 + P/N；未知 CA / IOA 始终分别回
    /// COT=46 / COT=47 + P/N，避免任意地址假成功。
    pub ack_unmapped_commands: bool,
    /// Select-Before-Operate 强制模式:Execute 必须有同点位未过期的 Select,
    /// 否则回否定 ACT_CON(7|0x40)。关闭时(默认)Select/Execute 均宽松接受。
    pub sbo_enforce: bool,
    /// SBO select 的有效期(毫秒),超时后 Execute 被拒。
    pub sbo_timeout_ms: u64,
    /// 应答主站时钟同步命令 (C_CS_NA_1, Type 103)。关闭后对结构合法的
    /// 激活请求回否定激活确认(COT=7 + P/N),不调整本机时钟(issue #28)。
    pub answer_clock_sync: bool,
    /// 命令执行后在 ACT_CON(7) 之外追加终止帧(COT 取 execute_ack_cot,默认 10)。
    /// 关闭后仅回 ACT_CON,适配不期待 ACT_TERM 的主站(issue #28 Actterm 开关)。
    pub send_act_term: bool,
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
            // 默认保留旧版同 CA/IOA 自动写回,兼容既有部署;可显式关闭。
            auto_map_commands: true,
            ack_unmapped_commands: true,
            sbo_enforce: false,
            sbo_timeout_ms: 30_000,
            answer_clock_sync: true,
            send_act_term: true,
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
                    comment: String::new(), mapping: None,
                    command_qualifier: None, select_before_operate: None
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

    /// 严格新增点位:目标 (IOA, 类型) 已存在时报 `DuplicateIoa`,不改动已有 def。
    /// 用户在 UI 上「添加单个点位」走这条路 —— `add_point` 的静默覆盖语义会把
    /// 已有点的名称/备注/QU-QL/S-E/控制映射整份冲掉(仅运行值因 contains 判断
    /// 而侥幸保留),属于不可撤销的数据损坏。
    /// 配置导入(`load_config`)、默认站生成、批量添加仍走宽松语义的 `add_point`。
    pub fn add_point_strict(&mut self, def: InformationObjectDef) -> Result<(), SlaveError> {
        if self.data_points.contains(def.ioa, def.asdu_type)
            || self.object_defs.iter().any(|d| d.ioa == def.ioa && d.asdu_type == def.asdu_type)
        {
            return Err(SlaveError::DuplicateIoa(def.ioa));
        }
        self.add_point(def)
    }

    /// 新增或**覆盖**点位定义:同 (IOA, 类型) 已存在时整份替换 def。
    /// 覆盖语义是配置导入/默认站生成等幂等重放路径依赖的;用户手动添加请用
    /// [`Station::add_point_strict`]。
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
                    ioa, asdu_type, category, name, comment: String::new(), mapping: None,
                    command_qualifier: None, select_before_operate: None
                });
            }
        }
        Ok(count)
    }

    /// Move a point (def + runtime data) to a new IOA, keeping value/quality.
    /// Fails when the target (ioa, type) already exists. Caller is responsible
    /// for updating control mappings in other stations that reference this
    /// point (they are addressed by CA + IOA + type).
    pub fn move_point_ioa(
        &mut self,
        ioa: u32,
        asdu_type: AsduTypeId,
        new_ioa: u32,
    ) -> Result<(), SlaveError> {
        if new_ioa == ioa {
            return Ok(());
        }
        if self.data_points.contains(new_ioa, asdu_type)
            || self.object_defs.iter().any(|d| d.ioa == new_ioa && d.asdu_type == asdu_type)
        {
            return Err(SlaveError::DuplicateIoa(new_ioa));
        }
        let def = self
            .object_defs
            .iter_mut()
            .find(|d| d.ioa == ioa && d.asdu_type == asdu_type)
            .ok_or(SlaveError::IoaNotFound(ioa))?;
        def.ioa = new_ioa;
        if let Some(mut point) = self.data_points.remove(ioa, asdu_type) {
            point.ioa = new_ioa;
            self.data_points.insert(point);
        }
        Ok(())
    }

    /// Batch-add data points at explicit IOAs (supports non-contiguous ranges,
    /// issue #28). Existing (ioa, type) pairs are skipped; returns the number
    /// actually created. Names: `{prefix}_{ioa}`, or `{prefix}_{typeid}_{ioa}`
    /// when `name_with_type_id`. Control options (QU/QL, S/E) are stamped on
    /// every newly created def — caller validates them beforehand.
    pub fn batch_add_points_list(
        &mut self,
        ioas: &[u32],
        asdu_type: AsduTypeId,
        name_prefix: &str,
        name_with_type_id: bool,
        command_qualifier: Option<u8>,
        select_before_operate: Option<bool>,
    ) -> Result<u32, SlaveError> {
        use std::collections::HashSet;
        let category = asdu_type.category();
        let mut existing: HashSet<(u32, AsduTypeId)> = self.object_defs.iter()
            .map(|d| (d.ioa, d.asdu_type))
            .collect();
        let mut created = 0u32;
        for &ioa in ioas {
            if !self.data_points.contains(ioa, asdu_type) {
                self.data_points.insert(DataPoint::new(ioa, asdu_type));
            }
            // existing 随创建同步扩充,输入列表内的重复 IOA 不会 push 两个 def。
            if existing.insert((ioa, asdu_type)) {
                let name = if name_prefix.is_empty() {
                    String::new()
                } else if name_with_type_id {
                    format!("{}_{}_{}", name_prefix, asdu_type as u8, ioa)
                } else {
                    format!("{}_{}", name_prefix, ioa)
                };
                self.object_defs.push(InformationObjectDef {
                    ioa, asdu_type, category, name, comment: String::new(), mapping: None,
                    command_qualifier, select_before_operate,
                });
                created += 1;
            }
        }
        Ok(created)
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
    /// 明文路径读任务句柄。stop() 用它 abort 驻留在 read().await 的读任务,
    /// drop ReadHalf 以立即关闭已建立的连接 socket(写任务靠 shutdown_flag
    /// 在 5ms 内自退并 drop WriteHalf)。TLS 阻塞任务不可 abort,恒为 None,
    /// 由 100ms 读超时轮询 flag 自退。
    reader_handle: Option<tokio::task::JoinHandle<()>>,
}

type SharedConnections = Arc<RwLock<HashMap<SocketAddr, ConnectionWrite>>>;

/// 从 TCP/TLS 字节流缓冲区中取出下一条完整 APDU。
///
/// TCP 与 TLS 都只提供字节流，不保证一次 `read` 对应一条 IEC 104 帧：
/// 一条 APDU 可能被拆成多次读取，多条 APDU 也可能粘在一次读取中。两条
/// 收包路径共用本函数，避免 TLS 把网络分片误判成协议短帧。
fn take_next_apdu(reassembly: &mut Vec<u8>) -> Option<Vec<u8>> {
    loop {
        if reassembly.len() < 2 {
            return None;
        }

        if reassembly[0] != 0x68 {
            match reassembly.iter().position(|&byte| byte == 0x68) {
                Some(start) => {
                    reassembly.drain(..start);
                }
                None => {
                    reassembly.clear();
                    return None;
                }
            }
            continue;
        }

        let frame_len = reassembly[1] as usize + 2;
        if reassembly.len() < frame_len {
            return None;
        }
        return Some(reassembly.drain(..frame_len).collect());
    }
}

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
    if out.len() > 8 {
        // 保留收帧的 test 位(bit7):测试帧的回显应仍带 test 位(IEC 60870-5-101 §7.2.2.3)。
        // negative-confirm(bit6)不由收帧透传,而由各分支显式设置(如 103 拒收回 45|0x40,确认帧不带 negative)。
        // 故只透传 test 位,cot 的 cause+negative 位由调用者原样给出。
        let test_bit = out[8] & 0x80;
        out[8] = cot | test_bit; }
    seq.ssn = seq.ssn.wrapping_add(2);
    out
}

/// C_CS_NA_1 的结构/协议决策。异步明文与阻塞 TLS 路径必须共用，确保
/// 校验优先级和响应 COT 完全一致。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ClockSyncDecision {
    Accept,
    Disabled,
    UnknownCot { cot: u8 },
    UnknownIoa { ioa: u32 },
    Malformed { reason: &'static str },
}

impl ClockSyncDecision {
    fn response_cot(self) -> Option<u8> {
        match self {
            Self::Accept => Some(7),
            Self::Disabled => Some(7 | 0x40),
            Self::UnknownCot { .. } => Some(45 | 0x40),
            Self::UnknownIoa { .. } => Some(47 | 0x40),
            Self::Malformed { .. } => None,
        }
    }
}

/// 校验单对象 C_CS_NA_1。完整合法帧固定为 22 字节：
/// APCI(6)+ASDU头/CA(6)+IOA(3)+CP56Time2a(7)。
///
/// 校验顺序与命令语义一致：先排除无法解析的结构，再判 COT；只有合法
/// 激活(COT=6)才校验系统命令固定 IOA=0，最后才应用应答开关。
fn decide_clock_sync(frame: &[u8], enabled: bool) -> ClockSyncDecision {
    if frame.len() < 2
        || frame.first() != Some(&0x68)
        || frame[1] as usize + 2 != frame.len()
    {
        return ClockSyncDecision::Malformed { reason: "apdu_length" };
    }
    if frame.len() != 22 {
        return ClockSyncDecision::Malformed { reason: "cp56_length" };
    }
    if frame[6] != 103 {
        return ClockSyncDecision::Malformed { reason: "type_id" };
    }
    // VSQ:SQ(bit7)=0 且对象数=1，故完整字节必须为 0x01。
    if frame[7] != 0x01 {
        return ClockSyncDecision::Malformed { reason: "vsq" };
    }

    let cot = frame[8] & 0x3F;
    if cot != 6 {
        return ClockSyncDecision::UnknownCot { cot };
    }

    let ioa = u32::from_le_bytes([frame[12], frame[13], frame[14], 0]);
    if ioa != 0 {
        return ClockSyncDecision::UnknownIoa { ioa };
    }

    if enabled {
        ClockSyncDecision::Accept
    } else {
        ClockSyncDecision::Disabled
    }
}

/// 时钟同步应答/丢弃日志。直接按 `ClockSyncDecision` 生成，保证日志所述
/// COT 与线上实际响应一致；畸形 ASDU 不发送伪造的 UNKNOWN_TYPE 响应。
fn clock_sync_log_entry(decision: ClockSyncDecision, ca: u16, frame_len: usize) -> LogEntry {
    match decision {
        ClockSyncDecision::Accept => LogEntry::new(
            Direction::Tx,
            FrameLabel::ClockSync,
            format!("Clock sync acknowledged (COT=7) CA={}", ca),
        ),
        ClockSyncDecision::Disabled => LogEntry::new(
            Direction::Tx,
            FrameLabel::ClockSync,
            format!("Clock sync rejected (replies disabled, COT=7 + P/N) CA={}", ca),
        )
        .with_detail_event("clockSyncDisabled", serde_json::json!({ "ca": ca })),
        ClockSyncDecision::UnknownCot { cot } => LogEntry::new(
            Direction::Tx,
            FrameLabel::ClockSync,
            format!("Clock sync rejected (unknown COT={}, COT=45 + P/N) CA={}", cot, ca),
        )
        .with_detail_event(
            "clockSyncInvalidCot",
            serde_json::json!({ "ca": ca, "cot": cot }),
        ),
        ClockSyncDecision::UnknownIoa { ioa } => LogEntry::new(
            Direction::Tx,
            FrameLabel::ClockSync,
            format!("Clock sync rejected (unknown IOA={}, COT=47 + P/N) CA={}", ioa, ca),
        )
        .with_detail_event(
            "clockSyncInvalidIoa",
            serde_json::json!({ "ca": ca, "ioa": ioa }),
        ),
        ClockSyncDecision::Malformed { reason } => LogEntry::new(
            Direction::Rx,
            FrameLabel::ClockSync,
            format!(
                "Malformed clock sync ASDU dropped (reason={}, len={}) CA={}",
                reason, frame_len, ca
            ),
        )
        .with_detail_event(
            "clockSyncMalformed",
            serde_json::json!({ "ca": ca, "len": frame_len, "reason": reason }),
        ),
    }
}

// ---------------------------------------------------------------------------
// Control-direction command processing (Type 45-51 / 58-64)
// 异步(明文 TCP)与阻塞(TLS)两条收包路径共用,消除历史上六份复制粘贴分支。
// ---------------------------------------------------------------------------

/// 控制命令携带的值(按命令族)。
#[derive(Debug, Clone, Copy)]
enum CommandValue {
    Single(bool),
    Double(u8),
    /// RCS 原始低 2 位:1=降一步,2=升一步(0/3 为标准保留值,照记录不执行步进)。
    Step(u8),
    Bitstring(u32),
    /// NVA 原始 i16(-32768..32767 ↔ -1.0..+1.0)。
    Normalized(i16),
    Scaled(i16),
    Float(f32),
}

impl CommandValue {
    /// 英文技术值串(日志 fallback / CSV 用;UI 经 detail_event 本地化)。
    fn display(&self) -> String {
        match self {
            Self::Single(v) => if *v { "ON".into() } else { "OFF".into() },
            Self::Double(v) => match v { 1 => "OFF".into(), 2 => "ON".into(), other => format!("{}", other) },
            Self::Step(v) => match v { 1 => "LOWER".into(), 2 => "HIGHER".into(), other => format!("{}", other) },
            Self::Bitstring(v) => format!("0x{:08X}", v),
            Self::Normalized(v) => format!("{:.4}", *v as f32 / 32767.0),
            Self::Scaled(v) => format!("{}", v),
            Self::Float(v) => format!("{:.6}", v),
        }
    }
}

/// 解析后的控制方向命令。
struct ParsedControlCommand {
    type_id: AsduTypeId,
    ioa: u32,
    value: CommandValue,
    is_select: bool,
    /// SCO/DCO/RCO 的 QU(bit2-6)或 QOS 的 QL(bit0-6);位串命令无限定词,恒 0。
    qu: u8,
    frame_label: FrameLabel,
}

/// 解析 45-51 / 58-64 控制命令帧;长度不足或类型非控制方向返回 None。
/// 帧布局:6 APCI + type(1) vsq(1) cot(2) ca(2) + ioa(3) + 值/限定词 [+ CP56Time2a(7)]。
fn parse_control_command(asdu_type: u8, data: &[u8]) -> Option<ParsedControlCommand> {
    let type_id = AsduTypeId::from_u8(asdu_type)?;
    if !type_id.is_control() { return None; }
    let time_len = if type_id.is_timestamped() { 7usize } else { 0 };
    let base = type_id.untimestamped_variant();
    let min_len = match base {
        AsduTypeId::CScNa1 | AsduTypeId::CDcNa1 | AsduTypeId::CRcNa1 => 16 + time_len,
        AsduTypeId::CSeNa1 | AsduTypeId::CSeNb1 => 18 + time_len,
        AsduTypeId::CSeNc1 => 20 + time_len,
        AsduTypeId::CBoNa1 => 19 + time_len,
        _ => return None,
    };
    if data.len() < min_len { return None; }
    let ioa = u32::from_le_bytes([data[12], data[13], data[14], 0]);
    let (value, is_select, qu, frame_label) = match base {
        AsduTypeId::CScNa1 => {
            let sco = data[15];
            (CommandValue::Single(sco & 0x01 != 0), sco & 0x80 != 0, (sco >> 2) & 0x1F, FrameLabel::SingleCommand)
        }
        AsduTypeId::CDcNa1 => {
            let dco = data[15];
            (CommandValue::Double(dco & 0x03), dco & 0x80 != 0, (dco >> 2) & 0x1F, FrameLabel::DoubleCommand)
        }
        AsduTypeId::CRcNa1 => {
            let rco = data[15];
            (CommandValue::Step(rco & 0x03), rco & 0x80 != 0, (rco >> 2) & 0x1F, FrameLabel::StepCommand)
        }
        AsduTypeId::CSeNa1 => {
            let raw = i16::from_le_bytes([data[15], data[16]]);
            let qos = data[17];
            (CommandValue::Normalized(raw), qos & 0x80 != 0, qos & 0x7F, FrameLabel::SetpointNormalized)
        }
        AsduTypeId::CSeNb1 => {
            let raw = i16::from_le_bytes([data[15], data[16]]);
            let qos = data[17];
            (CommandValue::Scaled(raw), qos & 0x80 != 0, qos & 0x7F, FrameLabel::SetpointScaled)
        }
        AsduTypeId::CSeNc1 => {
            let raw = f32::from_le_bytes([data[15], data[16], data[17], data[18]]);
            let qos = data[19];
            (CommandValue::Float(raw), qos & 0x80 != 0, qos & 0x7F, FrameLabel::SetpointFloat)
        }
        AsduTypeId::CBoNa1 => {
            let raw = u32::from_le_bytes([data[15], data[16], data[17], data[18]]);
            // C_BO_NA_1/C_BO_TA_1 无 S/E 位与限定词(IEC 60870-5-101 §7.3.2.8)。
            (CommandValue::Bitstring(raw), false, 0, FrameLabel::Bitstring)
        }
        _ => return None,
    };
    Some(ParsedControlCommand { type_id, ioa, value, is_select, qu, frame_label })
}

/// 每连接的 SBO select 状态:(ca, ioa, 精确控制类型) → select 时刻。
/// 带时标和不带时标控制点允许独立配置，不能只按命令族合并。
type SelectStateMap = std::collections::HashMap<(u16, u32, AsduTypeId), tokio::time::Instant>;

/// 命令处理结果。replies 依序回送;spontaneous 为执行成功后需向所有连接
/// 扇出突发上送(COT=3)的目标监视点。
struct ControlCommandOutcome {
    replies: Vec<Vec<u8>>,
    log_entries: Vec<LogEntry>,
    spontaneous: Option<(u16, (u32, AsduTypeId))>,
}

/// 将命令值转换为目标点位的新值。步调节命令对目标步位置做 ±1 增量并夹取。
fn command_value_for_point(v: &CommandValue, current: &DataPointValue) -> DataPointValue {
    match v {
        CommandValue::Single(b) => DataPointValue::SinglePoint { value: *b },
        CommandValue::Double(d) => DataPointValue::DoublePoint { value: *d },
        CommandValue::Step(rcs) => {
            if let DataPointValue::StepPosition { value, .. } = current {
                let delta: i8 = match rcs { 1 => -1, 2 => 1, _ => 0 };
                DataPointValue::StepPosition { value: value.saturating_add(delta).clamp(-64, 63), transient: false }
            } else {
                current.clone()
            }
        }
        CommandValue::Bitstring(b) => DataPointValue::Bitstring { value: *b },
        CommandValue::Normalized(nva) => DataPointValue::Normalized { value: *nva as f32 / 32767.0 },
        CommandValue::Scaled(sva) => DataPointValue::Scaled { value: *sva },
        CommandValue::Float(f) => DataPointValue::ShortFloat { value: *f },
    }
}

/// 构造命令拒收日志(英文 fallback + detail_event 结构化,前端本地化)。
fn command_rejected_log(cmd: &ParsedControlCommand, ca: u16, reason: &str, cot: u8) -> LogEntry {
    LogEntry::new(
        Direction::Tx,
        cmd.frame_label.clone(),
        format!("{} rejected ({}) IOA={} CA={}", cmd.type_id.name(), reason, cmd.ioa, ca),
    )
    .with_detail_event("cmdRejected", serde_json::json!({
        "type": cmd.type_id.name(), "ioa": cmd.ioa, "ca": ca, "reason": reason, "cot": cot,
    }))
}

/// 控制命令统一处理:COT 校验 → 目标解析(显式映射 / 自动同 CA+IOA / 未映射
/// 策略)→ SBO 状态机 → 执行(写控制点自身 + 映射目标)→ 应答帧构造。
/// 不做任何 IO;replies/spontaneous 由调用方按各自路径投递。
async fn process_control_command(
    data: &[u8],
    cause: u8,
    ca: u16,
    cmd: ParsedControlCommand,
    stations: &SharedStations,
    ops: &RemoteOperationConfig,
    seq: &SharedSeq,
    select_state: &mut SelectStateMap,
) -> ControlCommandOutcome {
    let mut out = ControlCommandOutcome { replies: Vec::new(), log_entries: Vec::new(), spontaneous: None };
    let sel_key = (ca, cmd.ioa, cmd.type_id);

    // COT 校验:控制命令仅接受激活(6)/停止激活(8),其余回未知 COT 否定确认
    // (COT=45|0x40,与 103 时钟同步拒收路径一致)。
    if cause != 6 && cause != 8 {
        if ops.answer_commands {
            let ack = { let mut s = seq.lock().await; build_response_frame(data, 45 | 0x40, &mut s) };
            out.replies.push(ack);
        }
        out.log_entries.push(command_rejected_log(&cmd, ca, "unexpected_cot", cause));
        return out;
    }

    // 停止激活(COT=8):撤销未决 select,仅回停止确认,不执行、不写值、不突发。
    if cause == 8 {
        select_state.remove(&sel_key);
        let ack = { let mut s = seq.lock().await; build_response_frame(data, ops.cancel_ack_cot.as_u8(), &mut s) };
        out.replies.push(ack);
        out.log_entries.push(
            LogEntry::new(
                Direction::Tx, cmd.frame_label.clone(),
                format!("{} deactivation confirmed IOA={} CA={}", cmd.type_id.name(), cmd.ioa, ca),
            )
            .with_detail_event("cmdCancel", serde_json::json!({
                "type": cmd.type_id.name(), "ioa": cmd.ioa, "ca": ca,
            })),
        );
        return out;
    }

    // —— 目标解析 ——
    // declared: (ca, ioa, type) 上声明了该控制点。带时标与不带时标控制点
    // 可以各自映射到不同监视点，因此按精确类型查找。
    // target: 显式映射(校验族匹配 + 目标点存在) > 自动同 CA+IOA 映射(可关)。
    let (station_known, declared, target, mapping_broken, configured_qualifier, configured_sbo) = {
        let guard = stations.read().await;
        let station_known = guard.contains_key(&ca);
        let (declared, explicit, point_options) = match guard.get(&ca) {
            Some(st) => {
                let def = st.object_defs.iter()
                    .find(|d| d.ioa == cmd.ioa && d.asdu_type == cmd.type_id);
                (
                    st.data_points.contains(cmd.ioa, cmd.type_id),
                    def.and_then(|d| d.mapping),
                    def.map(|d| (d.command_qualifier, d.select_before_operate)),
                )
            }
            None => (false, None, None),
        };
        let mut mapping_broken = false;
        let target = if let Some(m) = explicit {
            let family_ok = cmd.type_id.allowed_target_categories().contains(&m.asdu_type.category());
            let exists = family_ok
                && guard.get(&m.common_address)
                    .map(|st| st.data_points.contains(m.ioa, m.asdu_type))
                    .unwrap_or(false);
            if exists {
                Some((m.common_address, m.ioa, m.asdu_type))
            } else {
                mapping_broken = true;
                None
            }
        } else if ops.auto_map_commands {
            cmd.type_id.category().auto_map_monitor_category().and_then(|mc| {
                guard.get(&ca).and_then(|st| {
                    st.data_points.get_by_category(cmd.ioa, mc).map(|p| (ca, p.ioa, p.asdu_type))
                })
            })
        } else {
            None
        };
        let (configured_qualifier, configured_sbo) = point_options.unwrap_or((None, None));
        (station_known, declared, target, mapping_broken, configured_qualifier, configured_sbo)
    };

    // 未知 CA / IOA 必须否定确认。ack_unmapped_commands 只控制“已声明的
    // 控制点没有监视映射”是否仍正常应答，不能让任意未知地址假成功。
    let rejection = if !station_known {
        Some(("unknown_ca", 46u8))
    } else if !declared && target.is_none() {
        Some(("unknown_ioa", 47u8))
    } else if declared && target.is_none() && !ops.ack_unmapped_commands {
        Some((if mapping_broken { "mapping_target_missing" } else { "unmapped" }, 47u8))
    } else {
        None
    };
    if let Some((reason, cot)) = rejection {
        if ops.answer_commands {
            let ack = { let mut s = seq.lock().await; build_response_frame(data, cot | 0x40, &mut s) };
            out.replies.push(ack);
        }
        out.log_entries.push(command_rejected_log(&cmd, ca, reason, cause));
        return out;
    }

    // Per-point QOC/QL configuration is optional for legacy files. When set,
    // a command with a different qualifier is rejected with a negative
    // activation confirmation before any value is changed.
    if let Some(expected) = configured_qualifier {
        if expected != cmd.qu {
            if ops.answer_commands {
                let ack = { let mut s = seq.lock().await; build_response_frame(data, 7 | 0x40, &mut s) };
                out.replies.push(ack);
            }
            out.log_entries.push(command_rejected_log(&cmd, ca, "qualifier_mismatch", cause));
            return out;
        }
    }

    // A configured direct-only point rejects a Select frame. Global SBO mode
    // intentionally overrides that setting for test scenarios.
    if cmd.is_select && configured_sbo == Some(false) && !ops.sbo_enforce {
        if ops.answer_commands {
            let ack = { let mut s = seq.lock().await; build_response_frame(data, 7 | 0x40, &mut s) };
            out.replies.push(ack);
        }
        out.log_entries.push(command_rejected_log(&cmd, ca, "select_not_allowed", cause));
        return out;
    }

    // —— Select(S/E=1):记录 SBO 状态,只回选择确认,不执行 ——
    if cmd.is_select {
        if ops.sbo_enforce || configured_sbo == Some(true) {
            select_state.insert(sel_key, tokio::time::Instant::now());
        }
        if ops.answer_commands {
            let ack = { let mut s = seq.lock().await; build_response_frame(data, ops.select_ack_cot.as_u8(), &mut s) };
            out.replies.push(ack);
        }
        out.log_entries.push(
            LogEntry::new(
                Direction::Tx, cmd.frame_label.clone(),
                format!("{} select acknowledged IOA={} QU={} CA={}", cmd.type_id.name(), cmd.ioa, cmd.qu, ca),
            )
            .with_detail_event("cmdSelect", serde_json::json!({
                "type": cmd.type_id.name(), "ioa": cmd.ioa, "ca": ca, "qu": cmd.qu,
            })),
        );
        return out;
    }

    // —— Execute:SBO 强制模式下须有未过期的 select ——
    if ops.sbo_enforce || configured_sbo == Some(true) {
        let fresh = select_state
            .remove(&sel_key)
            .map(|t| t.elapsed() <= std::time::Duration::from_millis(ops.sbo_timeout_ms))
            .unwrap_or(false);
        if !fresh {
            if ops.answer_commands {
                let ack = { let mut s = seq.lock().await; build_response_frame(data, 7 | 0x40, &mut s) };
                out.replies.push(ack);
            }
            out.log_entries.push(command_rejected_log(&cmd, ca, "sbo_select_required", cause));
            return out;
        }
    }

    // —— 执行:控制点自身记录最近命令值;映射目标按族转换写入 ——
    {
        let mut guard = stations.write().await;
        if declared {
            if let Some(st) = guard.get_mut(&ca) {
                if let Some(dp) = st.data_points.get_mut(cmd.ioa, cmd.type_id) {
                    let next = command_value_for_point(&cmd.value, &dp.value);
                    dp.value = next;
                    dp.timestamp = Some(chrono::Utc::now());
                }
                st.data_points.mark_changed(cmd.ioa, cmd.type_id);
            }
        }
        if let Some((tca, tioa, ttype)) = target {
            if let Some(st) = guard.get_mut(&tca) {
                if let Some(dp) = st.data_points.get_mut(tioa, ttype) {
                    let next = command_value_for_point(&cmd.value, &dp.value);
                    dp.value = next;
                    dp.timestamp = Some(chrono::Utc::now());
                }
                st.data_points.mark_changed(tioa, ttype);
            }
        }
    }

    if ops.answer_commands {
        {
            let mut s = seq.lock().await;
            out.replies.push(build_response_frame(data, 7, &mut s));
            // Actterm 开关(issue #28):关闭后仅回 ACT_CON,不再追加终止帧。
            if ops.send_act_term {
                out.replies.push(build_response_frame(data, ops.execute_ack_cot.as_u8(), &mut s));
            }
        }
        out.spontaneous = target.map(|(tca, tioa, ttype)| (tca, (tioa, ttype)));
    }

    let target_desc = match target {
        Some((tca, tioa, ttype)) => format!("{}@{}:{}", ttype.name(), tca, tioa),
        None => "unmapped".to_string(),
    };
    out.log_entries.push(
        LogEntry::new(
            Direction::Tx, cmd.frame_label.clone(),
            format!(
                "{} executed val={} QU={} IOA={} CA={} target={}",
                cmd.type_id.name(), cmd.value.display(), cmd.qu, cmd.ioa, ca, target_desc,
            ),
        )
        .with_detail_event("cmdExecute", serde_json::json!({
            "type": cmd.type_id.name(), "ioa": cmd.ioa, "ca": ca,
            "val": cmd.value.display(), "qu": cmd.qu, "target": target_desc,
        })),
    );
    if mapping_broken {
        out.log_entries.push(
            LogEntry::new(
                Direction::Tx, cmd.frame_label.clone(),
                format!("{} mapping target missing IOA={} CA={}", cmd.type_id.name(), cmd.ioa, ca),
            )
            .with_detail_event("cmdMappingBroken", serde_json::json!({
                "type": cmd.type_id.name(), "ioa": cmd.ioa, "ca": ca,
            })),
        );
    }
    out
}

// ---------------------------------------------------------------------------
// SlaveServer
// ---------------------------------------------------------------------------

pub type SharedStations = Arc<RwLock<HashMap<u16, Station>>>;
pub type SharedRemoteOps = Arc<RwLock<RemoteOperationConfig>>;
pub type SharedProtocolTiming = Arc<RwLock<ProtocolTimingConfig>>;

struct PointMutationTask {
    handle: tokio::task::JoinHandle<()>,
    params: MutationParams,
    period_ms: u32,
}

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
        tokio::sync::Mutex<HashMap<(u16, u32, AsduTypeId), PointMutationTask>>,
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
        if let Some(task) = guard.remove(&key) { task.handle.abort(); }

        let params = normalize_mutation_params(asdu_type, params);
        let stations = self.stations.clone();
        let connections = self.connections.clone();
        let remote_ops = self.remote_ops.clone();
        let log_collector = self.log_collector.clone();
        let shutdown_flag = self.shutdown_flag.clone();
        let period_ms = period_ms.max(50);
        let handle = tokio::spawn(async move {
            let period = std::time::Duration::from_millis(period_ms as u64);
            let mut interval = tokio::time::interval(period);
            // A delayed spontaneous-send path must not replay every missed
            // deadline in a burst. Resume with one overdue tick, then keep the
            // configured spacing from that point onward.
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
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
                // The tick may have fired before a contended station lock was
                // acquired. Base the next deadline on the completed mutation
                // attempt so two value changes cannot run back-to-back after
                // that lock (or another pre-mutation delay) is released.
                interval.reset_after(period);
                if mutated {
                    do_queue_spontaneous(
                        &stations, &connections, &remote_ops, &log_collector,
                        ca, &[(ioa, asdu_type)],
                    ).await;
                }
            }
        });
        guard.insert(key, PointMutationTask { handle, params, period_ms });
    }

    /// 停止单个点位的周期变位。
    pub async fn stop_point_mutation(&self, ca: u16, ioa: u32, asdu_type: AsduTypeId) {
        let mut guard = self.point_mutation_handles.lock().await;
        if let Some(task) = guard.remove(&(ca, ioa, asdu_type)) { task.handle.abort(); }
    }

    /// 返回当前活跃的周期变位点位 (ca, ioa, asdu_type, mode)。
    pub async fn list_point_mutations(&self) -> Vec<(u16, u32, AsduTypeId, MutationMode)> {
        self.list_point_mutations_with_period()
            .await
            .into_iter()
            .map(|(ca, ioa, asdu_type, mode, _period_ms)| (ca, ioa, asdu_type, mode))
            .collect()
    }

    /// 返回当前活跃的周期变位点位及归一化后的实际周期。
    pub async fn list_point_mutations_with_period(
        &self,
    ) -> Vec<(u16, u32, AsduTypeId, MutationMode, u32)> {
        self.list_point_mutations_with_params()
            .await
            .into_iter()
            .map(|(ca, ioa, asdu_type, params, period_ms)| {
                (ca, ioa, asdu_type, params.mode, period_ms)
            })
            .collect()
    }

    /// 返回当前活跃的周期变位点位及完整、实际生效的任务参数。
    pub async fn list_point_mutations_with_params(
        &self,
    ) -> Vec<(u16, u32, AsduTypeId, MutationParams, u32)> {
        self.point_mutation_handles
            .lock()
            .await
            .iter()
            .map(|(&(ca, ioa, t), task)| (ca, ioa, t, task.params, task.period_ms))
            .collect()
    }

    pub async fn start(&mut self) -> Result<(), SlaveError> {
        if self.state == ServerState::Running { return Err(SlaveError::AlreadyRunning); }

        let addr_str = format!("{}:{}", self.transport.bind_address, self.transport.port);
        let listener = AsyncTcpListener::bind(&addr_str)
            .await
            .map_err(|e| classify_bind_error(&addr_str, &e))?;

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

        // 每次启动重建停止标志与通知句柄:上一轮的任务(写任务 5ms 轮询、TLS 阻塞
        // 任务 100ms 轮询)可能尚未观察到 flag=true;若复用同一 Arc 并 store(false),
        // 快速 stop→start 会让旧任务永远错过停止信号而滞留。旧任务继续持有
        // 旧 Arc(恒为 true),新任务用新 Arc。
        self.shutdown_flag = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let shutdown_flag = self.shutdown_flag.clone();
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
                                // 控制方向点位只应答命令,不参与周期/变位上送。
                                if point.asdu_type.category().is_control() { continue; }
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
                                reader_handle: None,
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
                                reader_handle: None,
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
                            // Keep its handle so stop() can cancel read().await immediately.
                            let connections_for_handle = Arc::clone(&connections);
                            let reader_handle = tokio::spawn(async move {
                                handle_client_read_loop(rh, stations_for_reader, lc_for_reader, flag, connections, queue_for_reader, seq_for_reader, started_for_reader, addr_for_read, conn_remote_ops, conn_protocol_timing).await;
                                // 读循环退出 = 对端已断开,通知写任务退出以释放 WriteHalf。
                                conn_closed_for_reader.store(true, std::sync::atomic::Ordering::SeqCst);
                            });
                            let mut guard = connections_for_handle.write().await;
                            if let Some(conn) = guard.get_mut(&peer_addr) {
                                conn.reader_handle = Some(reader_handle);
                            } else {
                                // The peer disconnected between spawn and registration.
                                reader_handle.abort();
                            }
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
        // 直接取消 accept 任务来释放监听 socket。过去通过回连监听地址唤醒
        // listener.accept()，但默认监听地址是 0.0.0.0；Windows 不能把通配地址
        // 当作连接目标，回连失败后 JoinHandle 会永远等在 accept()，同时
        // stop_server 一直占着应用级写锁，最终令参数保存/日志轮询等 IPC 全部挂起。
        if let Some(h) = self.server_handle.take() {
            h.abort();
            let _ = h.await;
        }
        // Plain readers can otherwise remain blocked in read().await after the
        // listener is gone. TLS readers have a 100ms socket timeout and observe
        // shutdown_flag; clearing the registry also prevents stale entries in a
        // rapid stop -> start cycle.
        {
            let mut connections = self.connections.write().await;
            for conn in connections.values_mut() {
                if let Some(handle) = conn.reader_handle.take() {
                    handle.abort();
                }
            }
            connections.clear();
        }
        if let Some(h) = self.cyclic_handle.take() { let _ = h.await; }
        {
            let mut handles = self.point_mutation_handles.lock().await;
            for (_key, task) in handles.drain() { task.handle.abort(); }
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

    // 按 (ca, cot_data) 维护在途召唤扫描任务的 JoinHandle:cot_data=20 为 GI,37 为计数量召唤。
    // 同一连接的 I 帧串行处理,本 map 无需锁。激活时 spawn 后存入(先 abort 旧的同 key 残留);
    // 收到 COT=8 停止激活时 abort 对应 handle,使 run_interrogation 在下一个 await 点被取消,
    // 不再继续上送数据帧或激活终止帧(IEC 60870-5-101 §7.2.6.1:去激活必须停止扫描)。
    let mut interrogation_tasks: std::collections::HashMap<(u16, u8), tokio::task::JoinHandle<()>> =
        std::collections::HashMap::new();

    // SBO select 状态(每连接独立):select → execute 配对与超时校验用。
    let mut select_state: SelectStateMap = SelectStateMap::new();

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

        // TCP/TLS 共用的流式 APDU 提取器同时处理分片与粘包。
        while let Some(data) = take_next_apdu(&mut reassembly_buf) {
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
                    // COT 字节布局:bit0..5=cause,bit6=negative-confirm,bit7=test(IEC 60870-5-101 §7.2.2.3)。
                    // 取低 6 位作 cause 比较,使主站叠加 T/PN 位的去激活(COT=8|0x80)仍能命中停止激活分支,
                    // 而非误入激活路径(与 decode.rs cot=byte&0x3F 一致)。
                    let cause = data[8] & 0x3F;
                let ca = u16::from_le_bytes([data[10], data[11]]);

                let ops_snapshot = remote_ops.read().await.clone();
                match asdu_type {
                    100 => {
                            // 去激活确认(COT=9)是管理层回复,须先于 answer_general_interrogation 抑制:
                            // 即使运营方关闭数据上送,停止激活仍需确认,否则主站 t1 超时。
                            if cause == 8 {
                                // 停止激活(COT=8):仅回停止确认(COT=9),不上送数据、不发激活终止。
                                // 取消该 CA 在途的 GI 扫描任务(若有),使其在下一个 await 点终止,
                                // 不再继续上送 COT=20 数据帧或 COT=10 激活终止帧。
                                if let Some(h) = interrogation_tasks.remove(&(ca, 20)) {
                                    h.abort();
                                }
                                let con = {
                                    let mut s = seq.lock().await;
                                    build_response_frame(
                                        &data[..n],
                                        ops_snapshot.cancel_ack_cot.as_u8(),
                                        &mut s,
                                    )
                                };
                                queue.lock().await.extend_from_slice(&con);
                                if let Some(ref lc) = log_collector {
                                    lc.try_add(LogEntry::new(
                                        Direction::Tx,
                                        FrameLabel::GeneralInterrogation,
                                        format!("GI 停止确认 CA={}", ca),
                                    ));
                                }
                            } else if !ops_snapshot.answer_general_interrogation {
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
                                    // 计数量召唤 (C_CI_NA_1) 上送,此处排除。控制方向
                                    // 点位不属于监视信息,同样排除。
                                    let pts: Vec<DataPoint> = station.data_points.all_sorted()
                                        .into_iter()
                                        .filter(|p| !matches!(p.value, DataPointValue::IntegratedTotal { .. })
                                            && !p.asdu_type.category().is_control())
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
                                    // 覆盖同 CA 仍在途的旧 GI 任务(刚完成则 abort 是 no-op)。
                                    if let Some(h) = interrogation_tasks.remove(&(ca, 20)) {
                                        h.abort();
                                    }
                                    let handle = tokio::spawn(async move {
                                    run_interrogation(
                                        points_snapshot, ca_bytes, 20,
                                        recv_template, include_ts,
                                        queue_clone, seq_clone, k, lc_clone,
                                        FrameLabel::GeneralInterrogation, ca,
                                    ).await;
                                });
                                    interrogation_tasks.insert((ca, 20), handle);
                                }
                        }
                    }
                    101 => {
                            if cause == 8 {
                                // 停止激活(COT=8):仅回停止确认(COT=9),不上送累计量、不发激活终止。
                                // 去激活确认先于 answer_counter_interrogation 抑制;取消在途扫描任务(若有)。
                                if let Some(h) = interrogation_tasks.remove(&(ca, 37)) {
                                    h.abort();
                                }
                                let con = {
                                    let mut s = seq.lock().await;
                                    build_response_frame(
                                        &data[..n],
                                        ops_snapshot.cancel_ack_cot.as_u8(),
                                        &mut s,
                                    )
                                };
                                queue.lock().await.extend_from_slice(&con);
                                if let Some(ref lc) = log_collector {
                                    lc.try_add(LogEntry::new(
                                        Direction::Tx,
                                        FrameLabel::CounterInterrogation,
                                        format!("累计量召唤 停止确认 CA={}", ca),
                                    ));
                                }
                            } else if !ops_snapshot.answer_counter_interrogation {
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
                                    // 覆盖同 CA 仍在途的旧计数量召唤任务(刚完成则 abort 是 no-op)。
                                    if let Some(h) = interrogation_tasks.remove(&(ca, 37)) {
                                        h.abort();
                                    }
                                    let handle = tokio::spawn(async move {
                                    run_interrogation(
                                        points_snapshot, ca_bytes, 37,
                                        recv_template, include_ts,
                                        queue_clone, seq_clone, k, lc_clone,
                                        FrameLabel::CounterInterrogation, ca,
                                    ).await;
                                });
                                    interrogation_tasks.insert((ca, 37), handle);
                                }
                        }
                    }
                    103 => {
                            // 合法 COT=6 且开关关闭时回否定 ACT_CON(7+P/N)；
                            // 非法 COT 回 45+P/N，IOA!=0 回 47+P/N。截断/畸形
                            // ASDU 无法安全回显，直接丢弃并记日志。
                            let decision = decide_clock_sync(
                                &data[..n],
                                ops_snapshot.answer_clock_sync,
                            );
                            if let Some(ack_cot) = decision.response_cot() {
                                let ack = {
                                    let mut s = seq.lock().await;
                                    build_response_frame(&data[..n], ack_cot, &mut s)
                                };
                                queue.lock().await.extend_from_slice(&ack);
                            }
                            if let Some(ref lc) = log_collector {
                                lc.try_add(clock_sync_log_entry(decision, ca, data.len()));
                            }
                    }
                    45..=51 | 58..=64 => {
                        match parse_control_command(asdu_type, &data[..n]) {
                            Some(cmd) => {
                                let outcome = process_control_command(
                                    &data[..n], cause, ca, cmd,
                                    &stations, &ops_snapshot, &seq, &mut select_state,
                                ).await;
                                if !outcome.replies.is_empty() {
                                    let mut q = queue.lock().await;
                                    for f in &outcome.replies { q.extend_from_slice(f); }
                                }
                                if let Some(ref lc) = log_collector {
                                    for e in outcome.log_entries { lc.try_add(e); }
                                }
                                if let Some((tca, pair)) = outcome.spontaneous {
                                    do_queue_spontaneous(
                                        &stations, &connections, &remote_ops, &log_collector,
                                        tca, &[pair],
                                    ).await;
                                }
                            }
                            None => {
                                // 已知控制类型但帧长不足:无法安全回显,丢弃并记日志。
                                if let Some(ref lc) = log_collector {
                                    lc.try_add(LogEntry::new(
                                        Direction::Rx, FrameLabel::IFrame(format!("Type{}", asdu_type)),
                                        format!("Malformed control frame type={} len={} CA={}", asdu_type, data.len(), ca),
                                    ).with_detail_event("cmdMalformed", serde_json::json!({
                                        "type": asdu_type, "len": data.len(), "ca": ca,
                                    })));
                                }
                            }
                        }
                    }
                    _ => {
                        // 未知 ASDU 类型:回 COT=44(unknown type)+ P/N 否定确认,而非静默
                        // 丢弃——静默会让主站 t1 超时(issue #28 主站侧 NG/超时来源之一)。
                        if ops_snapshot.answer_commands {
                            let ack = { let mut s = seq.lock().await; build_response_frame(&data[..n], 44 | 0x40, &mut s) };
                            queue.lock().await.extend_from_slice(&ack);
                        }
                        if let Some(ref lc) = log_collector {
                            lc.try_add(LogEntry::new(
                                Direction::Tx, FrameLabel::IFrame(format!("Type{}", asdu_type)),
                                format!("Unknown ASDU type={} rejected (COT=44 negative) CA={} COT={}", asdu_type, ca, cause),
                            ).with_detail_event("unknownType", serde_json::json!({
                                "type": asdu_type, "ca": ca, "cot": cause,
                            })));
                        }
                    }
                }
            }
        }
        } // end while reassembly_buf
    }

    // 连接断开:取消所有在途召唤扫描任务,避免句柄泄漏与对已关闭连接的后续上送。
    for (_, h) in interrogation_tasks.drain() {
        h.abort();
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
    let mut reassembly_buf: Vec<u8> = Vec::with_capacity(1024);

    // Cache the runtime handle once — this function always runs inside spawn_blocking.
    let rt = tokio::runtime::Handle::current();

    // SBO select 状态(每连接独立)。
    let mut select_state: SelectStateMap = SelectStateMap::new();

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

        reassembly_buf.extend_from_slice(&buf[..n]);

        // TLS 与明文 TCP 一样是字节流：一轮 read 可含半帧、一帧或多帧。
        while let Some(data) = take_next_apdu(&mut reassembly_buf) {
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
                rt.block_on(async { let mut s = seq.lock().await; observe_recv_iframe(&mut s, &data); });
                let asdu_type = data[6];
                // COT 字节布局:bit0..5=cause,bit6=negative-confirm,bit7=test(IEC 60870-5-101 §7.2.2.3)。
                // 取低 6 位作 cause 比较,使主站叠加 T/PN 位的去激活(COT=8|0x80)仍能命中停止激活分支,
                // 而非误入激活路径(与 decode.rs cot=byte&0x3F 一致)。
                let cause = data[8] & 0x3F;
                let ca = u16::from_le_bytes([data[10], data[11]]);

                let ops_snapshot = rt.block_on(async { remote_ops.read().await.clone() });
                match asdu_type {
                    100 => {
                        // 去激活确认先于 answer_general_interrogation 抑制:停止激活须确认,与数据上送开关无关。
                        if cause == 8 {
                            // 停止激活(COT=8):仅回停止确认(COT=9),不上送数据、不发激活终止。
                            let ack = rt.block_on(async {
                                let mut s = seq.lock().await;
                                build_response_frame(
                                    &data[..n],
                                    ops_snapshot.cancel_ack_cot.as_u8(),
                                    &mut s,
                                )
                            });
                            let _ = stream.write_all(&ack);
                            if let Some(ref lc) = log_collector {
                                lc.try_add(LogEntry::new(
                                    Direction::Tx,
                                    FrameLabel::GeneralInterrogation,
                                    format!("GI 停止确认 CA={}", ca),
                                ));
                            }
                        } else if !ops_snapshot.answer_general_interrogation {
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
                                send_gi_response_blocking(stream, station, &seq_cl, &ops_for_send,
                                    ).await;
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
                        // 去激活确认先于 answer_counter_interrogation 抑制:停止激活须确认。
                        if cause == 8 {
                            // 停止激活(COT=8):仅回停止确认(COT=9),不上送累计量、不发激活终止。
                            let ack = rt.block_on(async {
                                let mut s = seq.lock().await;
                                build_response_frame(
                                    &data[..n],
                                    ops_snapshot.cancel_ack_cot.as_u8(),
                                    &mut s,
                                )
                            });
                            let _ = stream.write_all(&ack);
                            if let Some(ref lc) = log_collector {
                                lc.try_add(LogEntry::new(
                                    Direction::Tx,
                                    FrameLabel::CounterInterrogation,
                                    format!("累计量召唤 停止确认 CA={}", ca),
                                ));
                            }
                        } else if !ops_snapshot.answer_counter_interrogation {
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
                            batch.extend_from_slice(&build_response_frame(&data[..n], 10, &mut s,
                                ));
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
                        // 与明文路径共用同一结构/COT/IOA/开关决策。
                        let decision = decide_clock_sync(
                            &data[..n],
                            ops_snapshot.answer_clock_sync,
                        );
                        if let Some(ack_cot) = decision.response_cot() {
                            let ack = rt.block_on(async {
                                let mut s = seq.lock().await;
                                build_response_frame(&data[..n], ack_cot, &mut s)
                            });
                            let _ = stream.write_all(&ack);
                        }
                        if let Some(ref lc) = log_collector {
                            lc.try_add(clock_sync_log_entry(decision, ca, data.len()));
                        }
                    }
                    45..=51 | 58..=64 => {
                        match parse_control_command(asdu_type, &data[..n]) {
                            Some(cmd) => {
                                let outcome = rt.block_on(process_control_command(
                                    &data[..n], cause, ca, cmd,
                                    &stations, &ops_snapshot, &seq, &mut select_state,
                                ));
                                for f in &outcome.replies { let _ = stream.write_all(f); }
                                if let Some(ref lc) = log_collector {
                                    for e in outcome.log_entries { lc.try_add(e); }
                                }
                                if let Some((tca, pair)) = outcome.spontaneous {
                                    rt.block_on(do_queue_spontaneous(
                                        &stations, &connections, &remote_ops, &log_collector,
                                        tca, &[pair],
                                    ));
                                }
                            }
                            None => {
                                // 已知控制类型但帧长不足:无法安全回显,丢弃并记日志。
                                if let Some(ref lc) = log_collector {
                                    lc.try_add(LogEntry::new(
                                        Direction::Rx, FrameLabel::IFrame(format!("Type{}", asdu_type)),
                                        format!("Malformed control frame type={} len={} CA={}", asdu_type, data.len(), ca),
                                    ).with_detail_event("cmdMalformed", serde_json::json!({
                                        "type": asdu_type, "len": data.len(), "ca": ca,
                                    })));
                                }
                            }
                        }
                    }
                    _ => {
                        // 未知 ASDU 类型:回 COT=44(unknown type)+ P/N 否定确认(与异步路径一致)。
                        if ops_snapshot.answer_commands {
                            let ack = rt.block_on(async { let mut s = seq.lock().await; build_response_frame(&data[..n], 44 | 0x40, &mut s) });
                            let _ = stream.write_all(&ack);
                        }
                        if let Some(ref lc) = log_collector {
                            lc.try_add(LogEntry::new(
                                Direction::Tx, FrameLabel::IFrame(format!("Type{}", asdu_type)),
                                format!("Unknown ASDU type={} rejected (COT=44 negative) CA={} COT={}", asdu_type, ca, cause),
                            ).with_detail_event("unknownType", serde_json::json!({
                                "type": asdu_type, "ca": ca, "cot": cause,
                            })));
                        }
                    }
                }
            }
        }
        } // end while complete TLS APDUs
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
            // 控制方向点位不属于监视信息,同样排除。
            if matches!(point.value, DataPointValue::IntegratedTotal { .. }) { continue; }
            if point.asdu_type.category().is_control() { continue; }
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
    let ioa_bytes = point.ioa.to_le_bytes();
    // CP24Time2a (TA) 监视类型:按自身 TypeID 输出 3 字节短时标。
    // 不参与 NA→TB 派生;force_timestamped=Some(false)(如 GI 不含时标)时
    // 落到下方 NA 分支,以 NA 基类型上送。
    if point.asdu_type.is_cp24() && force_timestamped != Some(false) {
        let ts = point.timestamp.unwrap_or_else(chrono::Utc::now);
        let ts_bytes = crate::asdu_encode::encode_cp24time2a(ts, point.quality.iv);
        value_bytes.extend_from_slice(&ts_bytes);
        return build_i_frame(point.asdu_type as u8, cot, ca, &ioa_bytes[..3], &value_bytes, seq);
    }
    let want_tb = match force_timestamped {
        Some(b) => b,
        None => point.asdu_type.is_timestamped(),
    };
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
    // CP24 (TA) 段:按存储类型自身的 TypeID 打包,时标用 3 字节 CP24Time2a。
    // 上游按 asdu_type 分段,这里再校验一次同型,防御混型段。
    let cp24 = points[0].asdu_type.is_cp24();
    if cp24 && points.iter().any(|p| p.asdu_type != points[0].asdu_type) {
        return None;
    }
    let final_type = if cp24 {
        points[0].asdu_type as u8
    } else if timestamped {
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
        if cp24 {
            let ts = p.timestamp.unwrap_or_else(chrono::Utc::now);
            let ts_bytes = crate::asdu_encode::encode_cp24time2a(ts, p.quality.iv);
            value_section.extend_from_slice(&ts_bytes);
        } else if timestamped {
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
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct MutationParams {
    pub mode: MutationMode,
    pub step: f64,
    pub min: f64,
    pub max: f64,
}

fn normalize_mutation_params(
    asdu_type: AsduTypeId,
    mut params: MutationParams,
) -> MutationParams {
    let category = asdu_type.category();
    if !matches!(
        category,
        DataCategory::NormalizedMeasured
            | DataCategory::ScaledMeasured
            | DataCategory::FloatMeasured
            | DataCategory::IntegratedTotals
    ) {
        params.mode = MutationMode::Flip;
    }

    params.step = if params.step.is_finite() {
        params.step.abs()
    } else {
        0.0
    };
    if !params.min.is_finite() {
        params.min = 0.0;
    }
    if !params.max.is_finite() {
        params.max = params.min;
    }
    (params.min, params.max) = (
        params.min.min(params.max),
        params.min.max(params.max),
    );

    let limits = match category {
        DataCategory::NormalizedMeasured => Some((-1.0, 1.0)),
        DataCategory::ScaledMeasured => Some((i16::MIN as f64, i16::MAX as f64)),
        DataCategory::FloatMeasured => Some((f32::MIN as f64, f32::MAX as f64)),
        DataCategory::IntegratedTotals => Some((i32::MIN as f64, i32::MAX as f64)),
        _ => None,
    };
    if let Some((lo, hi)) = limits {
        params.min = params.min.clamp(lo, hi);
        params.max = params.max.clamp(lo, hi);
    }
    params
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
                if asdu_type.category().is_control() { continue; }
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
                    if point.asdu_type.category().is_control() { continue; }
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
    /// 结构化 bind 失败:app 层据此给出可操作的本地化指引
    /// (端口被占 / 系统保留段 / 权限或安全软件拦截)。
    #[error("bind error: Failed to bind {addr}: {message}")]
    BindFailed {
        /// 机器可读分类:addr_in_use | access_denied | addr_not_available | bind_failed
        code: &'static str,
        addr: String,
        /// OS 原始错误码(如 Windows 10013/10048),未知时为 0。
        os_error: i32,
        message: String,
    },
    #[error("TLS error: {0}")] TlsError(String),
}

/// 把 bind 失败分类为结构化错误。Windows 上 10048(WSAEADDRINUSE)=端口被占,
/// 10013(WSAEACCES)=访问被拒——后者通常是 Hyper-V/WSL2 排除端口段、其他进程
/// 独占绑定(SO_EXCLUSIVEADDRUSE)或安全软件拦截,而非普通端口占用。
fn classify_bind_error(addr: &str, e: &std::io::Error) -> SlaveError {
    let code = match e.kind() {
        std::io::ErrorKind::AddrInUse => "addr_in_use",
        std::io::ErrorKind::PermissionDenied => "access_denied",
        std::io::ErrorKind::AddrNotAvailable => "addr_not_available",
        _ => match e.raw_os_error() {
            Some(10013) => "access_denied",
            Some(10048) => "addr_in_use",
            Some(10049) => "addr_not_available",
            _ => "bind_failed",
        },
    };
    SlaveError::BindFailed {
        code,
        addr: addr.to_string(),
        os_error: e.raw_os_error().unwrap_or(0),
        message: e.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data_point::ControlTarget;

    fn control_command(type_id: AsduTypeId, ioa: u32, value: CommandValue) -> ParsedControlCommand {
        ParsedControlCommand {
            type_id,
            ioa,
            value,
            is_select: false,
            qu: 0,
            frame_label: FrameLabel::SingleCommand,
        }
    }

    fn control_frame(type_id: AsduTypeId, ca: u16, ioa: u32) -> Vec<u8> {
        let mut frame = vec![0u8; 16];
        frame[0] = 0x68;
        frame[1] = 14;
        frame[6] = type_id as u8;
        frame[7] = 1;
        frame[8] = 6;
        frame[10..12].copy_from_slice(&ca.to_le_bytes());
        frame[12..15].copy_from_slice(&ioa.to_le_bytes()[..3]);
        frame
    }

    #[tokio::test(start_paused = true)]
    async fn point_mutation_delays_instead_of_bursting_missed_ticks() {
        let server = SlaveServer::new(SlaveTransportConfig::default());
        let mut station = Station::new(1, "test");
        station.add_point(InformationObjectDef {
            ioa: 1,
            asdu_type: AsduTypeId::MSpNa1,
            category: DataCategory::SinglePoint,
            name: String::new(),
            comment: String::new(),
            mapping: None,
            command_qualifier: None,
            select_before_operate: None,
        }).unwrap();
        server.add_station(station).await.unwrap();

        let initial_seq = server.stations.read().await
            .get(&1).unwrap().data_points.current_seq();
        // Block the spontaneous-send phase after the first mutation. Advancing
        // across two more deadlines reproduces a temporarily delayed task.
        let remote_ops_guard = server.remote_ops.write().await;
        server.start_point_mutation(
            1,
            1,
            AsduTypeId::MSpNa1,
            100,
            MutationParams::default(),
        ).await;
        tokio::task::yield_now().await;
        assert_eq!(
            server.list_point_mutations_with_period().await,
            vec![(1, 1, AsduTypeId::MSpNa1, MutationMode::Flip, 100)],
        );
        assert_eq!(
            server.list_point_mutations_with_params().await,
            vec![(
                1,
                1,
                AsduTypeId::MSpNa1,
                MutationParams::default(),
                100,
            )],
        );

        tokio::time::advance(std::time::Duration::from_millis(100)).await;
        tokio::task::yield_now().await;
        assert_eq!(
            server.stations.read().await.get(&1).unwrap().data_points.current_seq(),
            initial_seq + 1,
            "the first scheduled mutation should run",
        );

        tokio::time::advance(std::time::Duration::from_millis(250)).await;
        drop(remote_ops_guard);
        for _ in 0..4 {
            tokio::task::yield_now().await;
        }
        assert_eq!(
            server.stations.read().await.get(&1).unwrap().data_points.current_seq(),
            initial_seq + 2,
            "a delayed task should run one overdue mutation, not replay every missed tick",
        );

        tokio::time::advance(std::time::Duration::from_millis(99)).await;
        tokio::task::yield_now().await;
        assert_eq!(
            server.stations.read().await.get(&1).unwrap().data_points.current_seq(),
            initial_seq + 2,
            "Delay should preserve a full interval after recovery",
        );
        tokio::time::advance(std::time::Duration::from_millis(1)).await;
        tokio::task::yield_now().await;
        assert_eq!(
            server.stations.read().await.get(&1).unwrap().data_points.current_seq(),
            initial_seq + 3,
        );

        server.stop_point_mutation(1, 1, AsduTypeId::MSpNa1).await;
    }

    #[tokio::test]
    async fn point_mutation_reports_effective_parameters() {
        let server = SlaveServer::new(SlaveTransportConfig::default());
        server
            .start_point_mutation(
                1,
                9,
                AsduTypeId::MMeNa1,
                20,
                MutationParams {
                    mode: MutationMode::Increment,
                    step: -0.25,
                    min: 5.0,
                    max: -5.0,
                },
            )
            .await;
        assert_eq!(
            server.list_point_mutations_with_params().await,
            vec![(
                1,
                9,
                AsduTypeId::MMeNa1,
                MutationParams {
                    mode: MutationMode::Increment,
                    step: 0.25,
                    min: -1.0,
                    max: 1.0,
                },
                50,
            )],
        );

        server
            .start_point_mutation(
                1,
                1,
                AsduTypeId::MSpNa1,
                1000,
                MutationParams {
                    mode: MutationMode::Decrement,
                    step: -2.0,
                    min: 0.0,
                    max: 1.0,
                },
            )
            .await;
        let mut active = server.list_point_mutations_with_params().await;
        active.sort_by_key(|(_, ioa, _, _, _)| *ioa);
        assert_eq!(active[0].3.mode, MutationMode::Flip);
        assert_eq!(active[0].3.step, 2.0);
    }

    #[tokio::test(start_paused = true)]
    async fn point_mutation_resets_deadline_after_station_lock_delay() {
        let server = SlaveServer::new(SlaveTransportConfig::default());
        let mut station = Station::new(1, "test");
        station.add_point(InformationObjectDef {
            ioa: 1,
            asdu_type: AsduTypeId::MSpNa1,
            category: DataCategory::SinglePoint,
            name: String::new(),
            comment: String::new(),
            mapping: None,
            command_qualifier: None,
            select_before_operate: None,
        }).unwrap();
        server.add_station(station).await.unwrap();

        let initial_seq = server.stations.read().await
            .get(&1).unwrap().data_points.current_seq();
        let stations_guard = server.stations.write().await;
        server.start_point_mutation(
            1,
            1,
            AsduTypeId::MSpNa1,
            100,
            MutationParams::default(),
        ).await;
        tokio::task::yield_now().await;

        // The first tick is consumed at 100ms, then waits on stations.write()
        // while two more deadlines pass.
        tokio::time::advance(std::time::Duration::from_millis(350)).await;
        drop(stations_guard);
        for _ in 0..4 {
            tokio::task::yield_now().await;
        }
        assert_eq!(
            server.stations.read().await.get(&1).unwrap().data_points.current_seq(),
            initial_seq + 1,
            "unlocking the station must not trigger a back-to-back overdue mutation",
        );

        tokio::time::advance(std::time::Duration::from_millis(99)).await;
        tokio::task::yield_now().await;
        assert_eq!(
            server.stations.read().await.get(&1).unwrap().data_points.current_seq(),
            initial_seq + 1,
        );
        tokio::time::advance(std::time::Duration::from_millis(1)).await;
        tokio::task::yield_now().await;
        assert_eq!(
            server.stations.read().await.get(&1).unwrap().data_points.current_seq(),
            initial_seq + 2,
        );

        server.stop_point_mutation(1, 1, AsduTypeId::MSpNa1).await;
    }

    #[test]
    fn parses_every_control_type_45_through_51_and_58_through_64() {
        let cases = [
            (AsduTypeId::CScNa1, 16),
            (AsduTypeId::CDcNa1, 16),
            (AsduTypeId::CRcNa1, 16),
            (AsduTypeId::CSeNa1, 18),
            (AsduTypeId::CSeNb1, 18),
            (AsduTypeId::CSeNc1, 20),
            (AsduTypeId::CBoNa1, 19),
            (AsduTypeId::CScTa1, 23),
            (AsduTypeId::CDcTa1, 23),
            (AsduTypeId::CRcTa1, 23),
            (AsduTypeId::CSeTa1, 25),
            (AsduTypeId::CSeTb1, 25),
            (AsduTypeId::CSeTc1, 27),
            (AsduTypeId::CBoTa1, 26),
        ];
        for (type_id, len) in cases {
            let mut frame = vec![0u8; len];
            frame[6] = type_id as u8;
            frame[12..15].copy_from_slice(&0x010203u32.to_le_bytes()[..3]);
            let parsed = parse_control_command(type_id as u8, &frame)
                .unwrap_or_else(|| panic!("{} should parse", type_id.name()));
            assert_eq!(parsed.type_id, type_id);
            assert_eq!(parsed.ioa, 0x010203);
        }
    }

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

    #[tokio::test]
    async fn explicit_control_mappings_are_independent_across_ca_ioa_and_type() {
        let mut source = Station::new(1, "controls");
        source
            .add_point(InformationObjectDef {
                ioa: 10,
                asdu_type: AsduTypeId::CScNa1,
                category: DataCategory::SingleCommand,
                name: String::new(),
                comment: String::new(),
                mapping: Some(ControlTarget {
                    common_address: 2,
                    ioa: 20,
                    asdu_type: AsduTypeId::MSpNa1,
                }),
                command_qualifier: None,
                select_before_operate: None,
            })
            .unwrap();
        source
            .add_point(InformationObjectDef {
                ioa: 10,
                asdu_type: AsduTypeId::CScTa1,
                category: DataCategory::SingleCommand,
                name: String::new(),
                comment: String::new(),
                mapping: Some(ControlTarget {
                    common_address: 2,
                    ioa: 21,
                    asdu_type: AsduTypeId::MSpTb1,
                }),
                command_qualifier: None,
                select_before_operate: None,
            })
            .unwrap();

        let mut target = Station::new(2, "monitoring");
        for (ioa, asdu_type) in [(20, AsduTypeId::MSpNa1), (21, AsduTypeId::MSpTb1)] {
            target
                .add_point(InformationObjectDef {
                    ioa,
                    asdu_type,
                    category: DataCategory::SinglePoint,
                    name: String::new(),
                    comment: String::new(),
                    mapping: None,
                    command_qualifier: None,
                    select_before_operate: None,
                })
                .unwrap();
        }

        let stations = Arc::new(RwLock::new(HashMap::from([(1, source), (2, target)])));
        let seq = Arc::new(tokio::sync::Mutex::new(SeqState::default()));
        let mut selections = SelectStateMap::new();
        let ops = RemoteOperationConfig::default();

        let first = process_control_command(
            &control_frame(AsduTypeId::CScNa1, 1, 10),
            6,
            1,
            control_command(AsduTypeId::CScNa1, 10, CommandValue::Single(true)),
            &stations,
            &ops,
            &seq,
            &mut selections,
        )
        .await;
        assert_eq!(first.spontaneous, Some((2, (20, AsduTypeId::MSpNa1))));
        {
            let guard = stations.read().await;
            assert_eq!(
                guard.get(&2).unwrap().data_points.get(20, AsduTypeId::MSpNa1).unwrap().value,
                DataPointValue::SinglePoint { value: true }
            );
            assert_eq!(
                guard.get(&2).unwrap().data_points.get(21, AsduTypeId::MSpTb1).unwrap().value,
                DataPointValue::SinglePoint { value: false }
            );
        }

        let second = process_control_command(
            &control_frame(AsduTypeId::CScTa1, 1, 10),
            6,
            1,
            control_command(AsduTypeId::CScTa1, 10, CommandValue::Single(true)),
            &stations,
            &ops,
            &seq,
            &mut selections,
        )
        .await;
        assert_eq!(second.spontaneous, Some((2, (21, AsduTypeId::MSpTb1))));
        let guard = stations.read().await;
        assert_eq!(
            guard.get(&2).unwrap().data_points.get(21, AsduTypeId::MSpTb1).unwrap().value,
            DataPointValue::SinglePoint { value: true }
        );
    }

    #[tokio::test]
    async fn declared_unmapped_command_acknowledges_but_unknown_ioa_is_rejected() {
        let mut station = Station::new(1, "controls");
        station
            .add_point(InformationObjectDef {
                ioa: 10,
                asdu_type: AsduTypeId::CScNa1,
                category: DataCategory::SingleCommand,
                name: String::new(),
                comment: String::new(),
                mapping: None,
                command_qualifier: None,
                select_before_operate: None,
            })
            .unwrap();
        let stations = Arc::new(RwLock::new(HashMap::from([(1, station)])));
        let seq = Arc::new(tokio::sync::Mutex::new(SeqState::default()));
        let mut selections = SelectStateMap::new();
        let ops = RemoteOperationConfig::default();

        let accepted = process_control_command(
            &control_frame(AsduTypeId::CScNa1, 1, 10),
            6,
            1,
            control_command(AsduTypeId::CScNa1, 10, CommandValue::Single(true)),
            &stations,
            &ops,
            &seq,
            &mut selections,
        )
        .await;
        assert_eq!(accepted.replies.len(), 2);
        assert_eq!(accepted.replies[0][8] & 0x3f, 7);
        assert_eq!(accepted.replies[1][8] & 0x3f, 10);
        assert_eq!(
            stations.read().await.get(&1).unwrap().data_points.get(10, AsduTypeId::CScNa1).unwrap().value,
            DataPointValue::SinglePoint { value: true }
        );

        let rejected = process_control_command(
            &control_frame(AsduTypeId::CScNa1, 1, 999),
            6,
            1,
            control_command(AsduTypeId::CScNa1, 999, CommandValue::Single(true)),
            &stations,
            &ops,
            &seq,
            &mut selections,
        )
        .await;
        assert_eq!(rejected.replies.len(), 1);
        assert_eq!(rejected.replies[0][8] & 0x3f, 47);
        assert_ne!(rejected.replies[0][8] & 0x40, 0);
    }

    #[tokio::test]
    async fn sbo_enforcement_requires_matching_exact_type_select() {
        let mut station = Station::new(1, "controls");
        for type_id in [AsduTypeId::CScNa1, AsduTypeId::CScTa1] {
            station
                .add_point(InformationObjectDef {
                    ioa: 10,
                    asdu_type: type_id,
                    category: DataCategory::SingleCommand,
                    name: String::new(),
                    comment: String::new(),
                    mapping: None,
                    command_qualifier: None,
                    select_before_operate: None,
                })
                .unwrap();
        }
        let stations = Arc::new(RwLock::new(HashMap::from([(1, station)])));
        let seq = Arc::new(tokio::sync::Mutex::new(SeqState::default()));
        let mut selections = SelectStateMap::new();
        let ops = RemoteOperationConfig { sbo_enforce: true, ..Default::default() };

        let direct = process_control_command(
            &control_frame(AsduTypeId::CScNa1, 1, 10),
            6,
            1,
            control_command(AsduTypeId::CScNa1, 10, CommandValue::Single(true)),
            &stations,
            &ops,
            &seq,
            &mut selections,
        )
        .await;
        assert_eq!(direct.replies.len(), 1);
        assert_ne!(direct.replies[0][8] & 0x40, 0);

        let mut select = control_command(AsduTypeId::CScNa1, 10, CommandValue::Single(true));
        select.is_select = true;
        let selected = process_control_command(
            &control_frame(AsduTypeId::CScNa1, 1, 10),
            6,
            1,
            select,
            &stations,
            &ops,
            &seq,
            &mut selections,
        )
        .await;
        assert_eq!(selected.replies.len(), 1);
        assert_eq!(selected.replies[0][8] & 0x3f, 7);

        let wrong_type = process_control_command(
            &control_frame(AsduTypeId::CScTa1, 1, 10),
            6,
            1,
            control_command(AsduTypeId::CScTa1, 10, CommandValue::Single(true)),
            &stations,
            &ops,
            &seq,
            &mut selections,
        )
        .await;
        assert_eq!(wrong_type.replies.len(), 1);
        assert_ne!(wrong_type.replies[0][8] & 0x40, 0);

        let executed = process_control_command(
            &control_frame(AsduTypeId::CScNa1, 1, 10),
            6,
            1,
            control_command(AsduTypeId::CScNa1, 10, CommandValue::Single(true)),
            &stations,
            &ops,
            &seq,
            &mut selections,
        )
        .await;
        assert_eq!(executed.replies.len(), 2);
        assert_eq!(
            stations.read().await.get(&1).unwrap().data_points.get(10, AsduTypeId::CScNa1).unwrap().value,
            DataPointValue::SinglePoint { value: true }
        );
    }

    #[tokio::test]
    async fn per_point_qoc_and_sbo_options_are_enforced() {
        let mut station = Station::new(1, "controls");
        station
            .add_point(InformationObjectDef {
                ioa: 10,
                asdu_type: AsduTypeId::CScNa1,
                category: DataCategory::SingleCommand,
                name: String::new(),
                comment: String::new(),
                mapping: None,
                command_qualifier: Some(2),
                select_before_operate: Some(true),
            })
            .unwrap();
        let stations = Arc::new(RwLock::new(HashMap::from([(1, station)])));
        let seq = Arc::new(tokio::sync::Mutex::new(SeqState::default()));
        let mut selections = SelectStateMap::new();
        let ops = RemoteOperationConfig::default();

        let wrong_q = process_control_command(
            &control_frame(AsduTypeId::CScNa1, 1, 10),
            6,
            1,
            control_command(AsduTypeId::CScNa1, 10, CommandValue::Single(true)),
            &stations, &ops, &seq, &mut selections,
        ).await;
        assert_eq!(wrong_q.replies.len(), 1);
        assert_ne!(wrong_q.replies[0][8] & 0x40, 0);

        let mut select = control_command(AsduTypeId::CScNa1, 10, CommandValue::Single(true));
        select.qu = 2;
        select.is_select = true;
        let selected = process_control_command(
            &control_frame(AsduTypeId::CScNa1, 1, 10),
            6, 1, select, &stations, &ops, &seq, &mut selections,
        ).await;
        assert_eq!(selected.replies.len(), 1);

        let mut execute = control_command(AsduTypeId::CScNa1, 10, CommandValue::Single(true));
        execute.qu = 2;
        let executed = process_control_command(
            &control_frame(AsduTypeId::CScNa1, 1, 10),
            6, 1, execute, &stations, &ops, &seq, &mut selections,
        ).await;
        assert_eq!(executed.replies.len(), 2);
        assert_eq!(
            stations.read().await.get(&1).unwrap().data_points.get(10, AsduTypeId::CScNa1).unwrap().value,
            DataPointValue::SinglePoint { value: true }
        );
    }

    // 回归:停止服务器必须立即返回。除周期任务外，Windows 不能连接
    // 0.0.0.0 通配地址来唤醒 listener.accept()；停止逻辑必须直接取消 accept
    // 任务，并确保监听 socket 已释放、同端口可以再次启动。
    #[tokio::test]
    async fn test_stop_returns_promptly_after_start() {
        // 使用产品默认的通配监听地址，覆盖 Windows 上无法回连 0.0.0.0 的路径。
        let port = {
            let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            l.local_addr().unwrap().port()
        };
        let transport = SlaveTransportConfig {
            bind_address: "0.0.0.0".to_string(),
            port,
            tls: SlaveTlsConfig::default(),
        };
        let mut server = SlaveServer::new(transport);
        server.start().await.expect("start should succeed");

        // Keep an established plain TCP reader blocked while stopping. The server must
        // own and abort that reader task instead of waiting for the peer to disconnect.
        let _client = tokio::net::TcpStream::connect(("127.0.0.1", port))
            .await
            .expect("client should connect");

        // 预热:让周期任务越过"立即触发"的首个 tick,park 在下一个 2s tick 上。
        // 真实场景中服务器已运行一段时间,停止时任务正卡在 interval.tick() 等待。
        tokio::time::sleep(std::time::Duration::from_millis(150)).await;

        tokio::time::timeout(std::time::Duration::from_millis(500), server.stop())
            .await
            .expect("stop() 应在 500ms 内返回，不能依赖回连 0.0.0.0 唤醒 accept")
            .expect("stop should succeed");

        // stop 返回即代表监听 socket 已释放；同一个 SlaveServer 必须可原端口重启。
        server.start().await.expect("restart on the same port should succeed");
        tokio::time::timeout(std::time::Duration::from_millis(500), server.stop())
            .await
            .expect("second stop should also return promptly")
            .expect("second stop should succeed");
    }

    #[test]
    fn test_add_point_coexist_different_type() {
        let mut station = Station::new(1, "Test");
        let def_sp = InformationObjectDef {
            ioa: 100,
            asdu_type: AsduTypeId::MSpNa1,
            category: DataCategory::SinglePoint,
            name: "SP".to_string(),
            comment: String::new(), mapping: None,
            command_qualifier: None, select_before_operate: None
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
            comment: String::new(), mapping: None,
            command_qualifier: None, select_before_operate: None
        };
        station.add_point(def_float).unwrap();
        assert_eq!(station.data_points.len(), 2); // both coexist
        assert!(station.data_points.get(100, AsduTypeId::MSpNa1).is_some());
        assert!(station.data_points.get(100, AsduTypeId::MMeNc1).is_some());
        assert_eq!(station.object_defs.len(), 2);
    }

    // issue #28:同 (IOA, 类型) 重复添加必须报错,不能静默覆盖已有定义。
    #[test]
    fn add_point_strict_rejects_same_ioa_and_type() {
        let mut station = Station::new(1, "Test");
        let def = |name: &str| InformationObjectDef {
            ioa: 100,
            asdu_type: AsduTypeId::MSpNa1,
            category: DataCategory::SinglePoint,
            name: name.to_string(),
            comment: "原备注".to_string(),
            mapping: None,
            command_qualifier: None,
            select_before_operate: None,
        };
        station.add_point_strict(def("原名称")).unwrap();

        let err = station.add_point_strict(def("覆盖者")).unwrap_err();
        assert!(matches!(err, SlaveError::DuplicateIoa(100)), "got {err:?}");
        // 已有定义分毫未动
        assert_eq!(station.object_defs.len(), 1);
        assert_eq!(station.object_defs[0].name, "原名称");
        assert_eq!(station.object_defs[0].comment, "原备注");
        assert_eq!(station.data_points.len(), 1);
    }

    // 同 IOA 不同类型(含同分类 NA/TB 变体、控制↔监视配对)仍然允许共存。
    #[test]
    fn add_point_strict_allows_same_ioa_other_types() {
        let mut station = Station::new(1, "Test");
        let def = |asdu_type: AsduTypeId, category: DataCategory| InformationObjectDef {
            ioa: 100,
            asdu_type,
            category,
            name: String::new(),
            comment: String::new(),
            mapping: None,
            command_qualifier: None,
            select_before_operate: None,
        };
        station.add_point_strict(def(AsduTypeId::MSpNa1, DataCategory::SinglePoint)).unwrap();
        station.add_point_strict(def(AsduTypeId::MSpTb1, DataCategory::SinglePoint)).unwrap();
        station.add_point_strict(def(AsduTypeId::MMeNc1, DataCategory::FloatMeasured)).unwrap();
        station.add_point_strict(def(AsduTypeId::CScNa1, DataCategory::SingleCommand)).unwrap();
        assert_eq!(station.object_defs.len(), 4);
        assert_eq!(station.data_points.len(), 4);
    }

    // 宽松 add_point 的覆盖语义必须保留:配置导入等幂等重放路径依赖它。
    #[test]
    fn add_point_keeps_overwrite_semantics_for_config_import() {
        let mut station = Station::new(1, "Test");
        let def = |name: &str| InformationObjectDef {
            ioa: 7,
            asdu_type: AsduTypeId::MSpNa1,
            category: DataCategory::SinglePoint,
            name: name.to_string(),
            comment: String::new(),
            mapping: None,
            command_qualifier: None,
            select_before_operate: None,
        };
        station.add_point(def("v1")).unwrap();
        station.add_point(def("v2")).unwrap();
        assert_eq!(station.object_defs.len(), 1);
        assert_eq!(station.object_defs[0].name, "v2");
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
    fn encode_point_frame_ex_cp24_point_emits_own_type_with_3byte_time() {
        // M_SP_TA_1 (type 2):按自身类型上送,时标 3 字节 CP24Time2a。
        let mut point = DataPoint::new(9, AsduTypeId::MSpTa1);
        point.value = DataPointValue::SinglePoint { value: true };
        let ca = 1u16.to_le_bytes();
        let mut seq = SeqState::default();
        let frame = encode_point_frame_ex(&point, 3, &ca, &mut seq, None);
        assert_eq!(frame[6], 2, "type=2 (M_SP_TA_1)");
        // 帧长:APCI 6 + ASDU头 6 + IOA 3 + SIQ 1 + CP24 3 = 19
        assert_eq!(frame.len(), 19, "SIQ + 3 字节 CP24");
        assert_eq!(frame[15] & 0x01, 0x01, "SIQ ON");
        // force_timestamped=Some(true) 仍按 CP24 自身类型(已带时标,不派生 TB)。
        let mut seq2 = SeqState::default();
        let frame2 = encode_point_frame_ex(&point, 20, &ca, &mut seq2, Some(true));
        assert_eq!(frame2[6], 2);
        assert_eq!(frame2.len(), 19);
        // force_timestamped=Some(false)(GI 不含时标)回退到 NA 基类型。
        let mut seq3 = SeqState::default();
        let frame3 = encode_point_frame_ex(&point, 20, &ca, &mut seq3, Some(false));
        assert_eq!(frame3[6], 1, "退回 M_SP_NA_1");
        assert_eq!(frame3.len(), 16);
    }

    #[test]
    fn encode_point_frame_ex_cp24_measured_layout() {
        // M_ME_TC_1 (type 14): float(4) + QDS(1) + CP24(3)。
        let mut point = DataPoint::new(7, AsduTypeId::MMeTc1);
        point.value = DataPointValue::ShortFloat { value: 1.25 };
        let ca = 1u16.to_le_bytes();
        let mut seq = SeqState::default();
        let frame = encode_point_frame_ex(&point, 3, &ca, &mut seq, None);
        assert_eq!(frame[6], 14, "type=14 (M_ME_TC_1)");
        assert_eq!(frame.len(), 6 + 6 + 3 + 4 + 1 + 3);
        let f = f32::from_le_bytes([frame[15], frame[16], frame[17], frame[18]]);
        assert!((f - 1.25).abs() < 1e-6);
    }

    #[test]
    fn encode_points_grouped_cp24_packs_own_type() {
        let mut p1 = DataPoint::new(10, AsduTypeId::MSpTa1);
        p1.value = DataPointValue::SinglePoint { value: true };
        let mut p2 = DataPoint::new(11, AsduTypeId::MSpTa1);
        p2.value = DataPointValue::SinglePoint { value: false };
        let ca = 1u16.to_le_bytes();
        let mut seq = SeqState::default();
        let refs = vec![&p1, &p2];
        let frame = encode_points_grouped(&refs, 3, &ca, &mut seq, true).expect("packs");
        assert_eq!(frame[6], 2, "SQ 帧类型保持 M_SP_TA_1");
        assert_eq!(frame[7], 0x80 | 2, "VSQ.SQ=1, n=2");
        // APCI 6 + ASDU头 6 + IOA 3 + 2*(SIQ 1 + CP24 3) = 23
        assert_eq!(frame.len(), 23);
    }

    #[test]
    fn should_derive_tb_is_false_for_cp24_points() {
        let map = DataPointMap::new();
        assert!(!should_derive_tb(&map, AsduTypeId::MSpTa1, 1), "TA 自带时标,不再派生 TB");
    }

    #[test]
    fn move_point_ioa_moves_def_and_runtime_value() {
        let mut station = Station::new(1, "Test");
        station.add_point(InformationObjectDef {
            ioa: 100,
            asdu_type: AsduTypeId::MSpNa1,
            category: DataCategory::SinglePoint,
            name: "SP".to_string(),
            comment: String::new(), mapping: None,
            command_qualifier: None, select_before_operate: None
        }).unwrap();
        station.data_points.get_mut(100, AsduTypeId::MSpNa1).unwrap().value =
            DataPointValue::SinglePoint { value: true };

        station.move_point_ioa(100, AsduTypeId::MSpNa1, 200).unwrap();
        assert!(station.data_points.get(100, AsduTypeId::MSpNa1).is_none(), "旧地址消失");
        let moved = station.data_points.get(200, AsduTypeId::MSpNa1).expect("新地址存在");
        assert!(matches!(moved.value, DataPointValue::SinglePoint { value: true }), "运行值保留");
        assert_eq!(station.object_defs[0].ioa, 200, "定义同步改址");
        assert_eq!(station.object_defs[0].name, "SP");
    }

    #[test]
    fn move_point_ioa_rejects_conflict_and_missing() {
        let mut station = Station::new(1, "Test");
        for ioa in [100u32, 200] {
            station.add_point(InformationObjectDef {
                ioa,
                asdu_type: AsduTypeId::MSpNa1,
                category: DataCategory::SinglePoint,
                name: String::new(),
                comment: String::new(), mapping: None,
                command_qualifier: None, select_before_operate: None
            }).unwrap();
        }
        assert!(station.move_point_ioa(100, AsduTypeId::MSpNa1, 200).is_err(), "目标已存在");
        assert!(station.move_point_ioa(999, AsduTypeId::MSpNa1, 300).is_err(), "源不存在");
        // 同 IOA 不同类型不冲突。
        station.add_point(InformationObjectDef {
            ioa: 100,
            asdu_type: AsduTypeId::MMeNc1,
            category: DataCategory::FloatMeasured,
            name: String::new(),
            comment: String::new(), mapping: None,
            command_qualifier: None, select_before_operate: None
        }).unwrap();
        station.move_point_ioa(100, AsduTypeId::MMeNc1, 200).expect("跨类型不冲突");
        assert!(station.data_points.get(200, AsduTypeId::MMeNc1).is_some());
    }

    #[test]
    fn batch_add_points_list_supports_gaps_qu_se_and_type_id_names() {
        let mut station = Station::new(1, "Test");
        let ioas = [6001u32, 6003, 6006, 6012];
        let created = station
            .batch_add_points_list(&ioas, AsduTypeId::CScNa1, "CMD", true, Some(2), Some(true))
            .unwrap();
        assert_eq!(created, 4);
        for ioa in ioas {
            let def = station.object_defs.iter()
                .find(|d| d.ioa == ioa && d.asdu_type == AsduTypeId::CScNa1)
                .expect("def created");
            assert_eq!(def.name, format!("CMD_45_{}", ioa), "prefix_typeid_ioa");
            assert_eq!(def.command_qualifier, Some(2), "QU 批量下发");
            assert_eq!(def.select_before_operate, Some(true), "SBO 批量下发");
            assert!(station.data_points.get(ioa, AsduTypeId::CScNa1).is_some());
        }
        // 重复添加同 IOA:跳过而非重复 def;返回实际新建数。
        let again = station
            .batch_add_points_list(&[6001, 6002], AsduTypeId::CScNa1, "", false, None, None)
            .unwrap();
        assert_eq!(again, 1, "6001 已存在被跳过,仅 6002 新建");
        assert_eq!(
            station.object_defs.iter().filter(|d| d.ioa == 6001).count(),
            1,
            "无重复定义"
        );
        // 已存在点的 QU/SE 不被后续批量添加覆盖。
        let d6001 = station.object_defs.iter()
            .find(|d| d.ioa == 6001 && d.asdu_type == AsduTypeId::CScNa1)
            .unwrap();
        assert_eq!(d6001.command_qualifier, Some(2));
        // 输入列表内的重复 IOA 只建一个。
        let dup = station
            .batch_add_points_list(&[7000, 7000, 7000], AsduTypeId::MSpNa1, "", false, None, None)
            .unwrap();
        assert_eq!(dup, 1);
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
    fn apdu_reassembly_handles_split_and_coalesced_frames() {
        let first = vec![0x68, 0x04, 0x07, 0, 0, 0];
        let second = vec![0x68, 0x04, 0x43, 0, 0, 0];
        let mut buffer = first[..3].to_vec();
        assert!(take_next_apdu(&mut buffer).is_none());

        buffer.extend_from_slice(&first[3..]);
        buffer.extend_from_slice(&second);
        assert_eq!(take_next_apdu(&mut buffer), Some(first));
        assert_eq!(take_next_apdu(&mut buffer), Some(second));
        assert!(take_next_apdu(&mut buffer).is_none());
    }

    #[test]
    fn clock_sync_decision_enforces_structure_before_policy() {
        let mut frame = vec![
            0x68, 0x14, 0, 0, 0, 0, 103, 0x01, 6, 0, 1, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0,
        ];
        assert_eq!(
            decide_clock_sync(&frame, false),
            ClockSyncDecision::Disabled
        );

        frame[8] = 8;
        assert_eq!(
            decide_clock_sync(&frame, false),
            ClockSyncDecision::UnknownCot { cot: 8 }
        );

        frame[8] = 6;
        frame[12] = 1;
        assert_eq!(
            decide_clock_sync(&frame, false),
            ClockSyncDecision::UnknownIoa { ioa: 1 }
        );

        frame[12] = 0;
        frame[7] = 0x81;
        assert_eq!(
            decide_clock_sync(&frame, false),
            ClockSyncDecision::Malformed { reason: "vsq" }
        );
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
