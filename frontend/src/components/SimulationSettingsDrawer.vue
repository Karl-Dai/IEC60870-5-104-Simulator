<script setup lang="ts">
import { computed, inject, onBeforeUnmount, ref, watch } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { dialogKey } from '@shared/composables/useDialog'
import type { showAlert as ShowAlert } from '@shared/composables/useDialog'
import { useI18n } from '@shared/i18n'
import { formatAsduTypeWithId } from '../constants/asduTypes'
import { formatDataPointValue } from '@shared/utils/dataPointValue'
import type { DataPointInfo, MutationMode, PointMutationRow } from '../types'

const props = defineProps<{
  visible: boolean
  serverId: string
  commonAddress: number
  selectedPoints: DataPointInfo[]
  activeRows: PointMutationRow[]
}>()

const emit = defineEmits<{
  close: []
  changed: []
}>()

const { t } = useI18n()
const { showAlert } = inject<{ showAlert: typeof ShowAlert }>(dialogKey)!

const period = ref(1000)
const mode = ref<MutationMode>('flip')
const step = ref(1)
const min = ref(0)
const max = ref(100)
const actionPending = ref(false)

function pointKey(ioa: number, asduType: string) {
  return `${ioa}:${asduType}`
}

function pointSupportsStep(asduType: string) {
  return /^M_(ME|IT)_/.test(asduType)
}

const selectedSignature = computed(() =>
  props.selectedPoints.map((point) => pointKey(point.ioa, point.asdu_type)).join('|'),
)
const activeByKey = computed(() =>
  new Map(props.activeRows.map((row) => [pointKey(row.ioa, row.asdu_type), row])),
)
const selectionSupportsStep = computed(() =>
  props.selectedPoints.some((point) => pointSupportsStep(point.asdu_type)),
)
const anySelectedActive = computed(() =>
  props.selectedPoints.some((point) => activeByKey.value.has(pointKey(point.ioa, point.asdu_type))),
)
const selectedConfigSignatures = computed(() =>
  props.selectedPoints.map((point) => {
    const active = activeByKey.value.get(pointKey(point.ioa, point.asdu_type))
    return active
      ? `${active.mode}:${active.period_ms}:${active.step}:${active.min}:${active.max}`
      : 'inactive'
  }),
)
const mixedSelectedConfig = computed(() =>
  new Set(selectedConfigSignatures.value).size > 1,
)

function applyDefaults(point: DataPointInfo) {
  const value = Number.parseFloat(point.value) || 0
  mode.value = 'flip'
  period.value = 1000
  if (point.asdu_type.startsWith('M_ME_NA')) {
    step.value = 0.05
    min.value = -1
    max.value = 1
  } else if (point.asdu_type.startsWith('M_ME_NB')) {
    step.value = 100
    min.value = -10000
    max.value = 10000
  } else if (point.asdu_type.startsWith('M_ME_NC')) {
    step.value = 1
    min.value = Math.round((value - 100) * 1e3) / 1e3
    max.value = Math.round((value + 100) * 1e3) / 1e3
  } else if (point.asdu_type.startsWith('M_IT')) {
    step.value = 1
    min.value = 0
    max.value = 10000
  }
}

function loadSelectionConfig() {
  const first = props.selectedPoints[0]
  if (!first) return
  const active = activeByKey.value.get(pointKey(first.ioa, first.asdu_type))
  if (active && !mixedSelectedConfig.value) {
    period.value = active.period_ms
    mode.value = active.mode
    step.value = active.step
    min.value = active.min
    max.value = active.max
    return
  }
  applyDefaults(first)
  if (!selectionSupportsStep.value) mode.value = 'flip'
}

watch(
  [() => props.visible, selectedSignature],
  ([visible]) => {
    if (visible) loadSelectionConfig()
  },
  { flush: 'sync', immediate: true },
)

function close() {
  if (!actionPending.value) emit('close')
}

function handleBackdrop(event: MouseEvent) {
  if ((event.target as HTMLElement).classList.contains('sim-drawer-backdrop')) close()
}

