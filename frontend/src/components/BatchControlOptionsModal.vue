<script setup lang="ts">
// 批量设置控制点 QU/QL 限定词与 S/E 执行模式(issue #28)。
// 作用于表格当前多选中的控制点;两个字段可独立勾选是否应用,
// 未勾选的字段保持各点原值。
import { ref, watch, inject, computed } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { dialogKey } from '@shared/composables/useDialog'
import type { showAlert as ShowAlert } from '@shared/composables/useDialog'
import { useI18n } from '@shared/i18n'
import AppButton from '@shared/components/ui/AppButton.vue'
import AppCheckbox from '@shared/components/ui/AppCheckbox.vue'
import type { DataPointInfo } from '../types'

const { t } = useI18n()
const { showAlert } = inject<{ showAlert: typeof ShowAlert }>(dialogKey)!

interface Props {
  visible: boolean
  serverId: string
  commonAddress: number
  points: DataPointInfo[]
}

const props = defineProps<Props>()
const emit = defineEmits<{
  close: []
  applied: []
}>()

const applyQualifier = ref(true)
const qualifierChoice = ref<'any' | '0' | '1' | '2' | '3' | 'custom'>('any')
const qualifierCustom = ref<number | undefined>(undefined)
const applySbo = ref(false)
const sboChoice = ref<boolean | undefined>(undefined)
const isSaving = ref(false)

// 选区同时含命令 (QU 0..31) 与设定值 (QL 0..127) 时取共同上限 31。
const hasCommandPoint = computed(() => props.points.some(p => !p.asdu_type.startsWith('C_SE')))
const hasSetpointOnly = computed(() => props.points.length > 0 && !hasCommandPoint.value)
const qualifierMax = computed(() => (hasCommandPoint.value ? 31 : 127))
const qualifierPresets = computed(() =>
  hasSetpointOnly.value
    ? [{ key: '0' as const, label: t('pointModal.ql0') }]
    : [
        { key: '0' as const, label: t('pointModal.qu0') },
        { key: '1' as const, label: t('pointModal.qu1') },
        { key: '2' as const, label: t('pointModal.qu2') },
        { key: '3' as const, label: t('pointModal.qu3') },
      ]
)

watch(() => props.visible, (visible) => {
  if (visible) {
    applyQualifier.value = true
    qualifierChoice.value = 'any'
    qualifierCustom.value = undefined
    applySbo.value = false
    sboChoice.value = undefined
    isSaving.value = false
  }
})

async function handleApply() {
  if (!applyQualifier.value && !applySbo.value) {
    emit('close')
    return
  }
  let qualifier: number | null = null
  if (applyQualifier.value && qualifierChoice.value !== 'any') {
    const q = qualifierChoice.value === 'custom' ? qualifierCustom.value : Number(qualifierChoice.value)
    if (typeof q !== 'number' || q < 0 || q > qualifierMax.value) {
      await showAlert(t('batchControl.qualifierRange', { max: qualifierMax.value }))
      return
    }
    qualifier = q
  }
  isSaving.value = true
  try {
    const applied = await invoke<number>('batch_update_control_options', {
      request: {
        server_id: props.serverId,
        common_address: props.commonAddress,
        points: props.points.map(p => ({ ioa: p.ioa, asdu_type: p.asdu_type })),
        command_qualifier: qualifier,
        set_qualifier: applyQualifier.value,
        select_before_operate: applySbo.value ? sboChoice.value ?? null : null,
        set_select_before_operate: applySbo.value,
      },
    })
    await showAlert(t('batchControl.appliedResult', { applied, total: props.points.length }))
    emit('applied')
  } catch (e) {
    await showAlert(String(e))
  } finally {
    isSaving.value = false
  }
}
</script>

<template>
  <Teleport to="body">
    <Transition name="dialog-pop">
    <div v-if="visible" class="modal-backdrop dialog-blur">
      <div class="modal">
        <div class="modal-header">
          <span class="modal-title">{{ t('batchControl.title') }}</span>
          <AppButton variant="ghost" icon="x" class="btn-close" :aria-label="t('common.cancel')" @click="$emit('close')" />
        </div>

        <div class="modal-body">
          <div class="form-hint selection-hint">{{ t('batchControl.selectionHint', { count: points.length }) }}</div>

          <div class="form-group">
            <AppCheckbox v-model="applyQualifier" class="check-item">
              {{ t('batchControl.applyQualifier') }}
            </AppCheckbox>
            <div v-if="applyQualifier" class="radio-group indent">
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
            <AppCheckbox v-model="applySbo" class="check-item">
              {{ t('batchControl.applySbo') }}
            </AppCheckbox>
            <div v-if="applySbo" class="radio-group indent">
              <label class="radio-item">
                <input v-model="sboChoice" type="radio" :value="undefined" />
                <span>{{ t('pointModal.executionModeFlexible') }}</span>
              </label>
              <label class="radio-item">
                <input v-model="sboChoice" type="radio" :value="false" />
                <span>{{ t('pointModal.executionModeDirect') }}</span>
              </label>
              <label class="radio-item">
                <input v-model="sboChoice" type="radio" :value="true" />
                <span>{{ t('pointModal.executionModeSbo') }}</span>
              </label>
            </div>
          </div>
        </div>

        <div class="modal-footer">
          <AppButton class="btn-secondary" :disabled="isSaving" @click="$emit('close')">{{ t('common.cancel') }}</AppButton>
          <AppButton variant="primary" class="btn-primary" :disabled="isSaving || (!applyQualifier && !applySbo)" @click="handleApply">
            {{ isSaving ? t('pointModal.saving') : t('batchControl.apply') }}
          </AppButton>
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
  padding: 2px 4px;
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

/* 数字输入仍是原生控件,样式对齐 AppInput */
.form-input[type='number'] {
  width: 100%;
  padding: 6px var(--space-2);
  background: var(--bg-panel);
  border: 1px solid var(--border-strong);
  border-radius: var(--radius-md);
  color: var(--text-primary);
  font-family: inherit;
  font-size: var(--text-md);
  box-sizing: border-box;
}

.form-input[type='number']:focus {
  outline: none;
  border-color: var(--accent);
}

.form-hint {
  color: var(--c-overlay0);
  font-size: 11px;
  line-height: 1.4;
}

.selection-hint {
  margin-bottom: 14px;
}

.check-item {
  margin-bottom: 8px;
}

.radio-group {
  display: flex;
  flex-direction: column;
  gap: 6px;
}

.radio-group.indent {
  margin-left: 22px;
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
  accent-color: var(--accent);
  margin: 0;
}

.radio-custom-input {
  width: 110px;
  padding: 4px 8px;
  font-size: 13px;
}

.modal-footer {
  flex-shrink: 0;
  display: flex;
  justify-content: flex-end;
  gap: 8px;
  padding: 16px 20px;
  border-top: 1px solid var(--c-surface0);
}
</style>
