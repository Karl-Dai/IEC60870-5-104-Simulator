<script setup lang="ts">
import { inject, computed, ref, type Ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import type { ReceivedDataPointInfo, ControlResult } from '../types'
import { getControlConfig, asduHasTimestamp } from '../types'
import QualityIndicator from '@shared/components/QualityIndicator.vue'
import { useI18n, localizeCategoryLabel } from '@shared/i18n'
import { formatDataPointValue, normalizeDoublePointCode } from '@shared/utils/dataPointValue'

const { t } = useI18n()
const selectedConnectionId = inject<Ref<string | null>>('selectedConnectionId')!
const selectedPoints = inject<Ref<ReceivedDataPointInfo[]>>('selectedPoints')!

const hasSelection = computed(() => selectedPoints.value.length > 0)
const firstPoint = computed(() => selectedPoints.value[0] ?? null)

// Control state
const cmdSelect = ref(false)
const sending = ref(false)
const lastResult = ref<{ success: boolean; result?: ControlResult; error?: string } | null>(null)

// Setpoint input values
const setpointValue = ref('')

// Auto-detect control config from selected point's category
const controlConfig = computed(() => {
  if (!firstPoint.value) return null
  return getControlConfig(firstPoint.value.category)
})

// Get current value string for highlighting active state
const currentValue = computed(() => firstPoint.value?.value ?? '')

async function sendCommand(value: string | number | boolean) {
  if (!selectedConnectionId.value || !firstPoint.value || !controlConfig.value) return
  sending.value = true
  lastResult.value = null

  try {
    // The selected point carries its own CA (the station that emitted it),
    // so the control command always targets the right station even when
    // the connection has multiple CAs configured.
    const result = await invoke<ControlResult>('send_control_command', {
      request: {
        connection_id: selectedConnectionId.value,
        ioa: firstPoint.value.ioa,
        common_address: firstPoint.value.common_address,
        command_type: controlConfig.value.commandType,
        value: String(value),
        select: cmdSelect.value,
      }
    })
    lastResult.value = { success: true, result }
  } catch (e) {
    lastResult.value = { success: false, error: String(e) }
  } finally {
    sending.value = false
  }
}

function sendSetpoint() {
  sendCommand(setpointValue.value)
}

// Determine if an option matches the current value
function isActiveOption(optionValue: string): boolean {
  const cv = currentValue.value.toLowerCase()
  // single point
  if (optionValue === 'true') return cv === 'on'
  if (optionValue === 'false') return cv === 'off'
  // double point
  const point = firstPoint.value
  const isDoublePoint = point?.category === 'double_point'
    || point?.asdu_type.startsWith('M_DP_')
    || point?.asdu_type.startsWith('C_DC_')
  if (isDoublePoint && (optionValue === '0' || optionValue === '1' || optionValue === '2' || optionValue === '3')) {
    return normalizeDoublePointCode(cv) === optionValue
  }
  return false
}

// Parse numeric value from current point for setpoint prefill
const numericValue = computed(() => {
  if (!firstPoint.value) return 0
  const v = parseFloat(firstPoint.value.value)
  return isNaN(v) ? 0 : v
})

// Prefill setpoint when point changes
import { watch } from 'vue'
watch(firstPoint, (p) => {
  if (p && controlConfig.value) {
    const w = controlConfig.value.widget
    if (w === 'slider' || w === 'number_input') {
      setpointValue.value = String(numericValue.value)
    }
  }
  lastResult.value = null
})
</script>

<template>
  <div class="value-panel">
    <div class="panel-header">{{ t('valuePanel.title') }}</div>

    <div v-if="!hasSelection" class="empty-state">
      {{ t('valuePanel.selectPointHint') }}
    </div>

    <template v-else>
      <!-- Selected point details -->
      <div class="detail-section">
        <div class="section-title">{{ t('valuePanel.selectedPoint') }}</div>
        <div v-for="point in selectedPoints" :key="point.ioa" class="detail-item">
          <div class="detail-row">
            <span class="detail-label">IOA</span>
            <span class="detail-value mono">{{ point.ioa }}</span>
          </div>
          <div class="detail-row">
            <span class="detail-label">{{ t('valuePanel.type') }}</span>
            <span class="detail-value mono">{{ point.asdu_type }} · {{ point.asdu_type_id }}</span>
          </div>
          <div class="detail-row">
            <span class="detail-label">{{ t('valuePanel.category') }}</span>
            <span class="detail-value">{{ localizeCategoryLabel(point.category) }}</span>
          </div>
          <div class="detail-row">
            <span class="detail-label">{{ t('valuePanel.value') }}</span>
            <span class="detail-value mono">{{ formatDataPointValue(point, t) }}</span>
          </div>
          <div class="detail-row">
            <span class="detail-label">{{ t('valuePanel.quality') }}</span>
            <span class="detail-value">
              <QualityIndicator
                :quality="{ ov: point.quality_ov, bl: point.quality_bl, sb: point.quality_sb, nt: point.quality_nt, iv: point.quality_iv }"
                :show-ov="point.asdu_type.startsWith('M_ME')"
              />
            </span>
          </div>
          <div class="detail-row">
            <span class="detail-label">{{ t('valuePanel.timestamp') }}</span>
            <span class="detail-value mono">{{ asduHasTimestamp(point.asdu_type) ? (point.timestamp ?? t('valuePanel.timestampNone')) : t('valuePanel.timestampNone') }}</span>
          </div>
          <div v-if="selectedPoints.length > 1" class="detail-divider"></div>
        </div>
      </div>

      <!-- Smart control section -->
      <div v-if="controlConfig" class="control-section">
        <div class="section-title">{{ t('valuePanel.quickControl') }} - {{ controlConfig.label }}</div>

        <div class="control-form">
          <!-- Toggle (single point) -->
          <div v-if="controlConfig.widget === 'toggle'" class="toggle-buttons">
            <button
              v-for="opt in controlConfig.options"
              :key="opt.value"
              :class="['ctrl-btn', { active: isActiveOption(opt.value) }]"
              :disabled="sending || !selectedConnectionId"
              @click="sendCommand(opt.value)"
            >
              {{ opt.label }}
            </button>
          </div>

          <!-- Button group (double point) -->
          <div v-else-if="controlConfig.widget === 'button_group'" class="button-group">
            <button
              v-for="opt in controlConfig.options"
              :key="opt.value"
              :class="['ctrl-btn', 'ctrl-btn-sm', { active: isActiveOption(opt.value) }]"
              :disabled="sending || !selectedConnectionId"
              @click="sendCommand(opt.value)"
            >
              {{ opt.label }}
            </button>
          </div>

          <!-- Step buttons -->
          <div v-else-if="controlConfig.widget === 'step_buttons'" class="step-buttons">
            <button
              v-for="opt in controlConfig.options"
              :key="opt.value"
              class="ctrl-btn"
              :disabled="sending || !selectedConnectionId"
              @click="sendCommand(opt.value)"
            >
              {{ opt.value === '1' ? '&#9660;' : '&#9650;' }} {{ opt.label }}
            </button>
          </div>

          <!-- Slider (normalized) -->
          <div v-else-if="controlConfig.widget === 'slider'" class="slider-control">
            <div class="slider-row">
              <input
                type="range"
                class="slider-input"
                :min="controlConfig.min"
                :max="controlConfig.max"
                :step="controlConfig.step"
                v-model="setpointValue"
              />
              <input
                type="number"
                class="number-sm"
                :min="controlConfig.min"
                :max="controlConfig.max"
                :step="controlConfig.step"
                v-model="setpointValue"
              />
            </div>
            <button
              class="ctrl-btn ctrl-btn-primary"
              :disabled="sending || !selectedConnectionId"
              @click="sendSetpoint"
            >
              {{ t('valuePanel.sendSetpoint') }}
            </button>
          </div>

          <!-- Number input (scaled / float) -->
          <div v-else-if="controlConfig.widget === 'number_input'" class="number-control">
            <input
              type="number"
              class="form-input"
              :min="controlConfig.min"
              :max="controlConfig.max"
              :step="controlConfig.step"
              v-model="setpointValue"
              @keydown.enter="sendSetpoint"
            />
            <button
              class="ctrl-btn ctrl-btn-primary"
              :disabled="sending || !selectedConnectionId"
              @click="sendSetpoint"
            >
              {{ t('valuePanel.sendSetpoint') }}
            </button>
          </div>

          <!-- Select/Execute mode -->
          <div class="toggle-row">
            <label class="toggle-label">
              <input type="checkbox" v-model="cmdSelect" class="toggle-checkbox" />
              <span>{{ t('valuePanel.sboLabel') }}</span>
            </label>
            <span class="toggle-hint">{{ cmdSelect ? t('valuePanel.sboTwoStep') : t('valuePanel.sboDirect') }}</span>
          </div>

          <!-- Control result indicator -->
          <div v-if="lastResult" :class="['result-indicator', lastResult.success ? 'result-ok' : 'result-err']">
            <template v-if="lastResult.success && lastResult.result">
              <div class="result-steps">
                <span
                  v-for="(step, i) in lastResult.result.steps"
                  :key="i"
                  class="step-dot"
                  :title="step.action + ' ' + step.timestamp"
                >&#9679;</span>
              </div>
              <span class="result-text">OK {{ lastResult.result.duration_ms }}ms</span>
            </template>
            <template v-else>
              <span class="result-text">{{ lastResult.error }}</span>
            </template>
          </div>
        </div>
      </div>

      <div v-else-if="firstPoint" class="no-control-hint">
        {{ t('valuePanel.notControllable') }}
      </div>
    </template>
  </div>
</template>

<style scoped>
.value-panel {
  padding: 0;
  font-size: 13px;
}

.panel-header {
  padding: 8px 12px;
  font-size: 11px;
  text-transform: uppercase;
  color: var(--c-overlay0);
  letter-spacing: 0.5px;
}

.empty-state {
  padding: 24px 12px;
  color: var(--c-overlay0);
  text-align: center;
  font-size: 12px;
}

.detail-section {
  border-bottom: 1px solid var(--c-surface0);
  padding-bottom: 8px;
}

.section-title {
  padding: 6px 12px;
  font-size: 11px;
  color: var(--c-overlay0);
  text-transform: uppercase;
  letter-spacing: 0.3px;
}

.detail-item {
  padding: 0 4px;
}

.detail-row {
  display: flex;
  justify-content: space-between;
  padding: 3px 12px;
}

.detail-label {
  color: var(--c-overlay0);
  font-size: 12px;
}

.detail-value {
  color: var(--c-text);
  font-size: 12px;
  text-align: right;
}

.detail-value.mono {
  font-family: var(--font-mono);
}

.text-green {
  color: var(--c-green);
}

.text-red {
  color: var(--c-red);
}

.detail-divider {
  height: 1px;
  background: var(--c-surface0);
  margin: 6px 12px;
}

.control-section {
  padding-bottom: 12px;
}

.control-form {
  padding: 0 12px;
  display: flex;
  flex-direction: column;
  gap: 10px;
}

.toggle-buttons,
.button-group,
.step-buttons {
  display: flex;
  gap: 6px;
}

.button-group {
  flex-wrap: wrap;
}

.ctrl-btn {
  flex: 1;
  padding: 8px 12px;
  border: 1px solid var(--c-surface1);
  border-radius: 6px;
  background: var(--c-surface0);
  color: var(--c-text);
  font-size: 12px;
  cursor: pointer;
  transition: all 0.15s;
}

.ctrl-btn:hover:not(:disabled) {
  background: var(--c-surface1);
}

.ctrl-btn:disabled {
  opacity: 0.4;
  cursor: default;
}

.ctrl-btn.active {
  background: var(--c-blue);
  color: var(--c-base);
  border-color: var(--c-blue);
  font-weight: 600;
}

.ctrl-btn-sm {
  padding: 6px 8px;
  font-size: 11px;
}

.ctrl-btn-primary {
  background: var(--c-blue);
  color: var(--c-base);
  border-color: var(--c-blue);
  font-weight: 600;
}

.ctrl-btn-primary:hover:not(:disabled) {
  background: var(--c-sapphire);
  border-color: var(--c-sapphire);
}

.slider-control {
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.slider-row {
  display: flex;
  gap: 8px;
  align-items: center;
}

.slider-input {
  flex: 1;
  accent-color: var(--c-blue);
}

.number-sm {
  width: 72px;
  padding: 4px 6px;
  background: var(--c-surface0);
  border: 1px solid var(--c-surface1);
  border-radius: 4px;
  color: var(--c-text);
  font-size: 12px;
  font-family: var(--font-mono);
}

.number-sm:focus {
  outline: none;
  border-color: var(--c-blue);
}

.number-control {
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.form-input {
  padding: 5px 8px;
  background: var(--c-surface0);
  border: 1px solid var(--c-surface1);
  border-radius: 4px;
  color: var(--c-text);
  font-size: 12px;
}

.form-input:focus {
  outline: none;
  border-color: var(--c-blue);
}

.toggle-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 2px 0;
}

.toggle-label {
  display: flex;
  align-items: center;
  gap: 6px;
  font-size: 12px;
  color: var(--c-text);
  cursor: pointer;
}

.toggle-checkbox {
  accent-color: var(--c-blue);
}

.toggle-hint {
  font-size: 10px;
  color: var(--c-overlay0);
}

.result-indicator {
  padding: 6px 8px;
  border-radius: 4px;
  font-size: 11px;
  display: flex;
  align-items: center;
  gap: 8px;
}

.result-ok {
  background: rgba(166, 227, 161, 0.15);
  border: 1px solid rgba(166, 227, 161, 0.3);
  color: var(--c-green);
}

.result-err {
  background: rgba(243, 139, 168, 0.15);
  border: 1px solid rgba(243, 139, 168, 0.3);
  color: var(--c-red);
}

.result-steps {
  display: flex;
  gap: 4px;
  font-size: 8px;
}

.step-dot {
  color: var(--c-green);
}

.result-text {
  font-family: var(--font-mono);
}

.no-control-hint {
  padding: 16px 12px;
  color: var(--c-overlay0);
  text-align: center;
  font-size: 12px;
  font-style: italic;
}
</style>
