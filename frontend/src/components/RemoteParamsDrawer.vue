<script setup lang="ts">
import { computed, inject, onBeforeUnmount, ref, watch, type Ref } from 'vue'
import { useI18n } from '@shared/i18n'
import { useRemoteParams } from '../composables/useRemoteParams'
import RemoteParamsForm from './RemoteParamsForm.vue'
import type { ProtocolTimingConfig, RemoteOperationConfig } from '../types'

const props = defineProps<{
  visible: boolean
}>()
const emit = defineEmits<{
  close: []
  saved: []
}>()

const selectedServerId = inject<Ref<string | null>>('selectedServerId') as Ref<string | null>

const { t } = useI18n()

const { timing, ops, loading, lastError, load, applyTiming, applyOps } =
  useRemoteParams(selectedServerId)

const saving = ref(false)
const savedFlash = ref(false)
let flashTimer: ReturnType<typeof setTimeout> | null = null

function snapshot(t: ProtocolTimingConfig, o: RemoteOperationConfig): string {
  return JSON.stringify({ t, o })
}

function cloneTiming(value: ProtocolTimingConfig): ProtocolTimingConfig {
  return { ...value }
}

function cloneOps(value: RemoteOperationConfig): RemoteOperationConfig {
  return JSON.parse(JSON.stringify(value))
}

const baseline = ref<{ serverId: string | null; key: string }>({
  serverId: null,
  key: '',
})
watch(loading, (l) => {
  if (l) {
    baseline.value = { serverId: selectedServerId.value, key: '' }
  } else if (!lastError.value) {
    baseline.value = {
      serverId: selectedServerId.value,
      key: snapshot(timing.value, ops.value),
    }
  }
}, { immediate: true })

watch(selectedServerId, (id) => {
  // 基线不仅属于一份值，也属于一台服务器。切换目标时立刻让旧基线失效；
  // null 分支的 load() 同步恢复默认值且不会翻转 loading，因此在这里落新基线。
  baseline.value = id
    ? { serverId: id, key: '' }
    : { serverId: null, key: snapshot(timing.value, ops.value) }
})

const dirty = computed(() =>
  baseline.value.serverId === selectedServerId.value
  && baseline.value.key !== ''
  && snapshot(timing.value, ops.value) !== baseline.value.key
)

const saveLabel = computed(() =>
  saving.value ? t('remoteParams.saving') : savedFlash.value ? t('remoteParams.saved') : t('remoteParams.saveAll')
)

function clearFlashTimer() {
  if (flashTimer !== null) {
    clearTimeout(flashTimer)
    flashTimer = null
  }
}

onBeforeUnmount(clearFlashTimer)

async function saveAll() {
  if (!selectedServerId.value || saving.value || !dirty.value) return
  const serverId = selectedServerId.value
  const timingToSave = cloneTiming(timing.value)
  const opsToSave = cloneOps(ops.value)
  saving.value = true
  clearFlashTimer()
  savedFlash.value = false
  try {
    if (!(await applyTiming(serverId, timingToSave))) return
    if (!(await applyOps(serverId, opsToSave))) return
    // 保存期间即使外部代码切换了选择，也不能把旧服务器的快照登记成新服务器基线。
    if (selectedServerId.value === serverId) {
      baseline.value = {
        serverId,
        key: snapshot(timingToSave, opsToSave),
      }
    }
    emit('saved')
    savedFlash.value = true
    flashTimer = setTimeout(() => {
      savedFlash.value = false
      flashTimer = null
    }, 1200)
  } finally {
    saving.value = false
  }
}

async function discardChanges() {
  if (saving.value) return
  await load()
}

function close() {
  if (saving.value) return
  emit('close')
}

function handleBackdrop(e: MouseEvent) {
  if ((e.target as HTMLElement).classList.contains('rp-drawer-backdrop')) {
    close()
  }
}

function handleEsc(e: KeyboardEvent) {
  if (e.key === 'Escape' && props.visible) close()
}

