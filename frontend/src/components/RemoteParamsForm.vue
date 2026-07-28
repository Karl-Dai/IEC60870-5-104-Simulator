<script setup lang="ts">
import { ref } from 'vue'
import {
  correctTimingEdit,
  formatCorrections,
  isTimingField,
  type TimingCorrection,
} from '@shared/timing'
import { useI18n } from '@shared/i18n'
import type {
  ProtocolTimingConfig,
  RemoteOperationConfig,
  CommandAckCot,
  UploadMode,
} from '../types'

const props = defineProps<{
  timing: ProtocolTimingConfig
  ops: RemoteOperationConfig
}>()

const { t } = useI18n()

// 编辑感知 C3 自动纠正:在 change(失焦)时触发,t1/k 为锚,至多动一个邻居。
// 后端会再做一次权威规范化,正常情况下对前端已纠正的值为空操作。
const recentCorrections = ref<TimingCorrection[]>([])
let correctionClearTimer: ReturnType<typeof setTimeout> | undefined
function onTimingChange(key: string) {
  if (!isTimingField(key)) return
  const changes = correctTimingEdit(props.timing, key)
  if (changes.length === 0) return
  recentCorrections.value = changes
  if (correctionClearTimer) clearTimeout(correctionClearTimer)
  correctionClearTimer = setTimeout(() => { recentCorrections.value = [] }, 6000)
}

const cotOptions: { value: CommandAckCot; label: string }[] = [
  { value: 'activation_con', label: 'ACT_CON · 7' },
  { value: 'activation_termination', label: 'ACT_TERM · 10' },
  { value: 'deactivation_con', label: 'DEACT_CON · 9' },
]
const modeOptions: { value: UploadMode; labelKey: string }[] = [
  { value: 'continuous', labelKey: 'remoteParams.modeContinuous' },
  { value: 'discrete', labelKey: 'remoteParams.modeDiscrete' },
]
// 按分类的「变位同步上送 TB」开关(IT 不在内——靠召唤而非变位)。
type SyncTbKey = keyof RemoteOperationConfig['sync_tb_by_category']
const syncTbCategories: { key: SyncTbKey; map: string }[] = [
  { key: 'sp', map: 'M_SP_NA_1 → M_SP_TB_1' },
  { key: 'dp', map: 'M_DP_NA_1 → M_DP_TB_1' },
  { key: 'st', map: 'M_ST_NA_1 → M_ST_TB_1' },
  { key: 'bo', map: 'M_BO_NA_1 → M_BO_TB_1' },
  { key: 'me_na', map: 'M_ME_NA_1 → M_ME_TD_1' },
  { key: 'me_nb', map: 'M_ME_NB_1 → M_ME_TE_1' },
  { key: 'me_nc', map: 'M_ME_NC_1 → M_ME_TF_1' },
]
const timingMeta: { key: 't0' | 't1' | 't2' | 't3' | 'k' | 'w'; hintKey: string; unit?: string; min: number; max: number }[] = [
  { key: 't0', hintKey: 'remoteParams.hintT0', unit: 's', min: 1, max: 255 },
  { key: 't1', hintKey: 'remoteParams.hintT1', unit: 's', min: 1, max: 255 },
  { key: 't2', hintKey: 'remoteParams.hintT2', unit: 's', min: 1, max: 255 },
  { key: 't3', hintKey: 'remoteParams.hintT3', unit: 's', min: 1, max: 255 },
  { key: 'k', hintKey: 'remoteParams.hintK', min: 1, max: 32767 },
  { key: 'w', hintKey: 'remoteParams.hintW', min: 1, max: 32767 },
]
</script>

