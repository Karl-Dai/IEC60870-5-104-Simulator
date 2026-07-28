<script setup lang="ts">
import { ref, computed, watch, inject, onUnmounted } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { dialogKey } from '@shared/composables/useDialog'
import type { showAlert as ShowAlert } from '@shared/composables/useDialog'
import { useI18n, localizeCategoryLabel } from '@shared/i18n'
import type { DataPointInfo } from '../types'
import { compressRanges, parseIoaExpression, resolveIoaHits } from './batchAdd/ioaRanges'

const { t } = useI18n()
const { showAlert } = inject<{ showAlert: typeof ShowAlert }>(dialogKey)!

interface Props {
  visible: boolean
  serverId: string
  commonAddress: number
  // 父级传入本站全部点（IOA 升序），(ioa, asdu_type) 在上游唯一。
  existingPoints: ReadonlyArray<Pick<DataPointInfo, 'ioa' | 'asdu_type' | 'category'>>
  // 默认选中类型（取当前表格分类对应类型）；空则取首个可用类型。
  defaultType?: string
}
const props = defineProps<Props>()
const emit = defineEmits<{ close: []; written: [] }>()

const asduType = ref('')
const ioaText = ref('')
const value = ref('')
const isSaving = ref(false)

// 本站实际存在的 asdu_type（去重），带其中文 category 作下拉 label。
const typeOptions = computed(() => {
  const seen = new Map<string, string>()
  for (const p of props.existingPoints) {
    if (!seen.has(p.asdu_type)) seen.set(p.asdu_type, p.category)
  }
  return Array.from(seen, ([type, category]) => ({ type, category }))
    .sort((a, b) => a.type.localeCompare(b.type))
})

// 选定类型下的已存在 IOA，升序去重（喂给 resolveIoaHits）。
const existingIoas = computed<number[]>(() => {
  const xs = props.existingPoints
    .filter((p) => p.asdu_type === asduType.value)
    .map((p) => p.ioa)
  xs.sort((a, b) => a - b)
  return xs
})

const parsed = computed(() => parseIoaExpression(ioaText.value))
const hits = computed(() => resolveIoaHits(parsed.value, existingIoas.value))

const hasExpr = computed(() => ioaText.value.trim().length > 0)
const parseError = computed(() => parsed.value.error)
const hitCount = computed(() => hits.value.hitIoas.length)
const missedCount = computed(() => hits.value.missedSingles.length)
const hitRangesText = computed(() => compressRanges(hits.value.hitIoas))
const missedRangesText = computed(() => compressRanges(hits.value.missedSingles))

const canWrite = computed(
  () => !isSaving.value && !parseError.value && hitCount.value > 0 && value.value.trim().length > 0,
)

function valuePlaceholder(type: string): string {
  if (/^M_SP_/.test(type)) return t('batchWrite.phSingle')
  if (/^M_DP_/.test(type)) return t('batchWrite.phDouble')
  if (/^M_ST_/.test(type)) return t('batchWrite.phStep')
  if (/^M_BO_/.test(type)) return t('batchWrite.phBitstring')
  if (/^M_ME_(NA|ND|TD)/.test(type)) return t('batchWrite.phNormalized')
  if (/^M_ME_(NB|TE)/.test(type)) return t('batchWrite.phScaled')
  if (/^M_ME_(NC|TF)/.test(type)) return t('batchWrite.phFloat')
  if (/^M_IT_/.test(type)) return t('batchWrite.phTotal')
  return ''
}

// immediate: true —— 组件常驻挂载（visible 初始 false），需在每次 visible 转 true
// 时初始化；且测试以 visible=true 挂载，无 immediate 则 watch 不触发、asduType 永空。
// 键盘监听挂 window（仓库既有约定，见 RemoteParamsModal.vue）；backdrop div 无 tabindex
// 不可聚焦，挂 @keydown 在未聚焦内部元素时不触发。
watch(
  () => props.visible,
  (v) => {
    if (v) {
      ioaText.value = ''
      value.value = ''
      isSaving.value = false
      asduType.value = props.defaultType || typeOptions.value[0]?.type || ''
      window.addEventListener('keydown', handleKeydown)
    } else {
      window.removeEventListener('keydown', handleKeydown)
    }
  },
  { immediate: true },
)
onUnmounted(() => window.removeEventListener('keydown', handleKeydown))

async function handleWrite() {
  if (!canWrite.value) return
  isSaving.value = true
  try {
    const points = hits.value.hitIoas.map((ioa) => ({ ioa, asdu_type: asduType.value }))
    await invoke('batch_update_data_points', {
      serverId: props.serverId,
      commonAddress: props.commonAddress,
      points,
      value: value.value,
    })
    emit('written')
  } catch (e) {
    await showAlert(t('batchWrite.failedPrefix', { err: String(e) }))
  } finally {
    isSaving.value = false
  }
}

