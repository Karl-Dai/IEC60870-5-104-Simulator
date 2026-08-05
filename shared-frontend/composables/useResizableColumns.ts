import { computed, onUnmounted, shallowRef } from 'vue'

export function useResizableColumns<K extends string>(
  initialWidths: Record<K, number>,
  minimumWidths: Record<K, number>,
) {
  const widths = shallowRef<Record<K, number>>({ ...initialWidths })
  const tableWidth = computed(() => Object.values<number>(widths.value).reduce((sum, width) => sum + width, 0))

  let activeKey: K | null = null
  let startX = 0
  let startWidth = 0

  function setWidth(key: K, width: number) {
    widths.value = {
      ...widths.value,
      [key]: Math.max(minimumWidths[key], Math.round(width)),
    }
  }

  function onPointerMove(event: PointerEvent) {
    if (activeKey === null) return
    setWidth(activeKey, startWidth + event.clientX - startX)
  }

  function stopResize() {
    activeKey = null
    window.removeEventListener('pointermove', onPointerMove)
    window.removeEventListener('pointerup', stopResize)
    window.removeEventListener('pointercancel', stopResize)
  }

  function startResize(key: K, event: PointerEvent) {
    event.preventDefault()
    event.stopPropagation()
    stopResize()
    activeKey = key
    startX = event.clientX
    const renderedWidth = (event.currentTarget as HTMLElement | null)
      ?.parentElement?.getBoundingClientRect().width ?? 0
    startWidth = renderedWidth > 0 ? renderedWidth : widths.value[key]
    window.addEventListener('pointermove', onPointerMove)
    window.addEventListener('pointerup', stopResize)
    window.addEventListener('pointercancel', stopResize)
  }

  function resizeWithKeyboard(key: K, event: KeyboardEvent) {
    if (event.key !== 'ArrowLeft' && event.key !== 'ArrowRight') return
    event.preventDefault()
    event.stopPropagation()
    setWidth(key, widths.value[key] + (event.key === 'ArrowLeft' ? -10 : 10))
  }

  onUnmounted(stopResize)

  return { widths, tableWidth, startResize, resizeWithKeyboard }
}