watch(() => props.visible, (v) => {
  if (v) {
    // 打开时从后端重载:composable 只在 selectedServerId 变化时 load,
    // 期间若经弹窗(RemoteParamsModal)改过参数,这里的快照已经过期(issue #28)。
    // load() 翻转 loading,上面的 watch(loading) 会顺带重置 dirty 基线。
    // 但【有未保存编辑时不重载】—— 关掉抽屉保留草稿是原有设计(dirty 时才出现的
    // Discard 按钮就是为此),无条件 load 等于 Esc/点背景关掉就静默丢弃用户编辑。
    if (!dirty.value) load()
    window.addEventListener('keydown', handleEsc)
  } else {
    window.removeEventListener('keydown', handleEsc)
  }
})
</script>

<template>
  <Teleport to="body">
    <Transition name="rp-drawer">
      <div
        v-if="visible"
        class="rp-drawer-backdrop"
        @mousedown="handleBackdrop"
      >
        <aside
          class="rp-drawer"
          role="dialog"
          :aria-label="t('remoteParams.drawerTitle')"
          @mousedown.stop
        >
          <header class="rp-drawer-head">
            <div class="rp-drawer-title">
              <span class="rp-drawer-eyebrow">REMOTE OPS</span>
              <h3>{{ t('remoteParams.drawerTitle') }}</h3>
            </div>
            <div class="rp-drawer-actions">
              <button
                v-if="dirty"
                class="rp-btn rp-btn-ghost"
                :disabled="saving"
                @click="discardChanges"
                :title="t('remoteParams.discardTitle')"
              >{{ t('remoteParams.discard') }}</button>
              <button
                class="rp-btn rp-btn-primary"
                :class="{ 'is-dirty': dirty, 'is-flash': savedFlash }"
                :disabled="saving || !dirty || !selectedServerId"
                @click="saveAll"
              >
                <span class="rp-btn-dot" v-if="dirty" />
                {{ saveLabel }}
              </button>
              <button
                class="rp-btn-close"
                :disabled="saving"
                @click="close"
                :title="t('remoteParams.closeEsc')"
                :aria-label="t('common.close')"
              >×</button>
            </div>
          </header>

          <div class="rp-drawer-body">
            <div v-if="!selectedServerId" class="rp-empty">
              <span class="rp-empty-mark">·</span>
              <span>{{ t('remoteParams.selectServerFirst') }}</span>
            </div>

            <template v-else>
              <RemoteParamsForm :timing="timing" :ops="ops" />

              <p v-if="lastError" class="rp-error">{{ lastError }}</p>
              <p v-if="loading" class="rp-muted">{{ t('remoteParams.loadingText') }}</p>
              <p class="rp-foot-note">{{ t('remoteParams.footNote') }}</p>
            </template>
          </div>
        </aside>
      </div>
    </Transition>
  </Teleport>
</template>

<style scoped>
.rp-drawer-backdrop {
  position: fixed;
  inset: 0;
  z-index: 1500;
  background: color-mix(in srgb, var(--c-crust) 60%, transparent);
  backdrop-filter: blur(4px);
  -webkit-backdrop-filter: blur(4px);
  display: flex;
  justify-content: flex-end;
}

.rp-drawer {
  width: 420px;
  max-width: 92vw;
  height: 100vh;
  background: var(--c-mantle);
  border-left: 1px solid var(--c-surface0);
  box-shadow: -16px 0 32px -8px rgba(0, 0, 0, 0.45);
  display: flex;
  flex-direction: column;
  font-size: 12px;
  color: var(--c-text);
}

.rp-drawer-head {
  position: sticky;
  top: 0;
  z-index: 2;
  display: flex;
  align-items: flex-end;
  gap: 8px;
  padding: 12px 14px 12px;
  background: linear-gradient(180deg, var(--c-mantle) 0%, var(--c-mantle) 70%, color-mix(in srgb, var(--c-mantle) 80%, transparent) 100%);
  border-bottom: 1px solid var(--c-surface0);
}

.rp-drawer-title { flex: 1; min-width: 0; }

.rp-drawer-eyebrow {
  display: block;
  font: 600 9.5px/1 ui-monospace, "SF Mono", Menlo, monospace;
  letter-spacing: 0.16em;
  color: var(--c-overlay0);
  margin-bottom: 4px;
}

.rp-drawer-head h3 {
  margin: 0;
  font-size: 14px;
  font-weight: 600;
  color: var(--c-text);
  letter-spacing: 0.01em;
}

