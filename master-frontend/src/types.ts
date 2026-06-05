// Per-connection per-CA flash & count maps shared via provide/inject. Aliased
// so the three consumers (App.vue / ConnectionTree.vue / DataTable.vue) don't
// hand-repeat the nested generic.
export type ChangedCategoriesMap = Map<string, Map<number, Set<string>>>
export type CategoryCountsMap = Map<string, Map<number, Map<string, number>>>

export interface ConnectionInfo {
  id: string
  target_address: string
  port: number
  /** All Common Addresses configured for this connection. Always non-empty. */
  common_addresses: number[]
  state: string
  use_tls: boolean
  /** TLS file paths / policy echoed back from the backend (authoritative
   *  source), so the edit dialog pre-fills a connection's real cert paths
   *  instead of a shared localStorage blob. Empty when TLS is disabled. */
  ca_file: string
  cert_file: string
  key_file: string
  accept_invalid_certs: boolean
  tls_version: 'auto' | 'tls12_only' | 'tls13_only'
  /** IEC 60870-5-104 protocol parameters echoed back from the backend so the
   *  edit dialog can pre-fill without re-parsing local form state. */
  t0: number
  t1: number
  t2: number
  t3: number
  k: number
  w: number
  default_qoi: number
  default_qcc: number
  interrogate_period_s: number
  counter_interrogate_period_s: number
  broadcast_address: number
  /** Timing fields auto-corrected by the backend during creation/import.
   *  Empty when the supplied config already satisfied the IEC 104 invariants. */
  timing_corrections?: { field: string; from: number; to: number }[]
}

export interface ReceivedDataPointInfo {
  ioa: number
  /** Common Address of the station this point came from. */
  common_address: number
  asdu_type: string
  /** Numeric IEC 104 TypeID for `asdu_type` (e.g. M_SP_NA_1 → 1). */
  asdu_type_id: number
  category: string
  value: string
  quality_ov: boolean
  quality_bl: boolean
  quality_sb: boolean
  quality_nt: boolean
  quality_iv: boolean
  timestamp: string | null
  update_seq: number
}

export interface IncrementalDataResponse {
  seq: number
  total_count: number
  points: ReceivedDataPointInfo[]
}

export interface LogEntry {
  timestamp: string
  direction: string
  frame_label: { [key: string]: string } | string
  detail: string
  raw_bytes: number[] | null
  detail_event?: { kind: string; payload: Record<string, unknown> } | null
}

export type CommandType = 'single' | 'double' | 'step' | 'setpoint_normalized' | 'setpoint_scaled' | 'setpoint_float' | 'bitstring'

export interface ControlCommandRequest {
  connection_id: string
  ioa: number
  common_address: number
  command_type: CommandType
  value: string
  select?: boolean
  qualifier?: number
  cot?: number
  bitstring?: number
}

export interface RawApduRequest {
  connection_id: string
  hex_payload: string
}

export interface RawSendResult {
  sent_hex: string
  byte_len: number
  timestamp: string
}

// Frame parser types now live in @shared/types/frame to avoid duplicate
// definitions between slave and master frontends.
export * from '@shared/types/frame'

export interface ControlStep {
  action: string
  timestamp: string
}

export interface ControlResult {
  steps: ControlStep[]
  duration_ms: number
}

export type WidgetType = 'toggle' | 'button_group' | 'step_buttons' | 'slider' | 'number_input'

export interface ControlOption {
  label: string
  value: string
}

export interface ControlConfig {
  commandType: CommandType
  label: string
  widget: WidgetType
  options?: ControlOption[]
  min?: number
  max?: number
  step?: number
}

import { useI18n } from '@shared/i18n'

export function getControlConfig(category: string): ControlConfig | null {
  const { t } = useI18n()
  switch (category) {
    case '单点 (SP)':
      return {
        commandType: 'single',
        label: t('control.cmdSingle'),
        widget: 'toggle',
        options: [
          { label: t('control.optOff'), value: 'false' },
          { label: t('control.optOn'), value: 'true' },
        ],
      }
    case '双点 (DP)':
      return {
        commandType: 'double',
        label: t('control.cmdDouble'),
        widget: 'button_group',
        options: [
          { label: t('control.optIntermediate'), value: '0' },
          { label: t('control.optOpen'), value: '1' },
          { label: t('control.optClose'), value: '2' },
          { label: t('control.optInvalid'), value: '3' },
        ],
      }
    case '步位置 (ST)':
      return {
        commandType: 'step',
        label: t('control.cmdStep'),
        widget: 'step_buttons',
        options: [
          { label: t('control.optStepDown'), value: '1' },
          { label: t('control.optStepUp'), value: '2' },
        ],
      }
    case '归一化 (ME_NA)':
      return {
        commandType: 'setpoint_normalized',
        label: t('control.cmdSetNorm'),
        widget: 'slider',
        min: -1.0, max: 1.0, step: 0.001,
      }
    case '标度化 (ME_NB)':
      return {
        commandType: 'setpoint_scaled',
        label: t('control.cmdSetScaled'),
        widget: 'number_input',
        min: -32768, max: 32767, step: 1,
      }
    case '浮点 (ME_NC)':
      return {
        commandType: 'setpoint_float',
        label: t('control.cmdSetFloat'),
        widget: 'number_input',
        step: 0.1,
      }
    case '位串 (BO)':
      return {
        commandType: 'bitstring',
        label: t('control.cmdBitstring'),
        widget: 'number_input',
        min: 0, max: 0xFFFFFFFF, step: 1,
      }
    case '累计量 (IT)':
    default:
      return null
  }
}

/**
 * IEC 60870-5-104 命名约定：ASDU 类型名第二段以 T 开头表示携带 CP24/CP56 时标
 * (e.g. M_SP_TB_1 / M_ME_TF_1)，否则不带时标 (e.g. M_SP_NA_1 / M_ME_NC_1)。
 * 不带时标时,后端填入的 `timestamp` 仅为主站本地接收时间,不应作为报文时标展示。
 */
export function asduHasTimestamp(asduType: string): boolean {
  const parts = asduType.split('_')
  return parts.length >= 3 && parts[2].startsWith('T')
}
