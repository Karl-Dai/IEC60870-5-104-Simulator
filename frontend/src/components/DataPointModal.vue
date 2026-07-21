<script setup lang="ts">
import { ref, watch, inject, computed } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { dialogKey } from '@shared/composables/useDialog'
import type { showAlert as ShowAlert } from '@shared/composables/useDialog'
import { useI18n } from '@shared/i18n'
import { ASDU_TYPE_OPTIONS } from '../constants/asduTypes'
import type { DataPointInfo } from '../types'

const { t } = useI18n()
const { showAlert } = inject<{ showAlert: typeof ShowAlert }>(dialogKey)!

interface Props {
  visible: boolean
  serverId: string
  commonAddress: number
  point?: DataPointInfo | null
  /// 侧栏当前选中的分类(snake_case)。新增时用于收窄类型下拉;
  /// null/未传 = 不过滤(站点级 "All Points" 入口)。
  category?: string | null
}

const props = defineProps<Props>()
const emit = defineEmits<{
  close: []
  added: []
}>()

const ASDU_TYPES = computed(() => {
  // 编辑态类型锁定,不过滤以保证当前值可渲染;新增态按选中分类收窄。
  const opts = isEditing.value || !props.category
    ? ASDU_TYPE_OPTIONS
    : ASDU_TYPE_OPTIONS.filter(o => o.category === props.category)
  return opts.map(o => ({ value: o.value, label: t(o.labelKey), typeId: o.typeId }))
})

const IOA_MAX = 16777215

const formIoa = ref<number | undefined>(undefined)
const formAsduType = ref('MSpNa1')
const formName = ref('')
const formComment = ref('')
const formQualifier = ref<number | undefined>(undefined)
// S/E 执行模式(单选):flexible=兼容旧配置宽松接受 / direct=仅直接执行 / sbo=必须先选择
type SeMode = 'flexible' | 'direct' | 'sbo'
const seMode = ref<SeMode>('flexible')
// QU 限定词(单/双/步命令):unset=不校验(None) / '0'..'3'=标准预设 / custom=自定义数值
type QuMode = 'unset' | '0' | '1' | '2' | '3' | 'custom'
const quMode = ref<QuMode>('unset')
interface MappingTarget { common_address: number; ioa: number; asdu_type: string; name: string }
const mappingTargets = ref<MappingTarget[]>([])
const mappingKey = ref('')
const isSaving = ref(false)
const isEditing = computed(() => Boolean(props.point))
const isControlType = computed(() => formAsduType.value.startsWith('C'))
const isBitstringType = computed(() => formAsduType.value.startsWith('CBo'))
// 设点命令携带 QL(0..127,自由数值);单/双/步命令携带 QU(0..31,有标准预设)。
const isSetpointType = computed(() => formAsduType.value.startsWith('CSe'))

function targetKey(target: Pick<MappingTarget, 'common_address' | 'ioa' | 'asdu_type'>) {
  return `${target.common_address}|${target.ioa}|${target.asdu_type}`
}

// 后端 DataPointInfo.asdu_type 为显示名(如 "C_BO_NA_1"),表单/选项用 PascalCase
// 枚举名(如 "CBoNa1");去掉分隔符后忽略大小写匹配归一化,未匹配时原样返回。
function normalizeAsduType(raw: string): string {
  const key = raw.replace(/[^a-z0-9]/gi, '').toLowerCase()
  return ASDU_TYPE_OPTIONS.find(o => o.value.toLowerCase() === key)?.value ?? raw
}

async function loadMappingTargets() {
  if (!props.visible || !isControlType.value) {
    mappingTargets.value = []
    mappingKey.value = ''
    return
  }
  try {
    mappingTargets.value = await invoke<MappingTarget[]>('list_control_mapping_targets', {
      serverId: props.serverId,
      sourceAsduType: formAsduType.value,
    })
  } catch (e) {
    mappingTargets.value = []
    await showAlert(String(e))
  }
}

