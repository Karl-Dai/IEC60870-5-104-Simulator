<script setup lang="ts">
import { useI18n } from '@shared/i18n'

defineProps<{
  visible: boolean
}>()

const emit = defineEmits<{
  choose: [mode: 'merge' | 'replace']
  cancel: []
}>()

const { t } = useI18n()

function handleKeydown(event: KeyboardEvent) {
  if (event.key === 'Escape') emit('cancel')
}
</script>

<template>
  <Teleport to="body">
    <Transition name="dialog-pop">
      <div
        v-if="visible"
        class="csv-mode-backdrop dialog-blur"
        data-testid="csv-import-mode-modal"
        @mousedown.self="emit('cancel')"
        @keydown="handleKeydown"
      >
        <section class="csv-mode-dialog" role="dialog" aria-modal="true" :aria-label="t('toolbar.csvImportModeTitle')">
          <header>
            <h3>{{ t('toolbar.csvImportModeTitle') }}</h3>
            <p>{{ t('toolbar.csvImportModeHint') }}</p>
          </header>

          <div class="csv-mode-options">
            <button class="csv-mode-option csv-mode-option--merge" data-testid="csv-mode-merge" @click="emit('choose', 'merge')">
              <strong>{{ t('toolbar.csvMerge') }}</strong>
              <span>{{ t('toolbar.csvMergeHint') }}</span>
            </button>
            <button class="csv-mode-option csv-mode-option--replace" data-testid="csv-mode-replace" @click="emit('choose', 'replace')">
              <strong>{{ t('toolbar.csvReplace') }}</strong>
              <span>{{ t('toolbar.csvReplaceHint') }}</span>
            </button>
          </div>

          <footer>
            <button class="csv-mode-cancel" data-testid="csv-mode-cancel" @click="emit('cancel')">
              {{ t('common.cancel') }}
            </button>
          </footer>
        </section>
      </div>
    </Transition>
  </Teleport>
</template>

<style scoped>
.csv-mode-backdrop {
  position: fixed;
  inset: 0;
  z-index: 2100;
  display: flex;
  align-items: center;
  justify-content: center;
  padding: 20px;
  background: rgba(0, 0, 0, 0.58);
}

.csv-mode-dialog {
  width: min(520px, 94vw);
  padding: 20px;
  border: 1px solid var(--c-surface1);
  border-radius: 10px;
  background: var(--c-base);
  box-shadow: 0 18px 48px rgba(0, 0, 0, 0.48);
}

.csv-mode-dialog h3 {
  margin: 0;
  color: var(--c-text);
  font-size: 16px;
}

.csv-mode-dialog header p {
  margin: 8px 0 0;
  color: var(--c-subtext0);
  font-size: 12px;
  line-height: 1.55;
}

.csv-mode-options {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 10px;
  margin-top: 18px;
}

.csv-mode-option {
  display: flex;
  min-height: 112px;
  flex-direction: column;
  gap: 8px;
  padding: 14px;
  text-align: left;
  border: 1px solid var(--c-surface1);
  border-radius: 8px;
  background: var(--c-mantle);
  color: var(--c-text);
  cursor: pointer;
}

.csv-mode-option strong {
  color: var(--c-blue);
  font-size: 14px;
}

.csv-mode-option span {
  color: var(--c-subtext0);
  font-size: 12px;
  line-height: 1.5;
}

.csv-mode-option:hover {
  border-color: var(--c-blue);
  background: var(--c-surface0);
}

.csv-mode-option--replace strong {
  color: var(--c-red);
}

.csv-mode-option--replace:hover {
  border-color: var(--c-red);
}

.csv-mode-dialog footer {
  display: flex;
  justify-content: flex-end;
  margin-top: 14px;
}

.csv-mode-cancel {
  padding: 7px 18px;
  border: none;
  border-radius: 6px;
  background: var(--c-surface1);
  color: var(--c-text);
  cursor: pointer;
}

.csv-mode-cancel:hover {
  background: var(--c-surface2);
}

@media (max-width: 520px) {
  .csv-mode-options {
    grid-template-columns: 1fr;
  }
}
</style>
