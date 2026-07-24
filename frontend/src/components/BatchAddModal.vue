<script setup lang="ts">
import { ref, computed, watch, inject } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { dialogKey } from '@shared/composables/useDialog'
import type { showAlert as ShowAlert } from '@shared/composables/useDialog'
import { useI18n } from '@shared/i18n'
import { ASDU_TYPE_OPTIONS, findAsduTypeOption } from '../constants/asduTypes'
import type { DataPointInfo } from '../types'
import {
  IOA_MAX,
  compressRanges,
  lowerBound,
  findNextFreeGap,
  parseIoaExpression,
  expandIoaExpression,
} from './batchAdd/ioaRanges'

const { t } = useI18n()
const { showAlert } = inject<{ showAlert: typeof ShowAlert }>(dialogKey)!

const BATCH_MAX = 100000

interface Props {
  visible: boolean
  serverId: string
  commonAddress: number
  /** 当前左树选中的分类稳定键;提供时类型下拉只显示该分类的类型。 */
  category?: string | null
  // Caller passes the parent's already-IOA-sorted point list. We filter by
  // current ASDU type — same-IOA collisions across different types are not
  // collisions, so we never need the full DataPointInfo, just (ioa, asdu_type).
  existingPoints: ReadonlyArray<Pick<DataPointInfo, 'ioa' | 'asdu_type'>>
}

const props = defineProps<Props>()
const emit = defineEmits<{
  close: []
  added: []
}>()

const ASDU_TYPES = computed(() => {
  const source = props.category
    ? ASDU_TYPE_OPTIONS.filter(o => o.category === props.category)
    : ASDU_TYPE_OPTIONS
  return source.map(o => ({ value: o.value, label: t(o.labelKey), typeId: o.typeId }))
})

// IOA 输入模式:连续(起始+数量)或表达式("6001-6050" / "6001,6003,6012",issue #28)。
const ioaMode = ref<'range' | 'expression'>('range')
const startIoa = ref(0)
const count = ref(10)
const ioaExpression = ref('')
const formAsduType = ref('MSpNa1')
const namePrefix = ref('')
const nameWithTypeId = ref(false)
// 控制点批量创建统一配置 QU/QL 与 S/E(位串命令无这些字段)。
const qualifierChoice = ref<'any' | '0' | '1' | '2' | '3' | 'custom'>('any')
const qualifierCustom = ref<number | undefined>(undefined)
const formSbo = ref<boolean | undefined>(undefined)
const isSaving = ref(false)

const isControlType = computed(() => formAsduType.value.startsWith('C'))
const isBitstringType = computed(() => formAsduType.value.startsWith('CBo'))
const isSetpointType = computed(() => formAsduType.value.startsWith('CSe'))
const qualifierMax = computed(() => (isSetpointType.value ? 127 : 31))
const qualifierPresets = computed(() =>
  isSetpointType.value
    ? [{ key: '0' as const, label: t('pointModal.ql0') }]
    : [
        { key: '0' as const, label: t('pointModal.qu0') },
        { key: '1' as const, label: t('pointModal.qu1') },
        { key: '2' as const, label: t('pointModal.qu2') },
        { key: '3' as const, label: t('pointModal.qu3') },
      ]
)
const qualifierValue = computed<number | null>(() => {
  switch (qualifierChoice.value) {
    case 'any': return null
    case 'custom':
      return typeof qualifierCustom.value === 'number' ? qualifierCustom.value : null
    default: return Number(qualifierChoice.value)
  }
})

const parsedExpr = computed(() => parseIoaExpression(ioaExpression.value))
// null = 超过 BATCH_MAX;空数组 = 语法错或无输入。
const expandedIoas = computed<number[] | null>(() =>
  ioaMode.value === 'expression' ? expandIoaExpression(parsedExpr.value, BATCH_MAX) : null,
)

const endIoa = computed(() => startIoa.value + count.value - 1)

const effectiveCount = computed(() => {
  if (ioaMode.value === 'expression') return expandedIoas.value?.length ?? 0
  return count.value
})