watch(() => props.visible, (visible) => {
  if (visible) {
    const point = props.point
    formIoa.value = point?.ioa
    const prevAsduType = formAsduType.value
    formAsduType.value = point
      ? normalizeAsduType(point.asdu_type)
      : (ASDU_TYPES.value[0]?.value ?? 'MSpNa1')
    formName.value = point?.name ?? ''
    formComment.value = point?.comment ?? ''
    formQualifier.value = point?.command_qualifier ?? undefined
    const q = point?.command_qualifier
    quMode.value = q == null ? 'unset' : (q >= 0 && q <= 3 ? String(q) as QuMode : 'custom')
    seMode.value = point?.select_before_operate == null
      ? 'flexible'
      : (point.select_before_operate ? 'sbo' : 'direct')
    mappingKey.value = point?.mapping_common_address != null
      && point.mapping_ioa != null
      && point.mapping_asdu_type
      ? targetKey({
          common_address: point.mapping_common_address,
          ioa: point.mapping_ioa,
          asdu_type: point.mapping_asdu_type,
        })
      : ''
    isSaving.value = false
    // formAsduType 变化时由其 watcher 触发加载,这里只补未变化的情况,避免重复请求
    if (formAsduType.value === prevAsduType) loadMappingTargets()
  }
})

watch(formAsduType, () => {
  if (!isEditing.value) mappingKey.value = ''
  loadMappingTargets()
})

const mapping = computed(() => {
  if (!mappingKey.value) return null
  const [commonAddress, ioa, asduType] = mappingKey.value.split('|')
  return {
    common_address: Number(commonAddress),
    ioa: Number(ioa),
    asdu_type: asduType,
  }
})

// 提交时把 QU/QL 表单状态折算成后端的 command_qualifier(None=不校验)。
function resolveQualifier(): number | null {
  if (!isControlType.value || isBitstringType.value) return null
  if (isSetpointType.value) {
    return typeof formQualifier.value === 'number' ? formQualifier.value : null
  }
  if (quMode.value === 'unset') return null
  if (quMode.value === 'custom') {
    return typeof formQualifier.value === 'number' ? formQualifier.value : null
  }
  return Number(quMode.value)
}

