<script setup lang="ts">
import { inject, computed, ref, watch, type Ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { dialogKey } from '@shared/composables/useDialog'
import type { showAlert as ShowAlert } from '@shared/composables/useDialog'
import type { DataPointInfo } from '../types'
import { categoryKeyOf } from '../types'
import { useI18n, localizeCategoryLabel } from '@shared/i18n'
import EmptyState from '@shared/components/EmptyState.vue'
import QualityIndicator from '@shared/components/QualityIndicator.vue'

const { t } = useI18n()
const { showAlert } = inject<{ showAlert: typeof ShowAlert }>(dialogKey)!
const selectedServerId = inject<Ref<string | null>>('selectedServerId')!
const selectedCA = inject<Ref<number | null>>('selectedCA')!
const selectedPoints = inject<Ref<{ ioa: number; asdu_type: string; value: string }[]>>('selectedPoints')!

const hasSelection = computed(() => selectedPoints.value.length > 0)
const isSingle = computed(() => selectedPoints.value.length === 1)
const firstPoint = computed(() => selectedPoints.value[0] ?? null)

// Detailed info for the selected point
const pointDetail = ref<DataPointInfo | null>(null)

// 当前点的 5 个品质位(供 QualityIndicator 展示/编辑)
const quality = computed(() => ({
  ov: pointDetail.value?.quality_ov ?? false,
  bl: pointDetail.value?.quality_bl ?? false,
  sb: pointDetail.value?.quality_sb ?? false,
  nt: pointDetail.value?.quality_nt ?? false,
  iv: pointDetail.value?.quality_iv ?? false,
}))
// OV 仅测量类(M_ME_*)适用
const isMeasured = computed(() => pointDetail.value?.asdu_type?.startsWith('M_ME') ?? false)
// M_ME_ND_1 不携带品质描述词:整组品质开关隐藏,只显示中性占位
const isNoQuality = computed(() => pointDetail.value?.asdu_type === 'M_ME_ND_1')

async function toggleQuality(bit: 'ov' | 'bl' | 'sb' | 'nt' | 'iv') {
  const p = pointDetail.value
  if (!selectedServerId.value || selectedCA.value === null || !p) return
  const cur = quality.value
  const next = { ...cur, [bit]: !cur[bit] }
  // 乐观更新本地详情;失败回滚
  pointDetail.value = {
    ...p,
    quality_ov: next.ov, quality_bl: next.bl, quality_sb: next.sb,
    quality_nt: next.nt, quality_iv: next.iv,
  }
  try {
    await invoke('set_data_point_quality', {
      serverId: selectedServerId.value,
      commonAddress: selectedCA.value,
      ioa: p.ioa,
      asduType: p.asdu_type,
      ov: next.ov, bl: next.bl, sb: next.sb, nt: next.nt, iv: next.iv,
    })
  } catch (e) {
    pointDetail.value = { ...p }
    await showAlert(String(e))
  }
}

watch(
  () => [selectedServerId.value, selectedCA.value, selectedPoints.value] as const,
  async ([serverId, ca, points]) => {
    if (!serverId || ca === null || points.length !== 1) {
      pointDetail.value = null
      return
    }
    try {
      // 单点查询:直接取该点详情,避免在大点位场景(上万点)全量拉取
      // list_data_points 再 find(注释见下方 writeValue,单次耗时数百 ms)。
      pointDetail.value = await invoke<DataPointInfo | null>('get_data_point', {
        serverId,
        commonAddress: ca,
        ioa: points[0].ioa,
        asduType: points[0].asdu_type,
      })
    } catch {
      pointDetail.value = null
    }
  },
  { immediate: true },
)

// Editing state
const isEditing = ref(false)
const editValue = ref('')

function startEdit() {
  if (!firstPoint.value) return
  editValue.value = firstPoint.value.value
  isEditing.value = true
}

function cancelEdit() {
  isEditing.value = false
}

watch(selectedPoints, () => {
  isEditing.value = false
})

async function writeValue() {
  if (!selectedServerId.value || selectedCA.value === null || !firstPoint.value) return
  isEditing.value = false
  try {
    await invoke('update_data_point', {
      serverId: selectedServerId.value,
      commonAddress: selectedCA.value,
      ioa: firstPoint.value.ioa,
      asduType: pointDetail.value?.asdu_type ?? '',
      value: editValue.value,
    })
    // 不立即 refreshData：list_data_points 在大数据点场景下耗时数百 ms，
    // 立即触发会卡 UI；2s polling 自然跟上即可。本面板的 pointDetail
    // 也通过下一次 polling 重读。
    if (pointDetail.value) {
      pointDetail.value = { ...pointDetail.value, value: editValue.value }
    }
  } catch (e) {
    await showAlert(String(e))
  }
}

// ── 批量编辑(多选)──────────────────────────────────────────────
type QualityBits = { ov: boolean; bl: boolean; sb: boolean; nt: boolean; iv: boolean }
const batchQuality = ref<QualityBits>({ ov: false, bl: false, sb: false, nt: false, iv: false })
const batchValue = ref('')

// 选中点是否全同分类(批量写值前提)
const allSameCategory = computed(() => {
  const pts = selectedPoints.value
  if (pts.length === 0) return false
  const k = categoryKeyOf(pts[0].asdu_type)
  return pts.every((p) => categoryKeyOf(p.asdu_type) === k)
})
// 选中点是否全为测量类(OV 适用)
const allMeasured = computed(
  () => selectedPoints.value.length > 0 && selectedPoints.value.every((p) => p.asdu_type.startsWith('M_ME')),
)

function batchToggleQuality(bit: keyof QualityBits) {
  batchQuality.value = { ...batchQuality.value, [bit]: !batchQuality.value[bit] }
}

function batchTargets() {
  return selectedPoints.value.map((p) => ({ ioa: p.ioa, asdu_type: p.asdu_type }))
}

async function applyBatchQuality() {
  if (!selectedServerId.value || selectedCA.value === null || selectedPoints.value.length === 0) return
  const q = batchQuality.value
  try {
    await invoke('batch_set_data_point_quality', {
      serverId: selectedServerId.value,
      commonAddress: selectedCA.value,
      points: batchTargets(),
      ov: q.ov, bl: q.bl, sb: q.sb, nt: q.nt, iv: q.iv,
    })
    batchQuality.value = { ov: false, bl: false, sb: false, nt: false, iv: false }
  } catch (e) {
    await showAlert(String(e))
  }
}

async function applyBatchValue() {
  if (!selectedServerId.value || selectedCA.value === null || !batchValue.value || !allSameCategory.value) return
  try {
    await invoke('batch_update_data_points', {
      serverId: selectedServerId.value,
      commonAddress: selectedCA.value,
      points: batchTargets(),
      value: batchValue.value,
    })
    batchValue.value = ''
  } catch (e) {
    await showAlert(String(e))
  }
}

// 切换选择时清空批量草稿
watch(selectedPoints, () => {
  batchQuality.value = { ov: false, bl: false, sb: false, nt: false, iv: false }
  batchValue.value = ''
})

function handleEditKeydown(e: KeyboardEvent) {
  if (e.key === 'Enter') {
    e.preventDefault()
    writeValue()
  } else if (e.key === 'Escape') {
    e.preventDefault()
    cancelEdit()
  }
}
</script>

<template>
  <div class="value-panel">
    <div class="panel-header">{{ t('valuePanel.title') }}</div>

    <EmptyState
      v-if="!hasSelection"
      compact
      :title="t('valuePanel.selectPointHint')"
      :hint="t('valuePanel.selectPointHintSub')"
    >
      <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round">
        <rect x="4" y="3" width="16" height="18" rx="2" />
        <path d="M8 8h8M8 12h8M8 16h5" />
      </svg>
    </EmptyState>

    <template v-else-if="isSingle && pointDetail">
      <!-- Single point detail -->
      <div class="detail-section">
        <div class="section-title">{{ t('valuePanel.sectionInfo') }}</div>
        <div class="detail-row">
          <span class="detail-label">IOA</span>
          <span class="detail-value mono">{{ pointDetail.ioa }}</span>
        </div>
        <div class="detail-row">
          <span class="detail-label">{{ t('valuePanel.asduType') }}</span>
          <span class="detail-value">{{ pointDetail.asdu_type }}</span>
        </div>
        <div class="detail-row">
          <span class="detail-label">{{ t('valuePanel.category') }}</span>
          <span class="detail-value">{{ localizeCategoryLabel(pointDetail.category) }}</span>
        </div>
        <div class="detail-row">
          <span class="detail-label">{{ t('valuePanel.name') }}</span>
          <span class="detail-value">{{ pointDetail.name || '-' }}</span>
        </div>
        <div v-if="pointDetail.comment" class="detail-row">
          <span class="detail-label">{{ t('valuePanel.comment') }}</span>
          <span class="detail-value">{{ pointDetail.comment }}</span>
        </div>
      </div>

      <div class="detail-section">
        <div class="section-title">{{ t('valuePanel.sectionCurrent') }}</div>
        <div class="detail-row">
          <span class="detail-label">{{ t('valuePanel.value') }}</span>
          <span class="detail-value mono editable" @click="startEdit">{{ pointDetail.value }}</span>
        </div>
        <div class="detail-row">
          <span class="detail-label">{{ t('valuePanel.quality') }}</span>
          <span class="detail-value">
            <span v-if="isNoQuality" class="quality-na">{{ t('valuePanel.qualityNa') }}</span>
            <QualityIndicator v-else :quality="quality" editable :show-ov="isMeasured" @toggle="toggleQuality" />
          </span>
        </div>
        <div class="detail-row">
          <span class="detail-label">{{ t('valuePanel.timestamp') }}</span>
          <span class="detail-value mono">{{ pointDetail.timestamp || '-' }}</span>
        </div>
      </div>

      <div class="detail-section">
        <div class="section-title">{{ t('valuePanel.sectionWrite') }}</div>
        <div class="write-row">
          <input
            v-model="editValue"
            class="write-input"
            type="text"
            :placeholder="t('valuePanel.valuePlaceholder')"
            @keydown="handleEditKeydown"
          />
          <button class="write-btn" @click="writeValue">{{ t('valuePanel.write') }}</button>
        </div>
      </div>
    </template>

    <template v-else>
      <!-- Multiple selection: 批量编辑 -->
      <div class="multi-info">
        <div class="detail-section">
          <div class="section-title">{{ t('valuePanel.sectionMultiSelect') }}</div>
          <div class="detail-row">
            <span class="detail-label">{{ t('valuePanel.countLabel') }}</span>
            <span class="detail-value">{{ selectedPoints.length }} {{ t('table.countSuffix') }}</span>
          </div>

          <!-- 批量品质 -->
          <div class="detail-row">
            <span class="detail-label">{{ t('valuePanel.quality') }}</span>
            <span class="detail-value">
              <QualityIndicator :quality="batchQuality" editable :show-ov="allMeasured" @toggle="batchToggleQuality" />
            </span>
          </div>
          <div class="write-row">
            <button class="write-btn" @click="applyBatchQuality">{{ t('valuePanel.applyQuality') }}</button>
          </div>

          <!-- 批量写值(仅同分类启用)-->
          <div class="write-row">
            <input
              v-model="batchValue"
              class="write-input"
              type="text"
              :placeholder="t('valuePanel.valuePlaceholder')"
              :disabled="!allSameCategory"
            />
            <button class="write-btn" :disabled="!allSameCategory" @click="applyBatchValue">
              {{ t('valuePanel.applyValue') }}
            </button>
          </div>
          <div v-if="!allSameCategory" class="batch-hint">{{ t('valuePanel.batchValueMixedHint') }}</div>

          <div class="ioa-list">
            <span v-for="p in selectedPoints" :key="`${p.ioa}-${p.asdu_type}`" class="ioa-chip">
              {{ p.ioa }}
            </span>
          </div>
        </div>
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

.detail-section {
  padding: 8px 0;
  border-bottom: 1px solid var(--c-surface0);
}

.section-title {
  padding: 4px 16px;
  font-size: 11px;
  color: var(--c-blue);
  text-transform: uppercase;
  font-weight: 600;
}

.detail-row {
  display: flex;
  justify-content: space-between;
  align-items: center;
  padding: 4px 16px;
}

.detail-label {
  color: var(--c-overlay0);
  font-size: 12px;
  flex-shrink: 0;
}

.detail-value {
  color: var(--c-text);
  font-size: 12px;
  text-align: right;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.detail-value.mono {
  font-family: var(--font-mono);
}

.detail-value.editable {
  cursor: pointer;
  border-radius: 3px;
  padding: 0 4px;
  user-select: none;
}

.detail-value.editable:hover {
  background: var(--c-surface0);
}

.quality-na {
  color: var(--c-overlay0);
  font-size: 12px;
  font-style: italic;
}

.quality-badge {
  display: inline-block;
  padding: 1px 8px;
  border-radius: 3px;
  font-size: 11px;
  font-weight: 600;
}

.quality-badge.ok {
  background: var(--c-green);
  color: var(--c-base);
}

.quality-badge.invalid {
  background: var(--c-red);
  color: var(--c-base);
}

.write-row {
  display: flex;
  gap: 6px;
  padding: 6px 16px;
}

.write-input {
  flex: 1;
  padding: 6px 10px;
  background: var(--c-crust);
  border: 1px solid var(--c-surface1);
  border-radius: 6px;
  color: var(--c-text);
  font-family: var(--font-mono);
  font-size: 13px;
  outline: none;
  box-sizing: border-box;
}

.write-input:focus {
  border-color: var(--c-blue);
}

.write-btn {
  padding: 6px 14px;
  background: var(--c-blue);
  color: var(--c-base);
  border: none;
  border-radius: 6px;
  font-size: 12px;
  font-weight: 600;
  cursor: pointer;
  white-space: nowrap;
}

.write-btn:hover {
  background: var(--c-sapphire);
}

.write-btn:disabled,
.write-input:disabled {
  opacity: 0.45;
  cursor: not-allowed;
}

.batch-hint {
  padding: 2px 16px 6px;
  font-size: 11px;
  color: var(--c-overlay0);
}

.ioa-list {
  display: flex;
  flex-wrap: wrap;
  gap: 4px;
  padding: 6px 16px;
}

.ioa-chip {
  padding: 2px 8px;
  background: var(--c-surface0);
  border-radius: 4px;
  font-family: var(--font-mono);
  font-size: 11px;
  color: var(--c-blue);
}
</style>
