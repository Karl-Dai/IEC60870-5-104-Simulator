export interface DisplayableDataPointValue {
  asdu_type: string
  value: string
}

type Translate = (key: string) => string
export type DoublePointCode = '0' | '1' | '2' | '3'

function isDoublePointValue(asduType: string) {
  return asduType.startsWith('M_DP_') || asduType.startsWith('C_DC_')
}

/** Normalize current and legacy DPI representations to their protocol code. */
export function normalizeDoublePointCode(value: string): DoublePointCode | null {
  switch (value.trim().toLowerCase()) {
    case '0':
    case 'intermediate':
    case '中间':
      return '0'
    case '1':
    case 'off':
    case '分':
      return '1'
    case '2':
    case 'on':
    case '合':
      return '2'
    case '3':
    case 'indeterminate':
    case '不确定':
      return '3'
    default:
      return null
  }
}

/** Render stable backend point values in the active UI language. */
export function formatDataPointValue(
  point: DisplayableDataPointValue,
  t: Translate,
): string {
  if (!isDoublePointValue(point.asdu_type)) return point.value

  switch (normalizeDoublePointCode(point.value)) {
    case '0':
      return t('table.dpIntermediate')
    case '1':
      return 'OFF'
    case '2':
      return 'ON'
    case '3':
      return t('table.dpIndeterminate')
    default:
      return point.value
  }
}