async function handleConfirm() {
  // v-model.number 清空输入框后得到 '' 而非 undefined,非整数(1.5)也会通过
  // 松散比较——须显式要求非负整数,否则后端 serde 报未本地化的原始错误。
  if (
    typeof formIoa.value !== 'number'
    || !Number.isInteger(formIoa.value)
    || formIoa.value < 0
    || formIoa.value > IOA_MAX
  ) {
    await showAlert(t('errors.invalidIoa'))
    return
  }
  if (
    isControlType.value && !isBitstringType.value && !isSetpointType.value
    && quMode.value === 'custom' && typeof formQualifier.value !== 'number'
  ) {
    await showAlert(t('pointModal.quCustomRequired'))
    return
  }
  isSaving.value = true
  try {
    const request = {
        server_id: props.serverId,
        common_address: props.commonAddress,
        // 编辑态用原 IOA 作查找键,表单值经 new_ioa 改址;新增态直接用表单值。
        ioa: isEditing.value ? props.point!.ioa : formIoa.value,
        new_ioa: isEditing.value ? formIoa.value : null,
        asdu_type: formAsduType.value,
        name: formName.value || null,
        comment: formComment.value || null,
        mapping: mapping.value,
        command_qualifier: resolveQualifier(),
        select_before_operate: isControlType.value && !isBitstringType.value
          ? (seMode.value === 'flexible' ? null : seMode.value === 'sbo')
          : null,
    }
    await invoke(isEditing.value ? 'update_data_point_definition' : 'add_data_point', { request })
    emit('added')
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
          <span class="modal-title">{{ isEditing ? t('pointModal.editTitle') : t('pointModal.title') }}</span>
          <button class="btn-close" @click="$emit('close')">×</button>
        </div>

        <div class="modal-body">
          <div class="form-group">
            <label class="form-label">{{ t('pointModal.ioaLabel') }}</label>
            <input
              v-model.number="formIoa"
              type="number"
              class="form-input"
              min="0"
              max="16777215"
              :placeholder="t('pointModal.ioaPlaceholder')"
              @keyup.enter="handleConfirm"
            />
          </div>

          <div class="form-group">
            <label class="form-label">{{ t('pointModal.asduTypeLabel') }}</label>
            <select v-model="formAsduType" class="form-select" :disabled="isEditing">
              <option v-for="opt in ASDU_TYPES" :key="opt.value" :value="opt.value">
                {{ opt.label }} · {{ opt.typeId }}
              </option>
            </select>
          </div>

          <div class="form-group">
            <label class="form-label">{{ t('pointModal.nameLabel') }}</label>
            <input v-model="formName" type="text" class="form-input" :placeholder="t('pointModal.namePlaceholder')" />
          </div>

          <div class="form-group">
            <label class="form-label">{{ t('pointModal.commentLabel') }}</label>
            <input v-model="formComment" type="text" class="form-input" :placeholder="t('pointModal.commentPlaceholder')" />
          </div>

          <template v-if="isControlType && !isBitstringType">
            <!-- 设点命令:QL 自由数值 0..127;单/双/步命令:QU 标准预设 + 自定义 -->
            <div v-if="isSetpointType" class="form-group">
              <label class="form-label">{{ t('pointModal.qualifierLabel') }}</label>
              <input v-model.number="formQualifier" type="number" class="form-input" min="0" max="127" placeholder="0..127" />
              <div class="form-hint">{{ t('pointModal.qualifierHint') }}</div>
            </div>
            <div v-else class="form-group">
              <label class="form-label">{{ t('pointModal.qualifierLabel') }}</label>
              <div class="radio-group">
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
                    v-model.number="formQualifier"
                    type="number"
                    class="form-input radio-inline-input"
                    min="4"
                    max="31"
                    placeholder="4..31"
                    :disabled="quMode !== 'custom'"
                  />
                </label>
              </div>
              <div class="form-hint">{{ t('pointModal.qualifierHint') }}</div>
            </div>
            <div class="form-group">
              <label class="form-label">{{ t('pointModal.executionModeLabel') }}</label>
              <div class="radio-group">
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
          </template>

          <div v-if="isControlType" class="form-group">
            <label class="form-label">{{ t('pointModal.mappingLabel') }}</label>
            <select v-model="mappingKey" class="form-select">
              <option value="">{{ t('pointModal.mappingNone') }}</option>
              <option v-for="target in mappingTargets" :key="targetKey(target)" :value="targetKey(target)">
                CA {{ target.common_address }} · IOA {{ target.ioa }} · {{ target.asdu_type }}{{ target.name ? ` · ${target.name}` : '' }}
              </option>
            </select>
            <div class="form-hint">{{ t('pointModal.mappingHint') }}</div>
          </div>
        </div>

        <div class="modal-footer">
          <button class="btn btn-secondary" @click="$emit('close')" :disabled="isSaving">{{ t('common.cancel') }}</button>
          <button class="btn btn-primary" @click="handleConfirm" :disabled="isSaving">
            {{ isSaving ? t('pointModal.saving') : (isEditing ? t('pointModal.save') : t('pointModal.add')) }}
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

/* 禁用态必须与可编辑态视觉可区分(IOA 编辑曾因缺此样式被误判为 bug) */
.form-input:disabled,
.form-select:disabled {
  opacity: 0.55;
  cursor: not-allowed;
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

.form-hint {
  margin-top: 6px;
  color: var(--c-overlay0);
  font-size: 11px;
  line-height: 1.4;
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
