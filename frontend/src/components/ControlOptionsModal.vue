<script setup lang="ts">
import { ref, watch, inject, computed } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { dialogKey } from '@shared/composables/useDialog'
import type { showAlert as ShowAlert } from '@shared/composables/useDialog'
import { useI18n } from '@shared/i18n'

const { t } = useI18n()
const { showAlert } = inject<{ showAlert: typeof ShowAlert }>(dialogKey)!

interface ControlPointRef {
  ioa: number
  asdu_type: string
}

interface Props {
  visible: boolean
  serverId: string
  commonAddress: number
  /** 选中的遥控点(调用方保证均为 C_* 且不含 C_BO_*)。 */
  points: ControlPointRef[]
}

const props = defineProps<Props>()
const emit = defineEmits<{
  close: []
  applied: []
}>()

// QU 族(单/双/步命令,0..31 含标准预设)与 QL 族(设点命令,0..127 自由数值)
// 语义不同:混合选择时隐藏限定词区,只允许批量改 S/E。
const allQu = computed(() =>
  props.points.length > 0 && props.points.every(p => /^C_(SC|DC|RC)_/.test(p.asdu_type)),
)
const allQl = computed(() =>
  props.points.length > 0 && props.points.every(p => p.asdu_type.startsWith('C_SE_')),
)
const mixedFamily = computed(() => !allQu.value && !allQl.value)

const applyQualifier = ref(false)
const applySbo = ref(false)
type QuMode = 'unset' | '0' | '1' | '2' | '3' | 'custom'
const quMode = ref<QuMode>('unset')
const customQu = ref<number | undefined>(undefined)
const qlValue = ref<number | undefined>(undefined)
type SeMode = 'flexible' | 'direct' | 'sbo'
const seMode = ref<SeMode>('flexible')
const isSaving = ref(false)

watch(() => props.visible, (visible) => {
  if (visible) {
    applyQualifier.value = false
    applySbo.value = false
    quMode.value = 'unset'
    customQu.value = undefined
    qlValue.value = undefined
    seMode.value = 'flexible'
    isSaving.value = false
  }
})

function resolveQualifier(): number | null {
  if (allQl.value) {
    return typeof qlValue.value === 'number' ? qlValue.value : null
  }
  if (quMode.value === 'unset') return null
  if (quMode.value === 'custom') {
    return typeof customQu.value === 'number' ? customQu.value : null
  }
  return Number(quMode.value)
}