function handleKeydown(event: KeyboardEvent) {
  if (event.key === 'Escape' && props.visible) close()
}

watch(() => props.visible, (visible) => {
  if (visible) window.addEventListener('keydown', handleKeydown)
  else window.removeEventListener('keydown', handleKeydown)
}, { immediate: true })
onBeforeUnmount(() => window.removeEventListener('keydown', handleKeydown))

async function applyToSelection() {
  if (actionPending.value || props.selectedPoints.length === 0) return
  if (!Number.isFinite(period.value) || period.value < 50 || period.value > 60000) {
    await showAlert(t('simulationSettings.periodRange'))
    return
  }
  if (
    selectionSupportsStep.value
    && mode.value !== 'flip'
    && (!Number.isFinite(step.value) || step.value === 0)
  ) {
    await showAlert(t('simulationSettings.stepInvalid'))
    return
  }
  if (
    selectionSupportsStep.value
    && mode.value !== 'flip'
    && (!Number.isFinite(min.value) || !Number.isFinite(max.value) || min.value > max.value)
  ) {
    await showAlert(t('simulationSettings.boundsInvalid'))
    return
  }

  const serverId = props.serverId
  const commonAddress = props.commonAddress
  const targets = props.selectedPoints.map((point) => ({
    ioa: point.ioa,
    asdu_type: point.asdu_type,
  }))
  actionPending.value = true
  try {
    for (const target of targets) {
      await invoke('start_point_mutation', {
        serverId,
        commonAddress,
        ioa: target.ioa,
        asduType: target.asdu_type,
        periodMs: period.value,
        mode: pointSupportsStep(target.asdu_type) ? mode.value : 'flip',
        step: step.value,
        min: min.value,
        max: max.value,
      })
    }
    emit('changed')
  } catch (error) {
    await showAlert(String(error))
  } finally {
    actionPending.value = false
  }
}

async function stopPoints(points: Array<{ ioa: number; asdu_type: string }>) {
  if (actionPending.value || points.length === 0) return
  const serverId = props.serverId
  const commonAddress = props.commonAddress
  actionPending.value = true
  try {
    for (const point of points) {
      await invoke('stop_point_mutation', {
        serverId,
        commonAddress,
        ioa: point.ioa,
        asduType: point.asdu_type,
      })
    }
    emit('changed')
  } catch (error) {
    await showAlert(String(error))
  } finally {
    actionPending.value = false
  }
}

function stopSelection() {
  return stopPoints(props.selectedPoints)
}

function modeLabel(value: MutationMode) {
  if (value === 'increment') return t('table.modeIncrement')
  if (value === 'decrement') return t('table.modeDecrement')
  return t('table.modeFlip')
}
</script>

