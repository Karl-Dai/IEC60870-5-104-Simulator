export interface ServerInfo {
  id: string
  bind_address: string
  port: number
  state: string
  station_count: number
}

export interface StationInfo {
  common_address: number
  name: string
  point_count: number
}

export interface DataPointInfo {
  ioa: number
  asdu_type: string
  category: string
  name: string
  comment: string
  mapping_common_address?: number | null
  mapping_ioa?: number | null
  mapping_asdu_type?: string | null
  command_qualifier?: number | null
  select_before_operate?: boolean | null
  value: string
  quality_ov: boolean
  quality_bl: boolean
  quality_sb: boolean
  quality_nt: boolean
  quality_iv: boolean
  timestamp: string | null
}

/** 测量类(OV 适用):M_ME_*。 */
export function isMeasured(asduType: string): boolean {
  return asduType.startsWith('M_ME')
}

/** Response of the incremental `list_data_points_since` command. */
export interface IncrementalDataResponse {
  /** Current sequence counter — pass back as `sinceSeq` next poll. */
  seq: number
  /** Backend's total point count — a mismatch vs the local cache means a delete. */
  total_count: number
  /** Only the points changed since the caller's `sinceSeq`. */
  points: DataPointInfo[]
}

export interface LogEntry {
  timestamp: string
  direction: string
  frame_label: { [key: string]: string } | string
  detail: string
  raw_bytes: number[] | null
  detail_event?: { kind: string; payload: Record<string, unknown> } | null
}

// Frame parser types now live in @shared/types/frame to avoid duplicate
// definitions between slave and master frontends.
export * from '@shared/types/frame'

// ---------------------------------------------------------------------------
// Remote operation configuration (远动运行参数)
// 对应 Rust 端 `iec104sim_core::slave::ProtocolTimingConfig` 等结构。
// ---------------------------------------------------------------------------

export interface ProtocolTimingConfig {
  t0: number
  t1: number
  t2: number
  t3: number
  k: number
  w: number
}

export type UploadMode = 'continuous' | 'discrete'

export type CommandAckCot = 'activation_con' | 'deactivation_con' | 'activation_termination'

export interface RandomMutationPacing {
  batch_size: number
  delay_ms: number
}

/** 按分类的「变位同步上送 TB」开关。变位/周期上送时,开启的分类会额外派生 TB 帧。
 *  累计量 (IT) 靠召唤上送,不提供此开关。字段名与后端 serde 对齐。 */
export interface SyncTbByCategory {
  sp: boolean
  dp: boolean
  st: boolean
  bo: boolean
  me_na: boolean
  me_nb: boolean
  me_nc: boolean
}

export interface RemoteOperationConfig {
  sync_tb_by_category: SyncTbByCategory
  answer_general_interrogation: boolean
  answer_counter_interrogation: boolean
  answer_commands: boolean
  gi_include_timestamped: boolean
  upload_mode_untimestamped: UploadMode
  upload_mode_timestamped: UploadMode
  select_ack_cot: CommandAckCot
  execute_ack_cot: CommandAckCot
  cancel_ack_cot: CommandAckCot
  random_pacing: RandomMutationPacing
  auto_packing: boolean
  auto_map_commands: boolean
  ack_unmapped_commands: boolean
  sbo_enforce: boolean
  sbo_timeout_ms: number
  answer_clock_sync: boolean
  send_act_term: boolean
}

/** 周期变位方式:翻转(两态振荡)/ 递增 / 递减(三角波)。 */
export type MutationMode = 'flip' | 'increment' | 'decrement'

/** list_point_mutations 返回项。用于数据表状态标记、动态刷新和模拟参数回显。 */
export interface PointMutationInfo {
  ioa: number
  asdu_type: string
  mode: MutationMode
  period_ms: number
  step: number
  min: number
  max: number
}

/** 模拟设置面板中的活动点；value 复用点表当前最新值。 */
export interface PointMutationRow extends PointMutationInfo {
  value: string
}

/** 活动变位点的轻量动态快照。静态定义字段继续沿用 DataPointInfo 缓存。 */
export interface DataPointValueSnapshot {
  ioa: number
  asdu_type: string
  value: string
  quality_ov: boolean
  quality_bl: boolean
  quality_sb: boolean
  quality_nt: boolean
  quality_iv: boolean
  timestamp: string | null
}

export const DEFAULT_PROTOCOL_TIMING: ProtocolTimingConfig = {
  t0: 30, t1: 15, t2: 10, t3: 20, k: 12, w: 8,
}

export const DEFAULT_REMOTE_OPS: RemoteOperationConfig = {
  sync_tb_by_category: { sp: false, dp: false, st: false, bo: false, me_na: false, me_nb: false, me_nc: false },
  answer_general_interrogation: true,
  answer_counter_interrogation: true,
  answer_commands: true,
  gi_include_timestamped: false,
  upload_mode_untimestamped: 'discrete',
  upload_mode_timestamped: 'discrete',
  select_ack_cot: 'activation_con',
  execute_ack_cot: 'activation_termination',
  cancel_ack_cot: 'deactivation_con',
  random_pacing: { batch_size: 2000, delay_ms: 50 },
  auto_packing: false,
  // New servers use explicit control-to-monitor mappings. The Rust serde
  // default remains true only for legacy config files that predate this field.
  auto_map_commands: false,
  ack_unmapped_commands: true,
  sbo_enforce: false,
  sbo_timeout_ms: 30000,
  answer_clock_sync: true,
  send_act_term: true,
}
