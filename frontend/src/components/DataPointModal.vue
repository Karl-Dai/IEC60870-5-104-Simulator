<script setup lang="ts">
import { ref, watch, inject, computed } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { dialogKey } from '@shared/composables/useDialog'
import type { showAlert as ShowAlert } from '@shared/composables/useDialog'
import { useI18n } from '@shared/i18n'
import { ASDU_TYPE_OPTIONS, findAsduTypeOption, formatAsduTypeWithId } from '../constants/asduTypes'
import { IOA_MAX } from './batchAdd/ioaRanges'
import type { DataPointInfo } from '../types'

const { t } = useI18n()
const { showAlert } = inject<{ showAlert: typeof ShowAlert }>(dialogKey)!

interface Props {
  visible: boolean
  serverId: string
  commonAddress: number
  point?: DataPointInfo | null
  /** 当前左树选中的分类稳定键;提供时新增点位的类型下拉只显示该分类的类型。 */
  category?: string | null
  /** 本站(当前 CA)已有点位,用于同 CASDU 跨类型重复 IOA 警告(issue #28)。 */
  existingPoints?: ReadonlyArray<Pick<DataPointInfo, 'ioa' | 'asdu_type' | 'category'>>
}

const props = defineProps<Props>()
const emit = defineEmits<{
  close: []
  added: []
}>()

// 编辑模式类型锁定,列出全部以便回显;新增且有分类上下文时只列同分类类型
// (issue #28:C_SC 分类下只出现 45/58,不再是全部 37 种)。
const ASDU_TYPES = computed(() => {
  const source = !isEditing.value && props.category
    ? ASDU_TYPE_OPTIONS.filter(o => o.category === props.category)
    : ASDU_TYPE_OPTIONS
  return source.map(o => ({ value: o.value, label: t(o.labelKey), typeId: o.typeId }))
})

const formIoa = ref<number | undefined>(undefined)
const formAsduType = ref('MSpNa1')
const formName = ref('')
const formComment = ref('')
// QU/QL 限定词按钮组:any=不限制;0..3 为标准 QU 预设;custom 走数字输入。
const formQualifierChoice = ref<'any' | '0' | '1' | '2' | '3' | 'custom'>('any')
const formQualifierCustom = ref<number | undefined>(undefined)
const formSbo = ref<boolean | undefined>(undefined)
interface MappingTarget { common_address: number; ioa: number; asdu_type: string; name: string }
const mappingTargets = ref<MappingTarget[]>([])
const mappingKey = ref('')
const isSaving = ref(false)
const isEditing = computed(() => Boolean(props.point))

// 同 CASDU 跨类型重复 IOA 警告(issue #28,不阻断保存)。
// 同方向且不同分类的已有点共用该 IOA 才提示;NA/TA/TB 同分类变体与
// 控制↔监视配对(兼容自动映射)不算。编辑时自身同分类,天然被排除。
const dupIoaTypes = computed<string | null>(() => {
  const ioa = formIoa.value
  if (ioa === undefined || ioa === null) return null
  const opt = findAsduTypeOption(formAsduType.value)
  if (!opt) return null
  const dir = formAsduType.value.startsWith('C') ? 'c' : 'm'
  const clashes = (props.existingPoints ?? [])
    .filter(p =>
      p.ioa === ioa
      && (p.asdu_type.startsWith('C_') ? 'c' : 'm') === dir
      && p.category !== opt.category)
    .map(p => formatAsduTypeWithId(p.asdu_type))
  return clashes.length ? clashes.join(', ') : null
})
const isControlType = computed(() => formAsduType.value.startsWith('C'))
const isBitstringType = computed(() => formAsduType.value.startsWith('CBo'))
const isSetpointType = computed(() => formAsduType.value.startsWith('CSe'))
const qualifierMax = computed(() => isSetpointType.value ? 127 : 31)
// 设定值 (QL) 没有短脉冲/长脉冲语义,预设只保留 0;命令 (QU) 提供 0..3。
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
  switch (formQualifierChoice.value) {
    case 'any': return null
    case 'custom':
      return typeof formQualifierCustom.value === 'number' ? formQualifierCustom.value : null
    default: return Number(formQualifierChoice.value)
  }
})

