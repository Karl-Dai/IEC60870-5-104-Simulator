<script setup lang="ts">
import { ref, onBeforeUnmount } from 'vue'
import { useI18n } from '@shared/i18n'

const { t } = useI18n()

const STATES = [
  { code: '0', key: 'intermediate' },
  { code: '1', key: 'off' },
  { code: '2', key: 'on' },
  { code: '3', key: 'indeterminate' },
] as const

const open = ref(false)
const btn = ref<HTMLElement | null>(null)
// popover 用 fixed 定位(teleport 到 body),从按钮 rect 计算,右对齐避免溢出右缘。
const pos = ref({ top: 0, right: 0 })

function place() {
  const el = btn.value
  if (!el) return
  const r = el.getBoundingClientRect()
  pos.value = {
    top: r.bottom + 4,
    right: Math.max(8, window.innerWidth - r.right),
  }
}

function open_() {
  place()
  open.value = true
  // 推迟到下一帧再挂监听,避免当前这次点击立即触发 outside 关闭。
  requestAnimationFrame(() => {
    document.addEventListener('pointerdown', onDocPointer, true)
    document.addEventListener('keydown', onKey, true)
    window.addEventListener('scroll', close, true)
    window.addEventListener('resize', close, true)
  })
}

function close() {
  if (!open.value) return
  open.value = false
  document.removeEventListener('pointerdown', onDocPointer, true)
  document.removeEventListener('keydown', onKey, true)
  window.removeEventListener('scroll', close, true)
  window.removeEventListener('resize', close, true)
}

function toggle() {
  open.value ? close() : open_()
}

function onDocPointer(e: Event) {
  const target = e.target as HTMLElement
  if (btn.value?.contains(target)) return
  if (target.closest?.('.dp-legend')) return // 点在 popover 内不关
  close()
}
function onKey(e: KeyboardEvent) {
  if (e.key === 'Escape') close()
}

onBeforeUnmount(close)
</script>

<template>
  <button
    ref="btn"
    type="button"
    class="dp-help"
    :aria-label="t('doublePoint.legendTitle')"
    @click.stop="toggle"
  >?</button>
  <Teleport to="body">
    <div
      v-if="open"
      class="dp-legend"
      :style="{ top: pos.top + 'px', right: pos.right + 'px' }"
    >
      <div class="dp-legend-title">{{ t('doublePoint.legendTitle') }}</div>
      <div v-for="s in STATES" :key="s.code" class="dp-legend-row">
        <span class="dp-legend-code">DPI {{ s.code }}</span>
        <span class="dp-legend-token">{{ t(`doublePoint.tokens.${s.key}`) }}</span>
        <span class="dp-legend-desc">{{ t(`doublePoint.states.${s.key}`) }}</span>
      </div>
    </div>
  </Teleport>
</template>

<style scoped>
.dp-help {
  width: 15px;
  height: 15px;
  line-height: 13px;
  border-radius: 50%;
  border: 1px solid var(--c-surface2);
  background: var(--c-surface0);
  color: var(--c-subtext0);
  font-size: 10px;
  cursor: pointer;
  padding: 0;
  margin-left: 5px;
  vertical-align: middle;
}
.dp-help:hover {
  border-color: var(--c-blue);
  color: var(--c-blue);
}
/* teleport 到 body,fixed 定位 → 不受表格 overflow / 虚拟滚动裁剪 */
.dp-legend {
  position: fixed;
  z-index: 1000;
  min-width: 300px;
  padding: 8px 10px;
  background: var(--c-mantle);
  border: 1px solid var(--c-surface1);
  border-radius: 6px;
  box-shadow: 0 4px 16px rgba(0, 0, 0, 0.4);
}
.dp-legend-title {
  font-size: 11px;
  font-weight: 600;
  color: var(--c-subtext1);
  margin-bottom: 6px;
  white-space: nowrap;
}
.dp-legend-row {
  display: grid;
  grid-template-columns: 44px 48px 1fr;
  gap: 6px;
  align-items: baseline;
  font-size: 12px;
  padding: 2px 0;
}
.dp-legend-code {
  font: 600 11px/1 ui-monospace, monospace;
  color: var(--c-peach);
}
.dp-legend-token {
  font: 600 12px/1 ui-monospace, monospace;
  color: var(--c-text);
}
.dp-legend-desc {
  color: var(--c-subtext0);
  font-size: 11px;
}
</style>