.rp-drawer-actions {
  display: flex;
  align-items: center;
  gap: 6px;
}

.rp-btn {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  padding: 5px 10px;
  font: 600 11px/1.2 -apple-system, BlinkMacSystemFont, sans-serif;
  letter-spacing: 0.02em;
  border-radius: 4px;
  border: 1px solid transparent;
  cursor: pointer;
  transition: background 100ms, border-color 100ms, color 100ms, transform 60ms;
}

.rp-btn:disabled {
  opacity: 0.4;
  cursor: not-allowed;
}

.rp-btn:active:not(:disabled) { transform: translateY(0.5px); }

.rp-btn-ghost {
  background: transparent;
  color: var(--c-subtext0);
  border-color: var(--c-surface1);
}

.rp-btn-ghost:hover:not(:disabled) {
  color: var(--c-text);
  border-color: var(--c-surface2);
}

.rp-btn-primary {
  background: var(--c-surface0);
  color: var(--c-subtext0);
  border-color: var(--c-surface1);
}

.rp-btn-primary.is-dirty {
  background: var(--c-blue);
  color: var(--c-base);
  border-color: var(--c-blue);
  box-shadow: 0 0 0 2px color-mix(in srgb, var(--c-blue) 25%, transparent);
}

.rp-btn-primary.is-flash {
  background: var(--c-green);
  color: var(--c-base);
  border-color: var(--c-green);
}

.rp-btn-dot {
  width: 6px;
  height: 6px;
  border-radius: 50%;
  background: currentColor;
  box-shadow: 0 0 6px currentColor;
}

.rp-btn-close {
  width: 26px;
  height: 26px;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  background: transparent;
  border: none;
  border-radius: 4px;
  color: var(--c-overlay0);
  font-size: 20px;
  line-height: 1;
  cursor: pointer;
  margin-left: 2px;
  transition: background 100ms, color 100ms;
}

.rp-btn-close:hover:not(:disabled) {
  background: var(--c-surface0);
  color: var(--c-text);
}

.rp-btn-close:disabled {
  opacity: 0.4;
  cursor: not-allowed;
}

.rp-drawer-body {
  flex: 1;
  overflow-y: auto;
  padding: 0 14px 16px;
}

.rp-empty {
  margin: 24px 0;
  padding: 18px 14px;
  border: 1px dashed var(--c-surface0);
  border-radius: 6px;
  color: var(--c-subtext0);
  font-size: 12px;
  display: flex;
  align-items: center;
  gap: 10px;
}

.rp-empty-mark {
  font: 600 18px/1 ui-monospace, "SF Mono", Menlo, monospace;
  color: var(--c-overlay0);
}

.rp-muted { color: var(--c-subtext0); font-size: 11px; padding: 4px 0 8px; }

.rp-error {
  margin-top: 8px;
  padding: 6px 8px;
  font-size: 11.5px;
  color: var(--c-red);
  background: color-mix(in srgb, var(--c-red) 12%, transparent);
  border-left: 2px solid var(--c-red);
  border-radius: 3px;
}

.rp-foot-note {
  margin: 12px 0 0;
  padding: 6px 8px;
  font-size: 10.5px;
  color: var(--c-overlay0);
  border-left: 2px solid var(--c-surface1);
  background: var(--c-base);
  border-radius: 0 3px 3px 0;
}

/* —— 进出场动画 —— */
.rp-drawer-enter-active,
.rp-drawer-leave-active {
  transition: background-color 220ms ease, backdrop-filter 220ms ease;
}
.rp-drawer-enter-active .rp-drawer,
.rp-drawer-leave-active .rp-drawer {
  transition: transform 280ms cubic-bezier(0.32, 0.72, 0, 1),
              opacity 200ms ease;
}

.rp-drawer-enter-from,
.rp-drawer-leave-to {
  background: color-mix(in srgb, var(--c-crust) 0%, transparent);
  backdrop-filter: blur(0);
  -webkit-backdrop-filter: blur(0);
}
.rp-drawer-enter-from .rp-drawer,
.rp-drawer-leave-to .rp-drawer {
  transform: translateX(100%);
  opacity: 0.6;
}
</style>
