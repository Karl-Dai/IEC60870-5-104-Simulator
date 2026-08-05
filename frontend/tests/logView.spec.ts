import { describe, expect, it } from 'vitest'
import {
  describeFrame,
  formatLogTimestamp,
  frameSearchText,
  frameTypeIds,
  matchesDirection,
  matchesFrameFilter,
  matchesSearch,
} from '@shared/logging/logView'

describe('communication log view helpers', () => {
  it('classifies tagged and unit frame labels and exposes Type IDs', () => {
    expect(describeFrame({ i_frame: 'M_SP_NA_1' })).toMatchObject({ kind: 'i', typeId: 'M_SP_NA_1' })
    expect(describeFrame('s_frame').kind).toBe('s')
    expect(describeFrame('u_test_act').kind).toBe('u')
    expect(describeFrame('single_command')).toMatchObject({ kind: 'i', typeId: 'C_SC_NA_1' })
    expect(frameSearchText('single_command')).toContain('C_SC_NA_1')
    expect(frameTypeIds([
      { timestamp: '', direction: 'rx', frame_label: { i_frame: 'M_SP_NA_1' }, detail: '' },
      { timestamp: '', direction: 'tx', frame_label: { i_frame: 'C_SC_NA_1' }, detail: '' },
      { timestamp: '', direction: 'rx', frame_label: { i_frame: 'M_SP_NA_1' }, detail: '' },
    ])).toEqual(['C_SC_NA_1', 'M_SP_NA_1'])
  })

  it('uses serialized APDU control bits and ASDU Type ID ahead of misleading labels', () => {
    const iFrame = [0x68, 0x0a, 0x00, 0x00, 0x00, 0x00, 13, 1, 3, 0, 1, 0]
    const sFrame = [0x68, 0x04, 0x01, 0x00, 0x00, 0x00]
    const uFrame = [0x68, 0x04, 0x07, 0x00, 0x00, 0x00]

    expect(describeFrame('s_frame', iFrame)).toMatchObject({ kind: 'i', typeId: 'M_ME_NC_1' })
    expect(describeFrame({ i_frame: 'M_SP_NA_1' }, sFrame).kind).toBe('s')
    expect(describeFrame({ i_frame: 'M_SP_NA_1' }, uFrame).kind).toBe('u')
    expect(matchesFrameFilter({ i_frame: 'wrong' }, 'type:M_ME_NC_1', iFrame)).toBe(true)
    expect(matchesFrameFilter({ i_frame: 'M_SP_NA_1' }, 'kind:i', sFrame)).toBe(false)
    expect(matchesFrameFilter({ i_frame: 'M_SP_NA_1' }, 'kind:s', sFrame)).toBe(true)
  })

  it('combines case-insensitive direction, frame, and free-text matching', () => {
    expect(matchesDirection('Rx', 'rx')).toBe(true)
    expect(matchesDirection('tx', 'rx')).toBe(false)
    expect(matchesFrameFilter({ i_frame: 'M_ME_NC_1' }, 'kind:i')).toBe(true)
    expect(matchesFrameFilter({ i_frame: 'M_ME_NC_1' }, 'type:M_ME_NC_1')).toBe(true)
    expect(matchesFrameFilter('single_command', 'type:C_SC_NA_1')).toBe(true)
    expect(matchesFrameFilter('s_frame', 'kind:i')).toBe(false)
    expect(matchesSearch(['RX', 'IOA=42', '192.0.2.10'], 'ioa=42')).toBe(true)
    expect(matchesSearch(['RX', 'IOA=42'], 'tx')).toBe(false)
  })

  it('always renders a valid timestamp with millisecond precision', () => {
    expect(formatLogTimestamp('2026-08-04T09:10:11.123Z', 'en-US')).toMatch(/11\.123/)
    expect(formatLogTimestamp('not-a-date', 'en-US')).toBe('not-a-date')
  })
})