const isValid = computed(() => {
  if (ioaMode.value === 'expression') {
    return !parsedExpr.value.error
      && expandedIoas.value !== null
      && (expandedIoas.value?.length ?? 0) > 0
  }
  return count.value > 0 && count.value <= BATCH_MAX && startIoa.value >= 0
})

// 名称模板示例:prefix_ioa 或 prefix_typeid_ioa(issue #28 建议的 Name_TypeID_IOA)。
const namePatternExample = computed(() => {
  if (!namePrefix.value) return ''
  const sampleIoa = ioaMode.value === 'expression'
    ? expandedIoas.value?.[0] ?? 0
    : startIoa.value
  const typeId = ASDU_TYPE_OPTIONS.find(o => o.value === formAsduType.value)?.typeId ?? 0
  return nameWithTypeId.value
    ? `${namePrefix.value}_${typeId}_${sampleIoa}`
    : `${namePrefix.value}_${sampleIoa}`
})

// existingPoints arrives IOA-sorted from the parent and (ioa, asdu_type) is
// unique upstream (DataPointTable's dataMap is keyed by that pair), so
// filter alone is enough — no Set/sort needed.
// 类型比较须经 findAsduTypeOption 归一化:父组件传的是显示名(M_SP_NA_1),
// 表单值是 PascalCase(MSpNa1),直接 === 永远不相等,冲突提示整体失效。
const existingSameTypeIoas = computed<number[]>(() => {
  const wanted = findAsduTypeOption(formAsduType.value)?.typeId
  if (wanted === undefined) return []
  return props.existingPoints
    .filter(p => findAsduTypeOption(p.asdu_type)?.typeId === wanted)
    .map(p => p.ioa)
})

const existingRangesText = computed<string>(() =>
  compressRanges(existingSameTypeIoas.value),
)

const conflictIoas = computed<number[]>(() => {
  const xs = existingSameTypeIoas.value
  if (xs.length === 0) return []
  if (ioaMode.value === 'expression') {
    const list = expandedIoas.value
    if (!list || list.length === 0) return []
    const set = new Set(xs)
    return list.filter(n => set.has(n))
  }
  if (count.value <= 0 || startIoa.value < 0) return []
  const lo = startIoa.value
  const hi = lo + count.value - 1
  return xs.slice(lowerBound(xs, lo), lowerBound(xs, hi + 1))
})

const conflictCount = computed<number>(() => conflictIoas.value.length)

const conflictRanges = computed<string>(() => compressRanges(conflictIoas.value))

// 同 CASDU 跨类型重复 IOA 警告(issue #28,不阻断创建)。
// 同方向且不同分类的已有点共用目标 IOA 才提示;NA/TA/TB 同分类变体与
// 控制↔监视配对(兼容自动映射)不算。
const crossTypeDupIoas = computed<number[]>(() => {
  const opt = findAsduTypeOption(formAsduType.value)
  if (!opt) return []
  const dir = formAsduType.value.startsWith('C') ? 'c' : 'm'
  const clashSet = new Set<number>()
  for (const p of props.existingPoints) {
    const po = findAsduTypeOption(p.asdu_type)
    if (!po) continue
    if ((po.value.startsWith('C') ? 'c' : 'm') !== dir) continue
    if (po.category === opt.category) continue
    clashSet.add(p.ioa)
  }
  if (clashSet.size === 0) return []
  let target: number[]
  if (ioaMode.value === 'expression') {
    target = expandedIoas.value ?? []
  } else if (count.value > 0 && count.value <= BATCH_MAX && startIoa.value >= 0) {
    target = Array.from({ length: count.value }, (_, i) => startIoa.value + i)
  } else {
    target = []
  }
  return target.filter(n => clashSet.has(n))
})

const crossTypeDupRanges = computed<string>(() => compressRanges(crossTypeDupIoas.value))