async function handleConfirm() {
  const wantQualifier = applyQualifier.value && !mixedFamily.value
  if (!wantQualifier && !applySbo.value) {
    await showAlert(t('controlOptions.nothingToApply'))
    return
  }
  // 选了「自定义」却留空会折算成 null(=清除校验),与用户意图相反——拦下。
  if (wantQualifier && !allQl.value && quMode.value === 'custom' && typeof customQu.value !== 'number') {
    await showAlert(t('pointModal.quCustomRequired'))
    return
  }
  isSaving.value = true
  try {
    const request = {
      server_id: props.serverId,
      common_address: props.commonAddress,
      points: props.points.map(p => ({ ioa: p.ioa, asdu_type: p.asdu_type })),
      apply_qualifier: wantQualifier,
      command_qualifier: wantQualifier ? resolveQualifier() : null,
      apply_sbo: applySbo.value,
      select_before_operate: applySbo.value
        ? (seMode.value === 'flexible' ? null : seMode.value === 'sbo')
        : null,
    }
    await invoke('batch_update_control_options', { request })
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
          <span class="modal-title">{{ t('controlOptions.title') }} ({{ points.length }})</span>
          <button class="btn-close" @click="$emit('close')">×</button>
        </div>

        <div class="modal-body">
          <div v-if="mixedFamily" class="form-hint block-hint">{{ t('controlOptions.mixedFamilyHint') }}</div>

          <div v-else class="form-group">
            <label class="apply-toggle">
              <input type="checkbox" v-model="applyQualifier" />
              <span>{{ t('controlOptions.applyQualifier') }}</span>
            </label>
            <template v-if="applyQualifier">
              <div v-if="allQl" class="sub-block">
                <input v-model.number="qlValue" type="number" class="form-input" min="0" max="127" placeholder="0..127" />
                <div class="form-hint">{{ t('controlOptions.qlHint') }}</div>
              </div>
              <div v-else class="radio-group sub-block">
                <label class="radio-option">
                  <input type="radio" v-model="quMode" value="unset" />
                  <span>{{ t('pointModal.quUnset') }}</span>
                </label>
                <label class="radio-option">
                  <input type="radio" v-model="quMode" value="0" />
                  <span>0 · {{ t('pointModal.quNoDef') }}</span>
                </label>
                <label class="radio-option">
                  <input type="radio" v-model="quMode" value="1" />
                  <span>1 · {{ t('pointModal.quShortPulse') }}</span>
                </label>
                <label class="radio-option">
                  <input type="radio" v-model="quMode" value="2" />
                  <span>2 · {{ t('pointModal.quLongPulse') }}</span>
                </label>
                <label class="radio-option">
                  <input type="radio" v-model="quMode" value="3" />
                  <span>3 · {{ t('pointModal.quPersistent') }}</span>
                </label>
                <label class="radio-option">
                  <input type="radio" v-model="quMode" value="custom" />
                  <span>{{ t('pointModal.quCustom') }}</span>
                  <input
                    v-model.number="customQu"
                    type="number"
                    class="form-input radio-inline-input"
                    min="4"
                    max="31"
                    placeholder="4..31"
                    :disabled="quMode !== 'custom'"
                  />
                </label>
              </div>
            </template>
          </div>

          <div class="form-group">
            <label class="apply-toggle">
              <input type="checkbox" v-model="applySbo" />
              <span>{{ t('controlOptions.applySbo') }}</span>
            </label>
            <div v-if="applySbo" class="radio-group sub-block">
              <label class="radio-option">
                <input type="radio" v-model="seMode" value="flexible" />
                <span>{{ t('pointModal.executionModeFlexible') }}</span>
              </label>
              <label class="radio-option">
                <input type="radio" v-model="seMode" value="direct" />
                <span>{{ t('pointModal.executionModeDirect') }}</span>
              </label>
              <label class="radio-option">
                <input type="radio" v-model="seMode" value="sbo" />
                <span>{{ t('pointModal.executionModeSbo') }}</span>
              </label>
            </div>
          </div>
        </div>

        <div class="modal-footer">
          <button class="btn btn-secondary" @click="$emit('close')" :disabled="isSaving">{{ t('common.cancel') }}</button>
          <button class="btn btn-primary" @click="handleConfirm" :disabled="isSaving">
            {{ isSaving ? t('pointModal.saving') : t('controlOptions.apply') }}
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

.form-group {
  margin-bottom: 16px;
}

.apply-toggle {
  display: flex;
  align-items: center;
  gap: 8px;
  color: var(--c-text);
  font-size: 14px;
  cursor: pointer;
  margin-bottom: 8px;
}

.apply-toggle input[type='checkbox'] {
  accent-color: var(--c-blue);
  margin: 0;
}

.sub-block {
  margin-left: 22px;
}

.form-input {
  width: 100%;
  padding: 8px 12px;
  background: var(--c-crust);
  border: 1px solid var(--c-surface1);
  border-radius: 6px;
  color: var(--c-text);
  font-size: 14px;
  box-sizing: border-box;
}

.form-input:focus {
  outline: none;
  border-color: var(--c-blue);
}

.form-input:disabled {
  opacity: 0.55;
  cursor: not-allowed;
}

.form-hint {
  margin-top: 6px;
  color: var(--c-overlay0);
  font-size: 11px;
  line-height: 1.4;
}

.block-hint {
  margin-bottom: 12px;
}

.radio-group {
  display: flex;
  flex-direction: column;
  gap: 6px;
}

.radio-option {
  display: flex;
  align-items: center;
  gap: 8px;
  color: var(--c-text);
  font-size: 13px;
  cursor: pointer;
}

.radio-option input[type='radio'] {
  accent-color: var(--c-blue);
  margin: 0;
  flex-shrink: 0;
}

.radio-inline-input {
  width: 90px;
  padding: 4px 8px;
  font-size: 13px;
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
