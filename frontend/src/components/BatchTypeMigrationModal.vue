<script setup lang="ts">
import { computed, inject, ref, watch } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { dialogKey } from '@shared/composables/useDialog'
import type { showAlert as ShowAlert } from '@shared/composables/useDialog'
import { useI18n } from '@shared/i18n'
import AppButton from '@shared/components/ui/AppButton.vue'
import {
  ASDU_TYPE_OPTIONS,
  findAsduTypeOption,
} from '../constants/asduTypes'
import type { DataPointInfo } from '../types'

const props = defineProps<{
  visible: boolean
  serverId: string
  commonAddress: number
  points: DataPointInfo[]
}>()
const emit = defineEmits<{ close: []; applied: [] }>()
const { t } = useI18n()
const { showAlert } = inject<{ showAlert: typeof ShowAlert }>(dialogKey)!

const targetAsduType = ref('')
const isSaving = ref(false)
const sourceCategory = computed(() =>
  findAsduTypeOption(props.points[0]?.asdu_type ?? '')?.category ?? '',
)
const typeOptions = computed(() =>
  ASDU_TYPE_OPTIONS.filter(
    option => option.category === sourceCategory.value && option.value.startsWith('M'),
  ),
)
const targetOption = computed(() =>
  typeOptions.value.find(option => option.value === targetAsduType.value),
)
const changedCount = computed(() =>
  props.points.filter(
    point => findAsduTypeOption(point.asdu_type)?.value !== targetAsduType.value,
  ).length,
)

watch(() => props.visible, (visible) => {
  if (!visible) return
  const current = findAsduTypeOption(props.points[0]?.asdu_type ?? '')?.value
  targetAsduType.value =
    typeOptions.value.find(option => option.value !== current)?.value
    ?? current
    ?? ''
  isSaving.value = false
})

async function applyMigration() {
  if (!targetAsduType.value || changedCount.value === 0) return
  isSaving.value = true
  try {
    const applied = await invoke<number>('batch_migrate_data_point_types', {
      request: {
        server_id: props.serverId,
        common_address: props.commonAddress,
        points: props.points.map(point => ({
          ioa: point.ioa,
          asdu_type: point.asdu_type,
        })),
        target_asdu_type: targetAsduType.value,
      },
    })
    await showAlert(t('batchType.appliedResult', { applied }))
    emit('applied')
  } catch (error) {
    await showAlert(t('batchType.failed', { error: String(error) }))
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
            <span class="modal-title">{{ t('batchType.title') }}</span>
            <AppButton variant="ghost" icon="x" class="btn-close" :aria-label="t('common.cancel')" @click="$emit('close')" />
          </div>
          <div class="modal-body">
            <p class="selection-hint">
              {{ t('batchType.selectionHint', { count: points.length }) }}
            </p>
            <label class="form-group">
              <span class="form-label">{{ t('batchType.targetType') }}</span>
              <select v-model="targetAsduType" class="form-select">
                <option
                  v-for="option in typeOptions"
                  :key="option.value"
                  :value="option.value"
                >
                  {{ t(option.labelKey) }} · {{ option.typeId }}
                </option>
              </select>
            </label>
            <p class="migration-note">
              {{ t('batchType.preserveHint') }}
            </p>
            <p v-if="targetAsduType" class="target-preview">
              {{ targetOption ? `${t(targetOption.labelKey)} · ${targetOption.typeId}` : targetAsduType }}
              · {{ t('batchType.changedCount', { count: changedCount }) }}
            </p>
          </div>
          <div class="modal-footer">
            <AppButton class="btn-secondary" :disabled="isSaving" @click="$emit('close')">
              {{ t('common.cancel') }}
            </AppButton>
            <AppButton
              variant="primary"
              class="btn-primary"
              :disabled="isSaving || !targetAsduType || changedCount === 0"
              @click="applyMigration"
            >
              {{ isSaving ? t('pointModal.saving') : t('batchType.apply') }}
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
  z-index: 2000;
  display: flex;
  align-items: center;
  justify-content: center;
  background: rgba(0, 0, 0, 0.55);
}
.modal {
  width: 420px;
  max-width: 90vw;
  overflow: hidden;
  background: var(--c-base);
  border: 1px solid var(--c-surface1);
  border-radius: 8px;
  box-shadow: 0 8px 24px rgba(0, 0, 0, 0.5);
}
.modal-header,
.modal-footer {
  display: flex;
  align-items: center;
  padding: 16px 20px;
}
.modal-header {
  justify-content: space-between;
  border-bottom: 1px solid var(--c-surface0);
}
.modal-footer {
  justify-content: flex-end;
  gap: 8px;
  border-top: 1px solid var(--c-surface0);
}
.modal-title {
  color: var(--c-text);
  font-size: 16px;
  font-weight: 600;
}
.btn-close {
  padding: 0 4px;
  color: var(--c-overlay0);
  line-height: 1;
}
.modal-body { padding: 20px; }
.selection-hint,
.migration-note,
.target-preview {
  margin: 0 0 14px;
  color: var(--c-overlay0);
  font-size: 12px;
  line-height: 1.5;
}
.form-group { display: block; margin-bottom: 14px; }
.form-label {
  display: block;
  margin-bottom: 6px;
  color: var(--c-overlay0);
  font-size: 13px;
}
.form-select {
  width: 100%;
  box-sizing: border-box;
  padding: 6px var(--space-2);
  color: var(--text-primary);
  background: var(--bg-panel);
  border: 1px solid var(--border-strong);
  border-radius: var(--radius-md);
  font-family: inherit;
  font-size: var(--text-md);
}
.target-preview { color: var(--c-sapphire); }
</style>