<template>
  <Teleport to="body">
    <Transition name="sim-drawer">
      <div
        v-if="visible"
        class="sim-drawer-backdrop"
        @mousedown="handleBackdrop"
      >
        <aside
          class="sim-drawer"
          role="dialog"
          :aria-label="t('simulationSettings.title')"
          @mousedown.stop
        >
          <header class="sim-drawer-head">
            <div>
              <span class="sim-eyebrow">SIMULATION</span>
              <h3>{{ t('simulationSettings.title') }}</h3>
            </div>
            <button
              class="sim-close"
              :disabled="actionPending"
              :aria-label="t('common.close')"
              @click="close"
            >×</button>
          </header>

          <div class="sim-drawer-body">
            <section class="sim-section">
              <h4>{{ t('simulationSettings.selectionHint', { count: selectedPoints.length }) }}</h4>
              <p v-if="selectedPoints.length === 0" class="sim-empty">
                {{ t('simulationSettings.noSelection') }}
              </p>
              <template v-else>
                <div class="sim-selection">
                  <span
                    v-for="point in selectedPoints.slice(0, 8)"
                    :key="pointKey(point.ioa, point.asdu_type)"
                    class="sim-point-chip"
                  >{{ point.ioa }} · {{ formatAsduTypeWithId(point.asdu_type) }}</span>
                  <span v-if="selectedPoints.length > 8" class="sim-point-chip">
                    +{{ selectedPoints.length - 8 }}
                  </span>
                </div>
                <p v-if="mixedSelectedConfig" class="sim-warning">
                  {{ t('simulationSettings.mixedValues') }}
                </p>

                <div class="sim-form">
                  <label>
                    <span>{{ t('table.mutationPeriod') }}</span>
                    <div class="sim-input-unit">
                      <input v-model.number="period" type="number" min="50" max="60000" />
                      <span>ms</span>
                    </div>
                  </label>

                  <div v-if="selectionSupportsStep" class="sim-mode-field">
                    <span>{{ t('table.mutationMode') }}</span>
                    <div class="sim-mode-buttons">
                      <button :class="{ active: mode === 'flip' }" @click="mode = 'flip'">
                        {{ t('table.modeFlip') }}
                      </button>
                      <button :class="{ active: mode === 'increment' }" @click="mode = 'increment'">
                        {{ t('table.modeIncrement') }}
                      </button>
                      <button :class="{ active: mode === 'decrement' }" @click="mode = 'decrement'">
                        {{ t('table.modeDecrement') }}
                      </button>
                    </div>
                  </div>

                  <template v-if="selectionSupportsStep && mode !== 'flip'">
                    <label>
                      <span>{{ t('table.mutationStep') }}</span>
                      <input v-model.number="step" type="number" />
                    </label>
                    <label>
                      <span>{{ t('table.mutationMin') }}</span>
                      <input v-model.number="min" type="number" />
                    </label>
                    <label>
                      <span>{{ t('table.mutationMax') }}</span>
                      <input v-model.number="max" type="number" />
                    </label>
                  </template>
                </div>

                <div class="sim-actions">
                  <button
                    class="sim-btn sim-btn-primary"
                    :disabled="actionPending"
                    @click="applyToSelection"
                  >{{ t('simulationSettings.apply') }}</button>
                  <button
                    v-if="anySelectedActive"
                    class="sim-btn sim-btn-danger"
                    :disabled="actionPending"
                    @click="stopSelection"
                  >{{ t('simulationSettings.stopSelected') }}</button>
                </div>
              </template>
            </section>

            <section class="sim-section">
              <div class="sim-section-title">
                <h4>{{ t('simulationSettings.activeTitle') }}</h4>
                <span>{{ activeRows.length }}</span>
              </div>
              <p v-if="activeRows.length === 0" class="sim-empty">
                {{ t('simulationSettings.noActive') }}
              </p>
              <div v-else class="sim-active-list">
                <article
                  v-for="row in activeRows"
                  :key="pointKey(row.ioa, row.asdu_type)"
                  class="sim-active-card"
                >
                  <div class="sim-active-head">
                    <div>
                      <strong>IOA {{ row.ioa }}</strong>
                      <span>{{ formatAsduTypeWithId(row.asdu_type) }}</span>
                    </div>
                    <button
                      class="sim-row-stop"
                      :disabled="actionPending"
                      @click="stopPoints([row])"
                    >{{ t('simulationSettings.stop') }}</button>
                  </div>
                  <dl>
                    <div>
                      <dt>{{ t('table.mutationMode') }}</dt>
                      <dd>{{ modeLabel(row.mode) }}</dd>
                    </div>
                    <div>
                      <dt>{{ t('table.mutationPeriod') }}</dt>
                      <dd>{{ row.period_ms }} ms</dd>
                    </div>
                    <div v-if="row.mode !== 'flip'">
                      <dt>{{ t('table.mutationStep') }}</dt>
                      <dd>{{ row.step }}</dd>
                    </div>
                    <div v-if="row.mode !== 'flip'">
                      <dt>{{ t('table.mutationMin') }} / {{ t('table.mutationMax') }}</dt>
                      <dd>{{ row.min }} / {{ row.max }}</dd>
                    </div>
                    <div>
                      <dt>{{ t('simulationSettings.currentValue') }}</dt>
                      <dd class="sim-current-value">{{ formatDataPointValue(row, t) }}</dd>
                    </div>
                  </dl>
                </article>
              </div>
            </section>
          </div>
        </aside>
      </div>
    </Transition>
  </Teleport>
