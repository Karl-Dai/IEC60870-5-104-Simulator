import { ref, watch, type Ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import {
  type ProtocolTimingConfig,
  type RemoteOperationConfig,
  DEFAULT_PROTOCOL_TIMING,
  DEFAULT_REMOTE_OPS,
} from '../types'

export interface RemoteParamsApplyResult {
  ok: boolean
  error: string | null
}

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
  ): Promise<RemoteParamsApplyResult> {
    if (!targetServerId) return { ok: false, error: null }
    try {
      await invoke('set_protocol_timing', {
        request: { server_id: targetServerId, timing: value },
      })
      return { ok: true, error: null }
    } catch (e) {
      // 保存可能跨越服务器选择/弹窗会话。错误必须作为本次调用的局部结果
      // 返回，由持有会话 epoch 的组件决定是否仍可显示，不能在这里污染
      // 当前服务器的共享加载错误。
      return { ok: false, error: String(e) }
    }
  }

  async function applyOps(
    targetServerId: string | null = selectedServerId.value,
    value: RemoteOperationConfig = ops.value,
  ): Promise<RemoteParamsApplyResult> {
    if (!targetServerId) return { ok: false, error: null }
    try {
      await invoke('set_remote_operation_config', {
        request: { server_id: targetServerId, ops: value },
      })
      return { ok: true, error: null }
    } catch (e) {
      return { ok: false, error: String(e) }
    }
  }

  watch(selectedServerId, load, { immediate: true })

  return { timing, ops, loading, lastError, load, applyTiming, applyOps }
}