function handleBackdropClick(e: MouseEvent) {
  if ((e.target as HTMLElement).classList.contains('modal-backdrop')) emit('close')
}

function handleKeydown(e: KeyboardEvent) {
  if (e.key === 'Escape') {
    emit('close')
  } else if ((e.metaKey || e.ctrlKey) && e.key === 'Enter') {
    e.preventDefault()
    handleWrite()
  }
}
</script>

<template>
  <Teleport to="body">
    <Transition name="dialog-pop">
      <div v-if="visible" class="modal-backdrop dialog-blur" @click="handleBackdropClick">
        <div class="modal">
          <div class="modal-header">
            <span class="modal-title">{{ t('batchWrite.title') }}</span>
            <button class="btn-close" @click="$emit('close')">×</button>
          </div>

          <div class="modal-body">
            <div class="form-group">
              <label class="form-label">{{ t('batchWrite.typeLabel') }}</label>
              <select v-model="asduType" class="form-select">
                <option v-for="opt in typeOptions" :key="opt.type" :value="opt.type">
                  {{ localizeCategoryLabel(opt.category) }} · {{ opt.type }}
                </option>
              </select>
            </div>

            <div class="form-group">
              <label class="form-label">{{ t('batchWrite.ioaLabel') }}</label>
              <textarea
                v-model="ioaText"
                class="form-input ioa-textarea"
                rows="3"
                :placeholder="t('batchWrite.ioaPlaceholder')"
              />
              <div v-if="hasExpr" class="summary-card">
                <div v-if="parseError" class="summary-card__conflict no-border">
                  {{ t('batchWrite.parseError', { token: parseError }) }}
                </div>
                <template v-else>
                  <div class="summary-card__title">
                    <span class="summary-card__count hit-count">{{ t('batchWrite.hit', { count: hitCount }) }}</span>
                    <template v-if="missedCount > 0">
                      <span class="summary-card__sep">·</span>
                      <span class="summary-card__count">{{ t('batchWrite.ignored', { count: missedCount }) }}</span>
                    </template>
                  </div>
                  <div v-if="hitCount > 0" class="summary-card__ranges">
                    <span class="summary-card__ranges-label">IOA</span>
                    <span class="summary-card__ranges-value">{{ hitRangesText }}</span>
                  </div>
                  <div v-if="missedCount > 0" class="summary-card__conflict">
                    {{ t('batchWrite.ignoredDetail', { ranges: missedRangesText }) }}
                  </div>
                </template>
              </div>
            </div>

            <div class="form-group">
              <label class="form-label">{{ t('batchWrite.valueLabel') }}</label>
              <input v-model="value" type="text" class="form-input" :placeholder="valuePlaceholder(asduType)" />
            </div>
          </div>

          <div class="modal-footer">
            <button class="btn btn-secondary" :disabled="isSaving" @click="$emit('close')">
              {{ t('common.cancel') }}
            </button>
            <button class="btn btn-primary" :disabled="!canWrite" @click="handleWrite">
              {{ isSaving ? t('batchWrite.writing') : hitCount > 0 ? t('batchWrite.writeN', { count: hitCount }) : t('batchWrite.write') }}
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
  /* issue #28:长表单时标题/按钮不随内容滚走 —— 滚动收进 .modal-body */
  display: flex;
  flex-direction: column;
  overflow: hidden;
  box-shadow: 0 8px 24px rgba(0, 0, 0, 0.5);
}

.modal-header {
  flex-shrink: 0;
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
  flex: 1 1 auto;
  min-height: 0;
  overflow-y: auto;
}

.form-group {
  margin-bottom: 16px;
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

.ioa-textarea {
  font-family: var(--font-mono);
  resize: vertical;
  min-height: 64px;
  line-height: 1.5;
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

.summary-card__sep {
  color: var(--c-overlay0);
}

.summary-card__count {
  color: var(--c-subtext0);
}

.hit-count {
  color: var(--c-green);
  font-weight: 600;
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

.summary-card__conflict {
  margin-top: 4px;
  padding-top: 6px;
  border-top: 1px dashed var(--c-red);
  color: var(--c-red);
  font-size: 12px;
  font-family: var(--font-mono);
}

.summary-card__conflict.no-border {
  margin-top: 0;
  padding-top: 0;
  border-top: none;
}

.modal-footer {
  flex-shrink: 0;
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
