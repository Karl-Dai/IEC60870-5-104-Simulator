<script setup lang="ts">
import { ref, watch, nextTick } from 'vue'
import { useDialogState } from '../composables/useDialog'
import { useI18n } from '../i18n'

const { state, dialogConfirm, dialogCancel } = useDialogState()
const { t } = useI18n()
const inputRef = ref<HTMLInputElement | null>(null)
const inputValue = ref('')

watch(() => state.value.visible, async (visible) => {
  if (visible && state.value.mode === 'prompt') {
    inputValue.value = state.value.defaultValue
    await nextTick()
    inputRef.value?.focus()
    inputRef.value?.select()
  }
})

function handleConfirm() {
  if (state.value.mode === 'prompt') {
    dialogConfirm(inputValue.value)
  } else {
    dialogConfirm()
  }
}

function handleKeydown(e: KeyboardEvent) {
  if (e.key === 'Enter') {
    handleConfirm()
  } else if (e.key === 'Escape') {
    dialogCancel()
  }
}
</script>

<template>
  <Teleport to="body">
    <Transition name="dialog-pop">
    <div v-if="state.visible" class="dialog-backdrop dialog-blur" @mousedown.self="dialogCancel" @keydown="handleKeydown">
      <div class="dialog" :class="{ 'dialog--alert': state.mode === 'alert' }" role="dialog" aria-modal="true">
        <div class="dialog-header">
          <span class="dialog-title">{{ state.title }}</span>
        </div>
        <div class="dialog-body">
          <p class="dialog-message" data-testid="app-dialog-message" tabindex="0">{{ state.message }}</p>
          <input
            v-if="state.mode === 'prompt'"
            ref="inputRef"
            v-model="inputValue"
            class="dialog-input"
            type="text"
            @keydown.enter="handleConfirm"
            @keydown.escape="dialogCancel"
          />
        </div>
        <div class="dialog-footer">
          <button
            v-if="state.mode !== 'alert'"
            class="btn btn-secondary"
            @click="dialogCancel"
          >{{ t('appDialog.cancel') }}</button>
          <button
            class="btn btn-primary"
            @click="handleConfirm"
          >{{ t('appDialog.ok') }}</button>
        </div>
      </div>
    </div>
    </Transition>
  </Teleport>
</template>

<style scoped>
.dialog-backdrop {
  position: fixed;
  inset: 0;
  background: rgba(0, 0, 0, 0.55);
  display: flex;
  align-items: center;
  justify-content: center;
  z-index: 2000;
}

.dialog {
  background: var(--c-base);
  border: 1px solid var(--c-surface1);
  border-radius: 8px;
  width: 360px;
  max-width: 90vw;
  max-height: 90vh;
  display: flex;
  flex-direction: column;
  box-shadow: 0 8px 24px rgba(0, 0, 0, 0.5);
}

.dialog--alert {
  width: 520px;
}

.dialog-header {
  padding: 16px 20px 0;
}

.dialog-title {
  font-size: 15px;
  font-weight: 600;
  color: var(--c-text);
}

.dialog-body {
  padding: 12px 20px 16px;
  min-height: 0;
}

.dialog-message {
  font-size: 13px;
  color: var(--c-subtext1);
  line-height: 1.5;
  margin: 0 0 8px;
  max-height: min(60vh, 480px);
  overflow-y: auto;
  overflow-wrap: anywhere;
  white-space: pre-wrap;
}

.dialog-input {
  width: 100%;
  padding: 8px 12px;
  background: var(--c-crust);
  border: 1px solid var(--c-surface1);
  border-radius: 6px;
  color: var(--c-text);
  font-size: 14px;
  box-sizing: border-box;
  margin-top: 4px;
}

.dialog-input:focus {
  outline: none;
  border-color: var(--c-blue);
}

.dialog-footer {
  display: flex;
  justify-content: flex-end;
  gap: 8px;
  padding: 0 20px 16px;
}

.btn {
  padding: 7px 20px;
  border: none;
  border-radius: 6px;
  cursor: pointer;
  font-size: 13px;
}

.btn-primary {
  background: var(--c-blue);
  color: var(--c-base);
}

.btn-primary:hover {
  background: var(--c-sapphire);
}

.btn-secondary {
  background: var(--c-surface1);
  color: var(--c-text);
}

.btn-secondary:hover {
  background: var(--c-surface2);
}
</style>