<template>
  <!-- 链路参数 -->
  <section class="rp-sec">
    <header class="rp-sec-head">
      <h4>{{ t('remoteParams.linkParams') }}</h4>
      <span class="rp-sec-sub">{{ t('remoteParams.linkParamsSub') }}</span>
    </header>
    <div class="rp-rows">
      <label v-for="m in timingMeta" :key="m.key" class="rp-row rp-row-timing">
        <span class="rp-row-key">{{ m.key }}</span>
        <input
          type="number"
          :min="m.min"
          :max="m.max"
          v-model.number="timing[m.key]"
          @change="onTimingChange(m.key)"
        />
        <span class="rp-row-unit">{{ m.unit ?? '' }}</span>
        <span class="rp-row-hint">{{ t(m.hintKey) }}</span>
      </label>
    </div>
    <div v-if="recentCorrections.length" class="rp-corrected">
      {{ t('remoteParams.autoCorrected') }} {{ formatCorrections(recentCorrections) }}
    </div>
    <slot name="actions-timing" />
  </section>

  <!-- 召唤与应答 -->
  <section class="rp-sec">
    <header class="rp-sec-head">
      <h4>{{ t('remoteParams.interrogation') }}</h4>
      <span class="rp-sec-sub">{{ t('remoteParams.interrogationSub') }}</span>
    </header>

    <div class="rp-group">
      <span class="rp-group-label">{{ t('remoteParams.answerSwitches') }}</span>
      <label class="rp-switch">
        <input type="checkbox" v-model="ops.answer_general_interrogation" />
        <span class="rp-switch-text">{{ t('remoteParams.gi') }}</span>
        <code class="rp-tag">C_IC_NA_1</code>
      </label>
      <label class="rp-switch">
        <input type="checkbox" v-model="ops.answer_counter_interrogation" />
        <span class="rp-switch-text">{{ t('remoteParams.counterInterrogation') }}</span>
        <code class="rp-tag">C_CI_NA_1</code>
      </label>
      <label class="rp-switch">
        <input type="checkbox" v-model="ops.answer_clock_sync" />
        <span class="rp-switch-text">{{ t('remoteParams.clockSync') }}</span>
        <code class="rp-tag">C_CS_NA_1</code>
      </label>
      <div class="rp-switch-note">{{ t('remoteParams.clockSyncHint') }}</div>
      <label class="rp-switch">
        <input type="checkbox" v-model="ops.answer_commands" />
        <span class="rp-switch-text">{{ t('remoteParams.commands') }}</span>
      </label>
      <div class="rp-command-map">
        <span>{{ t('remoteParams.controlMappingHint') }}</span>
      </div>
      <label class="rp-switch">
        <input type="checkbox" v-model="ops.auto_map_commands" />
        <span class="rp-switch-text">{{ t('remoteParams.autoMapCommands') }}</span>
      </label>
      <label class="rp-switch">
        <input type="checkbox" v-model="ops.ack_unmapped_commands" />
        <span class="rp-switch-text">{{ t('remoteParams.ackUnmappedCommands') }}</span>
      </label>
      <label class="rp-switch">
        <input type="checkbox" v-model="ops.sbo_enforce" />
        <span class="rp-switch-text">{{ t('remoteParams.sboEnforce') }}</span>
      </label>
      <div class="rp-field">
        <label>{{ t('remoteParams.sboTimeout') }}</label>
        <div class="rp-inline">
          <input type="number" min="100" max="3600000" v-model.number="ops.sbo_timeout_ms" />
          <span class="rp-unit">ms</span>
        </div>
      </div>
      <label class="rp-switch">
        <input type="checkbox" v-model="ops.gi_include_timestamped" />
        <span class="rp-switch-text">{{ t('remoteParams.giWithTimestamp') }}</span>
      </label>
    </div>

    <div class="rp-group">
      <span class="rp-group-label">{{ t('remoteParams.cmdAckCot') }}</span>
      <div class="rp-field">
        <label>{{ t('remoteParams.select') }}</label>
        <select v-model="ops.select_ack_cot">
          <option v-for="o in cotOptions" :key="o.value" :value="o.value">{{ o.label }}</option>
        </select>
      </div>
      <div class="rp-field">
        <label>{{ t('remoteParams.execute') }}</label>
        <!-- Execute COT 只决定终止帧的 COT:ACT_TERM 关闭时后端根本不发终止帧,
             此处必须禁用,否则用户改成 10 却抓不到终止帧(issue #28)。 -->
        <select v-model="ops.execute_ack_cot" :disabled="!ops.send_act_term">
          <option v-for="o in cotOptions" :key="o.value" :value="o.value">{{ o.label }}</option>
        </select>
      </div>
      <div v-if="!ops.send_act_term" class="rp-note rp-note-warn">
        {{ t('remoteParams.executeCotDisabledHint') }}
      </div>
      <div class="rp-field">
        <label>{{ t('remoteParams.cancel') }}</label>
        <select v-model="ops.cancel_ack_cot">
          <option v-for="o in cotOptions" :key="o.value" :value="o.value">{{ o.label }}</option>
        </select>
      </div>
      <label class="rp-switch">
        <input type="checkbox" v-model="ops.send_act_term" />
        <span class="rp-switch-text">{{ t('remoteParams.sendActTerm') }}</span>
      </label>
      <div class="rp-switch-note">{{ t('remoteParams.sendActTermHint') }}</div>
      <div class="rp-note">{{ t('remoteParams.appLayerNote') }}</div>
    </div>

    <slot name="actions-ops" />
  </section>

  <!-- 数据上送方式 -->
  <section class="rp-sec">
    <header class="rp-sec-head">
      <h4>{{ t('remoteParams.uploadMode') }}</h4>
      <span class="rp-sec-sub">{{ t('remoteParams.uploadModeSub') }}</span>
    </header>

    <div class="rp-group">
      <span class="rp-group-label">{{ t('remoteParams.sqMode') }}</span>
      <div class="rp-field">
        <label>{{ t('remoteParams.untimestamped') }}</label>
        <select v-model="ops.upload_mode_untimestamped">
          <option v-for="o in modeOptions" :key="o.value" :value="o.value">{{ t(o.labelKey) }}</option>
        </select>
      </div>
      <div class="rp-field">
        <label>{{ t('remoteParams.timestamped') }}</label>
        <select v-model="ops.upload_mode_timestamped">
          <option v-for="o in modeOptions" :key="o.value" :value="o.value">{{ t(o.labelKey) }}</option>
        </select>
      </div>
    </div>

    <div class="rp-group">
      <span class="rp-group-label">{{ t('remoteParams.packingStrategy') }}</span>
      <label class="rp-switch">
        <input type="checkbox" v-model="ops.auto_packing" />
        <span class="rp-switch-text">{{ t('remoteParams.autoPacking') }}</span>
      </label>
      <div class="rp-subgroup">
        <span class="rp-subgroup-label">{{ t('remoteParams.syncTb') }}</span>
        <label v-for="c in syncTbCategories" :key="c.key" class="rp-switch">
          <input type="checkbox" v-model="ops.sync_tb_by_category[c.key]" />
          <code class="rp-tag">{{ c.map }}</code>
        </label>
        <div class="rp-note">{{ t('remoteParams.syncTbNote') }}</div>
      </div>
    </div>
  </section>

  <!-- 变位仿真 -->
  <section class="rp-sec">
    <header class="rp-sec-head">
      <h4>{{ t('remoteParams.mutationSim') }}</h4>
      <span class="rp-sec-sub">{{ t('remoteParams.randomPacing') }}</span>
    </header>

    <div class="rp-group">
      <span class="rp-group-label">{{ t('remoteParams.randomPacing') }}</span>
      <div class="rp-pacing">
        <div class="rp-field">
          <label>{{ t('remoteParams.perSend') }}</label>
          <div class="rp-inline">
            <input type="number" min="1" max="100000" v-model.number="ops.random_pacing.batch_size" />
            <span class="rp-unit">{{ t('remoteParams.unitCount') }}</span>
          </div>
        </div>
        <div class="rp-field">
          <label>{{ t('remoteParams.delay') }}</label>
          <div class="rp-inline">
            <input type="number" min="0" max="60000" v-model.number="ops.random_pacing.delay_ms" />
            <span class="rp-unit">ms</span>
          </div>
        </div>
      </div>
    </div>

  </section>

</template>

<style scoped>
/* —— Section（顶层分区，无卡片边框）—— */
.rp-sec {
  padding: 14px 0 6px;
}

.rp-sec + .rp-sec {
  border-top: 1px solid var(--c-surface0);
}

.rp-sec-head {
  display: flex;
  align-items: baseline;
  gap: 8px;
  margin-bottom: 10px;
}

.rp-sec-head h4 {
  margin: 0;
  font-size: 12.5px;
  font-weight: 600;
  color: var(--c-text);
  letter-spacing: 0.02em;
}

.rp-sec-sub {
  font-size: 11px;
  color: var(--c-overlay0);
  letter-spacing: 0.02em;
}

.rp-sec-sub::before {
  content: "·";
  margin-right: 6px;
  color: var(--c-surface2);
}

/* —— 组（section 内的子分组）—— */
.rp-group + .rp-group {
  margin-top: 14px;
}

.rp-group-label {
  display: block;
  font-size: 11px;
  font-weight: 500;
  color: var(--c-subtext0);
  margin-bottom: 8px;
  letter-spacing: 0.02em;
}

.rp-subgroup {
  display: flex;
  flex-direction: column;
  gap: 4px;
  margin-top: 6px;
}
.rp-subgroup-label {
  font-size: 11px;
  font-weight: 500;
  color: var(--c-subtext0);
  margin-bottom: 2px;
  letter-spacing: 0.02em;
}

.rp-note {
  margin-top: 4px;
  font-size: 11px;
  color: var(--c-overlay0);
  line-height: 1.5;
}

/* 开关下方的行为说明：与开关文字左对齐（复选框 14px + gap 8px）。 */
.rp-switch-note {
  margin: 0 0 6px 22px;
  font-size: 11px;
  color: var(--c-overlay0);
  line-height: 1.5;
}

/* 联动禁用提示：比普通说明更醒目，说明控件为何变灰。 */
.rp-note-warn {
  color: var(--c-yellow, var(--c-subtext1, var(--c-subtext0)));
}

/* —— 链路参数：单列表行 —— */
.rp-rows {
  display: flex;
  flex-direction: column;
}

.rp-row-timing {
  display: grid;
  grid-template-columns: 24px 84px 14px 1fr;
  align-items: center;
  gap: 0 10px;
  padding: 4px 0;
  font-size: 12px;
  cursor: text;
}

.rp-row-timing + .rp-row-timing {
  border-top: 1px solid color-mix(in srgb, var(--c-surface0) 50%, transparent);
}

.rp-row-key {
  font: 500 12px/1 ui-monospace, "SF Mono", Menlo, monospace;
  color: var(--c-subtext1, var(--c-subtext0));
  letter-spacing: 0.04em;
}

.rp-row-timing input {
  width: 100%;
  height: 24px;
  padding: 0 8px;
  background: var(--c-base);
  border: 1px solid var(--c-surface0);
  border-radius: 4px;
  color: var(--c-text);
  font: 500 12px/1 ui-monospace, "SF Mono", Menlo, monospace;
  text-align: right;
  transition: border-color 80ms linear, background 80ms linear;
}

.rp-row-timing input:hover {
  border-color: var(--c-surface1);
}

.rp-row-timing input:focus {
  outline: none;
  border-color: var(--c-blue);
  background: color-mix(in srgb, var(--c-blue) 6%, var(--c-base));
}

.rp-row-unit {
  font: 500 11px/1 ui-monospace, "SF Mono", Menlo, monospace;
  color: var(--c-overlay0);
  min-width: 14px;
}

.rp-row-hint {
  font-size: 11.5px;
  color: var(--c-subtext0);
  letter-spacing: 0.01em;
}
.rp-corrected {
  margin-top: 8px;
  padding: 6px 8px;
  font-size: 11.5px;
  color: var(--c-yellow, var(--c-subtext1, var(--c-subtext0)));
  background: color-mix(in srgb, var(--c-yellow, var(--c-surface1)) 14%, transparent);
  border: 1px solid color-mix(in srgb, var(--c-yellow, var(--c-surface1)) 35%, transparent);
  border-radius: 4px;
}

/* —— Switch 行 —— */
.rp-switch {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 5px 0;
  font-size: 12px;
  color: var(--c-text);
  cursor: pointer;
  user-select: none;
}

.rp-switch input {
  flex: none;
  width: 14px;
  height: 14px;
  margin: 0;
  accent-color: var(--c-blue);
  cursor: pointer;
}

.rp-switch-text {
  color: var(--c-text);
}

.rp-tag {
  font: 500 10.5px/1 ui-monospace, "SF Mono", Menlo, monospace;
  color: var(--c-subtext0);
  background: transparent;
  padding: 2px 6px;
  border: 1px solid var(--c-surface0);
  border-radius: 3px;
  letter-spacing: 0.02em;
}

.rp-command-map {
  display: flex;
  flex-wrap: wrap;
  gap: 5px;
  margin: 0 0 6px 22px;
  color: var(--c-overlay0);
  font-size: 10.5px;
}

.rp-command-map > span {
  flex-basis: 100%;
}

/* —— Field 行（label 左 + control 右）—— */
.rp-field {
  display: grid;
  grid-template-columns: 64px 1fr;
  align-items: center;
  gap: 10px;
  margin-bottom: 6px;
}

.rp-field label {
  font-size: 11.5px;
  color: var(--c-subtext0);
  letter-spacing: 0.02em;
}

.rp-field select,
.rp-field input[type="number"] {
  width: 100%;
  height: 26px;
  padding: 0 8px;
  background: var(--c-base);
  border: 1px solid var(--c-surface0);
  border-radius: 4px;
  color: var(--c-text);
  font: 500 12px/1 ui-monospace, "SF Mono", Menlo, monospace;
  transition: border-color 80ms linear;
}

.rp-field select:hover,
.rp-field input[type="number"]:hover {
  border-color: var(--c-surface1);
}

.rp-field select:disabled {
  opacity: 0.45;
  cursor: not-allowed;
  background: var(--c-surface0);
}

.rp-field select:disabled:hover {
  border-color: var(--c-surface0);
}

.rp-field select:focus,
.rp-field input[type="number"]:focus {
  outline: none;
  border-color: var(--c-blue);
  background: color-mix(in srgb, var(--c-blue) 6%, var(--c-base));
}

/* —— 变位仿真：节流 / 固定变位 —— */
.rp-inline {
  display: flex;
  align-items: center;
  gap: 6px;
}

.rp-inline input { flex: 1; }

.rp-unit {
  font: 500 11px/1 ui-monospace, "SF Mono", Menlo, monospace;
  color: var(--c-overlay0);
}

.rp-pacing {
  display: grid;
  gap: 6px;
}
</style>
