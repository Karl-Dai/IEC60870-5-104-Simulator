import { ref, watch, type Ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import {
  type ProtocolTimingConfig,
  type RemoteOperationConfig,
  DEFAULT_PROTOCOL_TIMING,
  DEFAULT_REMOTE_OPS,
} from '../types'

/**
 * 与当前选中的从站服务器联动:加载/应用协议时序与远动运行参数,
 * 启停固定变位后台任务。所有命令对接 commands.rs 中的 Tauri 命令。
 */
export function useRemoteParams(selectedServerId: Ref<string | null>) {
  const timing = ref<ProtocolTimingConfig>({ ...DEFAULT_PROTOCOL_TIMING })
  const ops = ref<RemoteOperationConfig>(JSON.parse(JSON.stringify(DEFAULT_REMOTE_OPS)))
  const loading = ref(false)
  const lastError = ref<string | null>(null)
  let loadEpoch = 0

  async function load(): Promise<boolean> {
    const id = selectedServerId.value
    const epoch = ++loadEpoch
    if (!id) {
      timing.value = { ...DEFAULT_PROTOCOL_TIMING }
      ops.value = JSON.parse(JSON.stringify(DEFAULT_REMOTE_OPS))
      loading.value = false
      lastError.value = null
      return true
    }
    loading.value = true
    lastError.value = null
    try {
      const [t, o] = await Promise.all([
        invoke<ProtocolTimingConfig>('get_protocol_timing', { serverId: id }),
        invoke<RemoteOperationConfig>('get_remote_operation_config', { serverId: id }),
      ])
      // selectedServerId 可在 IPC 往返期间变化。同一服务器也可能因“打开时重载”
      // 并发发出多次请求；只有最新一次、且目标仍是当前服务器的响应可以落地。
      if (epoch !== loadEpoch || selectedServerId.value !== id) return false
      timing.value = t
      ops.value = o
      return true
    } catch (e) {
      if (epoch === loadEpoch && selectedServerId.value === id) {
        lastError.value = String(e)
      }
      return false
    } finally {
      if (epoch === loadEpoch && selectedServerId.value === id) {
        loading.value = false
      }
    }
  }

  async function applyTiming(
    targetServerId: string | null = selectedServerId.value,
    value: ProtocolTimingConfig = timing.value,
  ): Promise<boolean> {
    if (!targetServerId) return false
    lastError.value = null
    try {
      await invoke('set_protocol_timing', {
        request: { server_id: targetServerId, timing: value },
      })
      return true
    } catch (e) {
      lastError.value = String(e)
      return false
    }
  }

  async function applyOps(
    targetServerId: string | null = selectedServerId.value,
    value: RemoteOperationConfig = ops.value,
  ): Promise<boolean> {
    if (!targetServerId) return false
    lastError.value = null
    try {
      await invoke('set_remote_operation_config', {
        request: { server_id: targetServerId, ops: value },
      })
      return true
    } catch (e) {
      lastError.value = String(e)
      return false
    }
  }

  watch(selectedServerId, load, { immediate: true })

  return { timing, ops, loading, lastError, load, applyTiming, applyOps }
}