const nextAvailableIoa = computed<number | null>(() => {
  const xs = existingSameTypeIoas.value
  if (xs.length === 0) return null
  const next = xs[xs.length - 1] + 1
  return next > IOA_MAX ? null : next
})

const nextFreeGapStart = computed<number | null>(() =>
  count.value > 0 ? findNextFreeGap(existingSameTypeIoas.value, count.value) : null,
)

const canApplyNextIoa = computed(() => nextAvailableIoa.value !== null)
const canApplyNextGap = computed(() => nextFreeGapStart.value !== null)

function applyNextAvailableIoa() {
  if (nextAvailableIoa.value !== null) startIoa.value = nextAvailableIoa.value
}

function applyNextFreeGap() {
  if (nextFreeGapStart.value !== null) startIoa.value = nextFreeGapStart.value
}


watch(() => props.visible, (visible) => {
  if (visible) {
    ioaMode.value = 'range'
    startIoa.value = 0
    count.value = 10
    ioaExpression.value = ''
    formAsduType.value = ASDU_TYPES.value[0]?.value ?? 'MSpNa1'
    namePrefix.value = ''
    nameWithTypeId.value = false
    qualifierChoice.value = 'any'
    qualifierCustom.value = undefined
    formSbo.value = undefined
    isSaving.value = false
  }
})

async function handleConfirm() {
  if (!isValid.value) return
  const withControlOptions = isControlType.value && !isBitstringType.value
  if (withControlOptions && qualifierChoice.value === 'custom') {
    const q = qualifierCustom.value
    if (typeof q !== 'number' || q < 0 || q > qualifierMax.value) {
      await showAlert(t('pointModal.qualifierHint'))
      return
    }
  }
  isSaving.value = true

  try {
    const request: Record<string, unknown> = {
      server_id: props.serverId,
      common_address: props.commonAddress,
      asdu_type: formAsduType.value,
      name_prefix: namePrefix.value || null,
      name_with_type_id: nameWithTypeId.value,
      command_qualifier: withControlOptions ? qualifierValue.value : null,
      select_before_operate: withControlOptions ? formSbo.value ?? null : null,
    }
    if (ioaMode.value === 'expression') {
      request.ioas = expandedIoas.value
    } else {
      request.start_ioa = startIoa.value
      request.count = count.value
    }
    await invoke('batch_add_data_points', { request })
    emit('added')
  } catch (e) {
    await showAlert(t('batchModal.failedPrefix', { err: String(e) }))
  } finally {
    isSaving.value = false
  }
}

function handleBackdropClick(e: MouseEvent) {
  if ((e.target as HTMLElement).classList.contains('modal-backdrop')) {
    emit('close')
  }
}
</script>

