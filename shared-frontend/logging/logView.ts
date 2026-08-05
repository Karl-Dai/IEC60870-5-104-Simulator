export type LogFrameLabel = Record<string, string> | string

export interface LogLike {
  timestamp: string
  direction: string
  frame_label: LogFrameLabel
  detail: string
  raw_bytes?: number[] | null
}

export type FrameFilter = 'all' | 'kind:i' | 'kind:s' | 'kind:u' | `type:${string}`

export interface FrameDescriptor {
  kind: 'i' | 's' | 'u' | 'other'
  typeId: string
  searchable: string
  variant?: string
}

const ASDU_TYPE_NAMES: Record<number, string> = {
  1: 'M_SP_NA_1', 2: 'M_SP_TA_1', 3: 'M_DP_NA_1', 4: 'M_DP_TA_1',
  5: 'M_ST_NA_1', 6: 'M_ST_TA_1', 7: 'M_BO_NA_1',
  9: 'M_ME_NA_1', 10: 'M_ME_TA_1', 11: 'M_ME_NB_1', 12: 'M_ME_TB_1',
  13: 'M_ME_NC_1', 14: 'M_ME_TC_1', 15: 'M_IT_NA_1', 21: 'M_ME_ND_1',
  30: 'M_SP_TB_1', 31: 'M_DP_TB_1', 32: 'M_ST_TB_1', 33: 'M_BO_TB_1',
  34: 'M_ME_TD_1', 35: 'M_ME_TE_1', 36: 'M_ME_TF_1', 37: 'M_IT_TB_1',
  45: 'C_SC_NA_1', 46: 'C_DC_NA_1', 47: 'C_RC_NA_1', 48: 'C_SE_NA_1',
  49: 'C_SE_NB_1', 50: 'C_SE_NC_1', 51: 'C_BO_NA_1',
  58: 'C_SC_TA_1', 59: 'C_DC_TA_1', 60: 'C_RC_TA_1', 61: 'C_SE_TA_1',
  62: 'C_SE_TB_1', 63: 'C_SE_TC_1', 64: 'C_BO_TA_1',
  100: 'C_IC_NA_1', 101: 'C_CI_NA_1', 103: 'C_CS_NA_1',
}

function normalizeFrameKey(key: string): string {
  return key.trim().toLowerCase().replaceAll('-', '_')
}

const I_FRAME_TYPE_BY_KEY: Record<string, string> = {
  general_interrogation: 'C_IC_NA_1',
  counter_read: 'C_CI_NA_1',
  counter_interrogation: 'C_CI_NA_1',
  clock_sync: 'C_CS_NA_1',
  single_command: 'C_SC_NA_1',
  double_command: 'C_DC_NA_1',
  step_command: 'C_RC_NA_1',
  setpoint_normalized: 'C_SE_NA_1',
  setpoint_scaled: 'C_SE_NB_1',
  setpoint_float: 'C_SE_NC_1',
  bitstring: 'C_BO_NA_1',
}

export function describeWireFrame(rawBytes: number[] | null | undefined): FrameDescriptor | null {
  if (!rawBytes || rawBytes.length < 6 || rawBytes[0] !== 0x68) return null
  if (rawBytes[1] + 2 > rawBytes.length) return null
  const control = rawBytes[2]
  if ((control & 0x01) === 0) {
    const typeId = rawBytes.length > 6
      ? (ASDU_TYPE_NAMES[rawBytes[6]] ?? `Type ID ${rawBytes[6]}`)
      : ''
    return { kind: 'i', typeId, searchable: `I-frame ${typeId}`.trim(), variant: 'i_frame' }
  }
  if ((control & 0x03) === 0x01) {
    return { kind: 's', typeId: '', searchable: 'S-frame', variant: 's_frame' }
  }
  if ((control & 0x03) === 0x03) {
    const variant = ({
      0x07: 'u_start_act',
      0x0b: 'u_start_con',
      0x13: 'u_stop_act',
      0x23: 'u_stop_con',
      0x43: 'u_test_act',
      0x83: 'u_test_con',
    } as Record<number, string>)[control]
    const suffix = variant?.slice(2).replaceAll('_', ' ').toUpperCase() ?? ''
    return { kind: 'u', typeId: '', searchable: `U-frame ${suffix}`.trim(), variant }
  }
  return null
}

