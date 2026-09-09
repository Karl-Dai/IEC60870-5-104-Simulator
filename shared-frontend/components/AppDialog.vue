<script setup lang="ts">
import { ref, watch, nextTick } from 'vue'
import { useDialogState } from '../composables/useDialog'
import { useI18n } from '../i18n'
import AppButton from './ui/AppButton.vue'
import AppInput from './ui/AppInput.vue'

const { state, dialogConfirm, dialogCancel } = useDialogState()
const { t } = useI18n()
const inputRef = ref<InstanceType<typeof AppInput> | null>(null)
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
          <AppInput
            v-if="state.mode === 'prompt'"
            ref="inputRef"
            v-model="inputValue"
            class="dialog-input"
            @keydown.enter="handleConfirm"
            @keydown.escape="dialogCancel"
          />
        </div>
        <div class="dialog-footer">
          <AppButton
            v-if="state.mode !== 'alert'"
            variant="secondary"
            class="btn btn-secondary"
            @click="dialogCancel"
          >{{ t('appDialog.cancel') }}</AppButton>
          <AppButton
            variant="primary"
            class="btn btn-primary"
            @click="handleConfirm"
          >{{ t('appDialog.ok') }}</AppButton>
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
  z-index: var(--z-dialog);
}

.dialog {
  background: var(--bg-app);
  border: 1px solid var(--border-strong);
  border-radius: var(--radius-lg);
  width: 360px;
  max-width: 90vw;
  max-height: 90vh;
  display: flex;
  flex-direction: column;
  box-shadow: var(--shadow-lg);
}

.dialog--alert {
  width: 520px;
}

.dialog-header {
  padding: var(--space-4) var(--space-5) 0;
}

.dialog-title {
  font-size: var(--text-lg);
  font-weight: 600;
  color: var(--text-primary);
}

.dialog-body {
  padding: var(--space-3) var(--space-5) var(--space-4);
  min-height: 0;
}

.dialog-message {
  font-size: var(--text-md);
  color: var(--text-secondary);
  line-height: 1.5;
  margin: 0 0 var(--space-2);
  max-height: min(60vh, 480px);
  overflow-y: auto;
  overflow-wrap: anywhere;
  white-space: pre-wrap;
}

.dialog-input {
  margin-top: var(--space-1);
}

.dialog-footer {
  display: flex;
  justify-content: flex-end;
  gap: var(--space-2);
  padding: 0 var(--space-5) var(--space-4);
}
</style>