<template>
  <Teleport to="body">
    <Transition name="dialog-pop">
    <div v-if="visible" class="modal-backdrop dialog-blur" @click="handleBackdropClick">
      <div class="modal">
        <div class="modal-header">
          <span class="modal-title">{{ t('batchModal.title') }}</span>
          <button class="btn-close" @click="$emit('close')">×</button>
        </div>

        <div class="modal-body">
          <div class="form-group">
            <div class="mode-toggle">
              <button
                type="button"
                :class="{ active: ioaMode === 'range' }"
                @click="ioaMode = 'range'"
              >{{ t('batchModal.modeRange') }}</button>
              <button
                type="button"
                :class="{ active: ioaMode === 'expression' }"
                @click="ioaMode = 'expression'"
              >{{ t('batchModal.modeExpression') }}</button>
            </div>
          </div>

          <div v-if="ioaMode === 'range'" class="form-row">
            <div class="form-group half">
              <label class="form-label">{{ t('batchModal.startIoa') }}</label>
              <input
                v-model.number="startIoa"
                type="number"
                class="form-input"
                min="0"
              />
            </div>
            <div class="form-group half">
              <label class="form-label">{{ t('batchModal.count') }}</label>
              <input
                v-model.number="count"
                type="number"
                class="form-input"
                min="1"
                max="100000"
              />
            </div>
          </div>
          <div v-else class="form-group">
            <label class="form-label">{{ t('batchModal.expressionLabel') }}</label>
            <input
              v-model="ioaExpression"
              type="text"
              class="form-input"
              :placeholder="t('batchModal.expressionPlaceholder')"
            />
            <div v-if="parsedExpr.error" class="expr-error">
              {{ t('batchModal.expressionError', { token: parsedExpr.error }) }}
            </div>
            <div v-else-if="expandedIoas === null" class="expr-error">
              {{ t('batchModal.countWarn') }}
            </div>
            <div v-else-if="expandedIoas.length > 0" class="form-hint">
              {{ t('batchModal.expressionHint', { count: expandedIoas.length }) }}
            </div>
          </div>

          <div class="form-group">
            <label class="form-label">{{ t('batchModal.asduTypeLabel') }}</label>
            <select v-model="formAsduType" class="form-select">
              <option v-for="opt in ASDU_TYPES" :key="opt.value" :value="opt.value">
                {{ opt.label }} · {{ opt.typeId }}
              </option>
            </select>
            <div v-if="existingSameTypeIoas.length > 0" class="summary-card">
              <div class="summary-card__title">
                <span class="summary-card__type">{{ formAsduType }}</span>
                <span class="summary-card__sep">·</span>
                <span class="summary-card__count">
                  {{ t('batchModal.existingSameType', { count: existingSameTypeIoas.length }) }}
                </span>
              </div>
              <div class="summary-card__ranges">
                <span class="summary-card__ranges-label">IOA</span>
                <span class="summary-card__ranges-value">{{ existingRangesText }}</span>
              </div>
              <div v-if="ioaMode === 'range'" class="summary-card__actions">
                <button
                  type="button"
                  class="summary-card__btn"
                  :disabled="!canApplyNextIoa"
                  :title="canApplyNextIoa ? '' : t('batchModal.capacityFullTooltip')"
                  @click="applyNextAvailableIoa"
                >
                  {{ t('batchModal.nextIoaBtn') }}
                </button>
                <button
                  type="button"
                  class="summary-card__btn"
                  :disabled="!canApplyNextGap"
                  :title="canApplyNextGap ? '' : t('batchModal.capacityFullTooltip')"
                  @click="applyNextFreeGap"
                >
                  {{ t('batchModal.nextGapBtn') }}
                </button>
              </div>
              <div v-if="conflictCount > 0" class="summary-card__conflict">
                {{ t('batchModal.conflictDetail', { ranges: conflictRanges, count: conflictCount }) }}
              </div>
              <div v-if="crossTypeDupIoas.length > 0" class="summary-card__conflict summary-card__conflict--warn">
                ⚠ {{ t('batchModal.crossTypeDup', { ranges: crossTypeDupRanges, count: crossTypeDupIoas.length }) }}
              </div>
            </div>
          </div>

          <div class="form-group">
            <label class="form-label">{{ t('batchModal.namePrefix') }}</label>
            <input
              v-model="namePrefix"
              type="text"
              class="form-input"
              :placeholder="t('batchModal.namePrefixPlaceholder')"
            />
            <label class="check-item">
              <input v-model="nameWithTypeId" type="checkbox" />
              <span>{{ t('batchModal.nameWithTypeId') }}</span>
            </label>
            <div v-if="namePatternExample" class="form-hint">
              {{ t('batchModal.namePatternExample', { example: namePatternExample }) }}
            </div>
          </div>

          <template v-if="isControlType && !isBitstringType">
            <div class="form-group">
              <label class="form-label">{{ t('pointModal.qualifierLabel') }}</label>
              <div class="radio-group">
                <label class="radio-item">
                  <input v-model="qualifierChoice" type="radio" value="any" />
                  <span>{{ t('pointModal.quAny') }}</span>
                </label>
                <label v-for="preset in qualifierPresets" :key="preset.key" class="radio-item">
                  <input v-model="qualifierChoice" type="radio" :value="preset.key" />
                  <span>{{ preset.label }}</span>
                </label>
                <label class="radio-item">
                  <input v-model="qualifierChoice" type="radio" value="custom" />
                  <span>{{ t('pointModal.quCustom') }}</span>
                  <input
                    v-if="qualifierChoice === 'custom'"
                    v-model.number="qualifierCustom"
                    type="number"
                    class="form-input radio-custom-input"
                    min="0"
                    :max="qualifierMax"
                    :placeholder="`0..${qualifierMax}`"
                  />
                </label>
              </div>
            </div>
            <div class="form-group">
              <label class="form-label">{{ t('pointModal.executionModeLabel') }}</label>
              <div class="radio-group">
                <label class="radio-item">
                  <input v-model="formSbo" type="radio" :value="undefined" />
                  <span>{{ t('pointModal.executionModeFlexible') }}</span>
                </label>
                <label class="radio-item">
                  <input v-model="formSbo" type="radio" :value="false" />
                  <span>{{ t('pointModal.executionModeDirect') }}</span>
                </label>
                <label class="radio-item">
                  <input v-model="formSbo" type="radio" :value="true" />
                  <span>{{ t('pointModal.executionModeSbo') }}</span>
                </label>
              </div>
            </div>
          </template>

          <div class="count-info">
            <span v-if="ioaMode === 'range' && count > 100000" class="count-warn">{{ t('batchModal.countWarn') }}</span>
            <template v-else-if="ioaMode === 'range'">
              <span>{{ t('batchModal.rangeHint', { startIoa, endIoa, count }) }}</span>
            </template>
            <template v-else-if="effectiveCount > 0">
              <span>{{ t('batchModal.expressionHint', { count: effectiveCount }) }}</span>
            </template>
          </div>

        </div>

        <div class="modal-footer">
          <button class="btn btn-secondary" @click="$emit('close')" :disabled="isSaving">{{ t('common.cancel') }}</button>
          <button class="btn btn-primary" @click="handleConfirm" :disabled="!isValid || isSaving">
            {{ isSaving ? t('batchModal.saving') : t('batchModal.add') }}
          </button>
        </div>
      </div>
    </div>
    </Transition>
  </Teleport>