export function describeFrame(
  label: LogFrameLabel,
  rawBytes?: number[] | null,
): FrameDescriptor {
  // The wire control field is authoritative. Some receive paths attach a
  // semantic or summary label that does not preserve the actual APCI kind.
  const rawDescriptor = describeWireFrame(rawBytes)
  if (rawDescriptor) return rawDescriptor

  if (typeof label === 'string') {
    const key = normalizeFrameKey(label)
    if (key === 'i' || key === 'i_frame') return { kind: 'i', typeId: '', searchable: label }
    const knownTypeId = I_FRAME_TYPE_BY_KEY[key]
    if (knownTypeId) return { kind: 'i', typeId: knownTypeId, searchable: `${label} ${knownTypeId}` }
    const legacyIFrame = /^i(?:_frame)?[\s:]+(.+)$/i.exec(label.trim())
    if (legacyIFrame) return { kind: 'i', typeId: legacyIFrame[1].trim(), searchable: label }
    if (key === 's' || key === 's_frame') return { kind: 's', typeId: '', searchable: label }
    if (key === 'u' || key === 'u_frame' || key.startsWith('u_')) {
      return { kind: 'u', typeId: '', searchable: label }
    }
    return { kind: 'other', typeId: '', searchable: label }
  }

  const [rawKey = '', rawValue = ''] = Object.entries(label)[0] ?? []
  const key = normalizeFrameKey(rawKey)
  const value = String(rawValue).trim()
  if (key === 'i' || key === 'i_frame') {
    return { kind: 'i', typeId: value, searchable: `${rawKey} ${value}`.trim() }
  }
  if (key === 's' || key === 's_frame') {
    return { kind: 's', typeId: '', searchable: `${rawKey} ${value}`.trim() }
  }
  if (key === 'u' || key === 'u_frame' || key.startsWith('u_')) {
    return { kind: 'u', typeId: '', searchable: `${rawKey} ${value}`.trim() }
  }
  return { kind: 'other', typeId: '', searchable: `${rawKey} ${value}`.trim() }
}

export function frameTypeIds(logs: readonly LogLike[]): string[] {
  const values = new Set<string>()
  for (const log of logs) {
    const typeId = describeFrame(log.frame_label, log.raw_bytes).typeId
    if (typeId) values.add(typeId)
  }
  return [...values].sort((a, b) => a.localeCompare(b))
}

export function frameSearchText(label: LogFrameLabel, rawBytes?: number[] | null): string {
  return describeFrame(label, rawBytes).searchable
}

export function matchesFrameFilter(
  label: LogFrameLabel,
  filter: FrameFilter,
  rawBytes?: number[] | null,
): boolean {
  if (filter === 'all') return true
  const descriptor = describeFrame(label, rawBytes)
  if (filter.startsWith('kind:')) return descriptor.kind === filter.slice(5)
  return descriptor.typeId.toLocaleLowerCase() === filter.slice(5).toLocaleLowerCase()
}

export function matchesDirection(direction: string, filter: string): boolean {
  return filter === 'all' || direction.toLocaleLowerCase() === filter.toLocaleLowerCase()
}

export function matchesSearch(fields: readonly string[], query: string): boolean {
  const needle = query.trim().toLocaleLowerCase()
  return needle === '' || fields.some(field => field.toLocaleLowerCase().includes(needle))
}

const timestampFormatters = new Map<string, Intl.DateTimeFormat>()

export function formatLogTimestamp(timestamp: string, locale: string): string {
  try {
    const date = new Date(timestamp)
    if (Number.isNaN(date.getTime())) return timestamp
    let formatter = timestampFormatters.get(locale)
    if (!formatter) {
      formatter = new Intl.DateTimeFormat(locale, {
        hour12: false,
        hour: '2-digit',
        minute: '2-digit',
        second: '2-digit',
        fractionalSecondDigits: 3,
      } as Intl.DateTimeFormatOptions)
      timestampFormatters.set(locale, formatter)
    }
    return formatter.format(date)
  } catch {
    return timestamp
  }
}

export function formatRawBytes(raw: number[] | null | undefined): string {
  if (!raw || raw.length === 0) return ''
  return raw.map(byte => byte.toString(16).toUpperCase().padStart(2, '0')).join(' ')
}

/** Stable enough for viewport anchoring across polling snapshots. */
export function logIdentity(log: LogLike): string {
  const frame = typeof log.frame_label === 'string'
    ? log.frame_label
    : JSON.stringify(log.frame_label)
  return `${log.timestamp}\u0000${log.direction}\u0000${frame}\u0000${log.detail}`
}