</template>

<style scoped>
.sim-drawer-backdrop {
  position: fixed;
  inset: 0;
  z-index: 1550;
  display: flex;
  justify-content: flex-end;
  background: color-mix(in srgb, var(--c-crust) 60%, transparent);
  backdrop-filter: blur(4px);
  -webkit-backdrop-filter: blur(4px);
}

.sim-drawer {
  width: 460px;
  max-width: 94vw;
  height: 100vh;
  display: flex;
  flex-direction: column;
  color: var(--c-text);
  background: var(--c-mantle);
  border-left: 1px solid var(--c-surface0);
  box-shadow: -16px 0 32px -8px rgba(0, 0, 0, 0.45);
}

.sim-drawer-head {
  flex-shrink: 0;
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 14px 16px;
  border-bottom: 1px solid var(--c-surface0);
}

.sim-eyebrow {
  display: block;
  margin-bottom: 4px;
  color: var(--c-overlay0);
  font: 600 9.5px/1 ui-monospace, "SF Mono", Menlo, monospace;
  letter-spacing: 0.16em;
}

.sim-drawer-head h3,
.sim-section h4 {
  margin: 0;
  color: var(--c-text);
}

.sim-drawer-head h3 {
  font-size: 14px;
}

.sim-close {
  width: 28px;
  height: 28px;
  color: var(--c-overlay0);
  background: transparent;
  border: 0;
  border-radius: 4px;
  font-size: 21px;
  cursor: pointer;
}

.sim-close:hover:not(:disabled) {
  color: var(--c-text);
  background: var(--c-surface0);
}

.sim-drawer-body {
  flex: 1;
  min-height: 0;
  padding: 14px;
  overflow-y: auto;
}

.sim-section {
  margin-bottom: 14px;
  padding: 14px;
  background: var(--c-base);
  border: 1px solid var(--c-surface0);
  border-radius: 7px;
}

.sim-section h4 {
  font-size: 12px;
}

.sim-section-title {
  display: flex;
  align-items: center;
  justify-content: space-between;
}

.sim-section-title > span {
  min-width: 22px;
  padding: 2px 6px;
  color: var(--c-subtext0);
  background: var(--c-surface0);
  border-radius: 10px;
  font: 600 10px/1.3 ui-monospace, "SF Mono", Menlo, monospace;
  text-align: center;
}

.sim-empty,
.sim-warning {
  margin: 12px 0 0;
  padding: 9px 10px;
  color: var(--c-subtext0);
  background: var(--c-mantle);
  border-left: 2px solid var(--c-surface1);
  border-radius: 3px;
  font-size: 11px;
  line-height: 1.45;
}

.sim-warning {
  color: var(--c-yellow);
  border-left-color: var(--c-yellow);
}

.sim-selection {
  display: flex;
  flex-wrap: wrap;
  gap: 5px;
  margin-top: 10px;
}