</template>

<style scoped>
.modal-backdrop {
  position: fixed;
  inset: 0;
  background: rgba(0, 0, 0, 0.55);
  display: flex;
  align-items: center;
  justify-content: center;
  z-index: 2000;
}

.modal {
  background: var(--c-base);
  border: 1px solid var(--c-surface1);
  border-radius: 8px;
  width: 420px;
  max-width: 90vw;
  max-height: 90vh;
  overflow-y: auto;
  box-shadow: 0 8px 24px rgba(0, 0, 0, 0.5);
}

.modal-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  padding: 16px 20px;
  border-bottom: 1px solid var(--c-surface0);
}

.modal-title {
  font-size: 16px;
  font-weight: 600;
  color: var(--c-text);
}

.btn-close {
  background: none;
  border: none;
  color: var(--c-overlay0);
  font-size: 20px;
  cursor: pointer;
  padding: 0 4px;
  line-height: 1;
}

.btn-close:hover {
  color: var(--c-text);
}

.modal-body {
  padding: 20px;
}

.form-row {
  display: flex;
  gap: 12px;
}

.form-group {
  margin-bottom: 16px;
}

.form-group.half {
  flex: 1;
}

.form-label {
  display: block;
  font-size: 13px;
  color: var(--c-overlay0);
  margin-bottom: 6px;
}

.form-input,
.form-select {
  width: 100%;
  padding: 8px 12px;
  background: var(--c-crust);
  border: 1px solid var(--c-surface1);
  border-radius: 6px;
  color: var(--c-text);
  font-size: 14px;
  box-sizing: border-box;
}

.form-input:focus,
.form-select:focus {
  outline: none;
  border-color: var(--c-blue);
}

.count-info {
  font-size: 13px;
  color: var(--c-subtext0);
  padding: 8px 0;
}

