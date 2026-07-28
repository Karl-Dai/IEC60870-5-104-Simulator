export interface DisplayableDataPointValue {
  asdu_type: string
  value: string
}

type Translate = (key: string) => string

function isDoublePointValue(asduType: string) {
  return asduType.startsWith('M_DP_') || asduType.startsWith('C_DC_')
}

/** Render stable backend point values in the active UI language. */
export function formatDataPointValue(
  point: DisplayableDataPointValue,
  t: Translate,
): string {
  if (!isDoublePointValue(point.asdu_type)) return point.value

  switch (point.value.trim()) {
    // Accept the legacy Chinese strings as a compatibility fallback for any
    // stale response produced during an in-place upgrade.
    case '0':
    case '中间':
    case 'Intermediate':
      return t('table.dpIntermediate')
    case '1':
      return 'OFF'
    case '2':
      return 'ON'
    case '3':
    case '不确定':
    case 'Indeterminate':
      return t('table.dpIndeterminate')
    default:
      return point.value
  }
}