.sim-point-chip {
  max-width: 100%;
  padding: 3px 6px;
  overflow: hidden;
  color: var(--c-subtext0);
  background: var(--c-surface0);
  border-radius: 4px;
  font: 500 10px/1.3 ui-monospace, "SF Mono", Menlo, monospace;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.sim-form {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 10px;
  margin-top: 12px;
}

.sim-form label,
.sim-mode-field {
  display: flex;
  flex-direction: column;
  gap: 5px;
  color: var(--c-subtext0);
  font-size: 11px;
}

.sim-mode-field {
  grid-column: 1 / -1;
}

.sim-form input {
  width: 100%;
  height: 30px;
  box-sizing: border-box;
  padding: 0 8px;
  color: var(--c-text);
  background: var(--c-crust);
  border: 1px solid var(--c-surface1);
  border-radius: 4px;
  font: 500 12px/1 ui-monospace, "SF Mono", Menlo, monospace;
  outline: none;
}

.sim-form input:focus {
  border-color: var(--c-blue);
}

.sim-input-unit {
  position: relative;
}

.sim-input-unit input {
  padding-right: 34px;
}

.sim-input-unit span {
  position: absolute;
  top: 8px;
  right: 8px;
  color: var(--c-overlay0);
  font: 500 10px/1 ui-monospace, "SF Mono", Menlo, monospace;
}

.sim-mode-buttons {
  display: grid;
  grid-template-columns: repeat(3, 1fr);
  gap: 5px;
}

.sim-mode-buttons button {
  height: 30px;
  color: var(--c-subtext0);
  background: var(--c-crust);
  border: 1px solid var(--c-surface1);
  border-radius: 4px;
  cursor: pointer;
}

.sim-mode-buttons button.active {
  color: var(--c-base);
  background: var(--c-blue);
  border-color: var(--c-blue);
  font-weight: 600;
}

.sim-actions {
  display: flex;
  gap: 8px;
  margin-top: 12px;
}

.sim-btn {
  padding: 7px 12px;
  border: 1px solid transparent;
  border-radius: 4px;
  font-size: 11px;
  font-weight: 600;
  cursor: pointer;
}

.sim-btn:disabled,
.sim-row-stop:disabled,
.sim-close:disabled {
  opacity: 0.45;
  cursor: not-allowed;
}

.sim-btn-primary {
  color: var(--c-base);
  background: var(--c-blue);
  border-color: var(--c-blue);
}

.sim-btn-danger {
  color: var(--c-red);
  background: transparent;
  border-color: var(--c-red);
}

.sim-active-list {
  display: flex;
  flex-direction: column;
  gap: 8px;
  margin-top: 10px;
}

.sim-active-card {
  padding: 10px;
  background: var(--c-mantle);
  border: 1px solid var(--c-surface0);
  border-radius: 5px;
}

.sim-active-head {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  gap: 8px;
}

.sim-active-head strong,
.sim-active-head span {
  display: block;
}

.sim-active-head strong {
  font: 600 11px/1.3 ui-monospace, "SF Mono", Menlo, monospace;
}

.sim-active-head span {
  margin-top: 2px;
  color: var(--c-overlay0);
  font-size: 10px;
}

.sim-row-stop {
  padding: 3px 7px;
  color: var(--c-red);
  background: transparent;
  border: 1px solid color-mix(in srgb, var(--c-red) 55%, transparent);
  border-radius: 4px;
  font-size: 10px;
  cursor: pointer;
}

.sim-active-card dl {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 6px 10px;
  margin: 9px 0 0;
}

.sim-active-card dl > div {
  min-width: 0;
}

.sim-active-card dt {
  color: var(--c-overlay0);
  font-size: 9.5px;
}

.sim-active-card dd {
  margin: 2px 0 0;
  overflow: hidden;
  color: var(--c-subtext0);
  font: 500 11px/1.3 ui-monospace, "SF Mono", Menlo, monospace;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.sim-active-card .sim-current-value {
  color: var(--c-green);
  font-weight: 700;
}

.sim-drawer-enter-active,
.sim-drawer-leave-active {
  transition: background-color 220ms ease, backdrop-filter 220ms ease;
}

.sim-drawer-enter-active .sim-drawer,
.sim-drawer-leave-active .sim-drawer {
  transition: transform 280ms cubic-bezier(0.32, 0.72, 0, 1), opacity 200ms ease;
}

.sim-drawer-enter-from,
.sim-drawer-leave-to {
  background: color-mix(in srgb, var(--c-crust) 0%, transparent);
  backdrop-filter: blur(0);
}

.sim-drawer-enter-from .sim-drawer,
.sim-drawer-leave-to .sim-drawer {
  transform: translateX(100%);
  opacity: 0.6;
}
</style>