function qualifierChoiceFor(q: number | null | undefined): typeof formQualifierChoice.value {
  if (q == null) return 'any'
  const presets = isSetpointType.value ? [0] : [0, 1, 2, 3]
  return presets.includes(q) ? (String(q) as '0' | '1' | '2' | '3') : 'custom'
}

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
    formQualifierChoice.value = qualifierChoiceFor(point?.command_qualifier)
    formQualifierCustom.value = formQualifierChoice.value === 'custom'
      ? point?.command_qualifier ?? undefined
      : undefined
    formSbo.value = point?.select_before_operate ?? undefined
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

async function handleConfirm() {
  // v-model.number 清空输入框时给出 ''(空串)而非 undefined,小数也能通过
  // number 输入;这两类值穿透后会落到后端 serde 的原始报错,须在表单层拦截。
  if (
    typeof formIoa.value !== 'number' ||
    !Number.isInteger(formIoa.value) ||
    formIoa.value < 0 ||
    formIoa.value > IOA_MAX
  ) {
    await showAlert(t('errors.invalidIoa'))
    return
  }
  const isCommandOptions = isControlType.value && !isBitstringType.value
  if (isCommandOptions && formQualifierChoice.value === 'custom') {
    const q = formQualifierCustom.value
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
        // 编辑模式 ioa 是定位键(原地址),改址走 new_ioa(issue #28:IOA 可编辑)。
        ioa: isEditing.value ? props.point!.ioa : formIoa.value,
        asdu_type: formAsduType.value,
        name: formName.value || null,
        comment: formComment.value || null,
        mapping: mapping.value,
        command_qualifier: isCommandOptions ? qualifierValue.value : null,
        select_before_operate: isCommandOptions ? formSbo.value ?? null : null,
    }
    if (isEditing.value) {
      request.new_ioa = formIoa.value
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
              :max="IOA_MAX"
              :placeholder="t('pointModal.ioaPlaceholder')"
              @keyup.enter="handleConfirm"
            />
            <div v-if="isEditing" class="form-hint">{{ t('pointModal.ioaEditHint') }}</div>
            <div v-if="dupIoaTypes" class="form-hint form-hint--warn">
              ⚠ {{ t('pointModal.dupIoaWarn', { types: dupIoaTypes }) }}
            </div>
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
            <div class="form-group">
              <label class="form-label">{{ t('pointModal.qualifierLabel') }}</label>
              <div class="radio-group">
                <label class="radio-item">
                  <input v-model="formQualifierChoice" type="radio" value="any" />
                  <span>{{ t('pointModal.quAny') }}</span>
                </label>
                <label v-for="preset in qualifierPresets" :key="preset.key" class="radio-item">
                  <input v-model="formQualifierChoice" type="radio" :value="preset.key" />
                  <span>{{ preset.label }}</span>
                </label>
                <label class="radio-item">
                  <input v-model="formQualifierChoice" type="radio" value="custom" />
                  <span>{{ t('pointModal.quCustom') }}</span>
                  <input
                    v-if="formQualifierChoice === 'custom'"
                    v-model.number="formQualifierCustom"
                    type="number"
                    class="form-input radio-custom-input"
                    min="0"
                    :max="qualifierMax"
                    :placeholder="`0..${qualifierMax}`"
                  />
                </label>
              </div>
              <div class="form-hint">{{ t('pointModal.qualifierHint') }}</div>
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

.form-hint {
  margin-top: 6px;
  color: var(--c-overlay0);
  font-size: 11px;
  line-height: 1.4;
}

/* 同 CASDU 跨类型重复 IOA 警告(不阻断保存) */
.form-hint--warn {
  color: var(--c-peach);
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