.mode-toggle {
  display: inline-flex;
  border: 1px solid var(--c-surface1);
  border-radius: 6px;
  overflow: hidden;
}

.mode-toggle button {
  padding: 6px 14px;
  font-size: 13px;
  background: var(--c-crust);
  border: none;
  color: var(--c-subtext0);
  cursor: pointer;
}

.mode-toggle button.active {
  background: var(--c-blue);
  color: var(--c-base);
  font-weight: 600;
}

.expr-error {
  margin-top: 6px;
  color: var(--c-red);
  font-size: 12px;
  font-family: var(--font-mono);
}

.form-hint {
  margin-top: 6px;
  color: var(--c-overlay0);
  font-size: 11px;
  line-height: 1.4;
}

.check-item {
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: 13px;
  color: var(--c-text);
  cursor: pointer;
  margin-top: 8px;
}

.check-item input[type='checkbox'] {
  accent-color: var(--c-blue);
  margin: 0;
}

.radio-group {
  display: flex;
  flex-direction: column;
  gap: 6px;
}

.radio-item {
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: 13px;
  color: var(--c-text);
  cursor: pointer;
}

.radio-item input[type='radio'] {
  accent-color: var(--c-blue);
  margin: 0;
}

.radio-custom-input {
  width: 110px;
  padding: 4px 8px;
  font-size: 13px;
}

.count-info strong {
  color: var(--c-green);
}

.count-warn {
  color: var(--c-red);
}

.summary-card {
  margin-top: 6px;
  padding: 10px 12px;
  background: var(--c-mantle);
  border: 1px solid var(--c-surface1);
  border-radius: 6px;
  display: flex;
  flex-direction: column;
  gap: 6px;
}

.summary-card__title {
  display: flex;
  align-items: center;
  gap: 6px;
  font-size: 13px;
}

.summary-card__type {
  font-weight: 600;
  color: var(--c-text);
}

.summary-card__sep {
  color: var(--c-overlay0);
}

.summary-card__count {
  color: var(--c-subtext0);
}

.summary-card__ranges {
  display: flex;
  align-items: baseline;
  gap: 6px;
  font-size: 12px;
}

.summary-card__ranges-label {
  color: var(--c-overlay0);
}

.summary-card__ranges-value {
  font-family: var(--font-mono);
  color: var(--c-text);
  word-break: break-all;
}

.summary-card__actions {
  display: flex;
  gap: 6px;
  flex-wrap: wrap;
}

.summary-card__btn {
  padding: 4px 10px;
  font-size: 12px;
  background: var(--c-surface0);
  border: 1px solid var(--c-surface1);
  color: var(--c-text);
  border-radius: 4px;
  cursor: pointer;
}

.summary-card__btn:hover:not(:disabled) {
  background: var(--c-surface1);
}

.summary-card__btn:disabled {
  opacity: 0.45;
  cursor: not-allowed;
}

.summary-card__conflict {
  margin-top: 4px;
  padding-top: 6px;
  border-top: 1px dashed var(--c-red);
  color: var(--c-red);
  font-size: 12px;
  font-family: var(--font-mono);
}

/* 跨类型重复 IOA:仅警示,不阻断创建(issue #28) */
.summary-card__conflict--warn {
  border-top-color: var(--c-peach);
  color: var(--c-peach);
}

.modal-footer {
  display: flex;
  justify-content: flex-end;
  gap: 8px;
  padding: 16px 20px;
  border-top: 1px solid var(--c-surface0);
}

.btn {
  padding: 8px 20px;
  border: none;
  border-radius: 6px;
  cursor: pointer;
  font-size: 14px;
}

.btn-primary {
  background: var(--c-blue);
  color: var(--c-base);
  font-weight: 600;
}

.btn-primary:hover {
  background: var(--c-sapphire);
}

.btn-primary:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

.btn-secondary {
  background: var(--c-surface1);
  color: var(--c-text);
}

.btn-secondary:hover {
  background: var(--c-surface2);
}

.btn-secondary:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}
</style>
