// issue #28 同类残余:「加载配置」保存后只 refreshTree(),从不 refreshData()。
// 导入一份 sync_tb_by_category 已开的配置后,数据表沿用旧快照 —— 点位与 +TB 徽标
// 都按老配置显示。断言 load_config 成功后两个刷新钩子都被调用。
import { describe, it, expect, vi, beforeEach } from 'vitest'
import { ref } from 'vue'
import { mount, flushPromises } from '@vue/test-utils'
import { dialogKey } from '@shared/composables/useDialog'
import Toolbar from '../src/components/Toolbar.vue'

const invokeMock = vi.fn()
vi.mock('@tauri-apps/api/core', () => ({ invoke: (...a: unknown[]) => invokeMock(...a) }))
const openMock = vi.fn()
vi.mock('@tauri-apps/plugin-dialog', () => ({
  open: (...a: unknown[]) => openMock(...a),
  save: () => Promise.resolve(null),
}))
vi.mock('@tauri-apps/plugin-opener', () => ({ openUrl: () => Promise.resolve() }))

const refreshTree = vi.fn()
const refreshData = vi.fn()

function mountToolbar() {
  return mount(Toolbar, {
    global: {
      provide: {
        selectedServerId: ref<string | null>('s1'),
        selectedServerState: ref('Stopped'),
        refreshTree,
        refreshData,
        openParseFrame: () => {},
        openRuntimeParamsDrawer: () => {},
        checkUpdate: () => Promise.resolve(null),
        [dialogKey as symbol]: {
          showAlert: () => Promise.resolve(),
          showPrompt: () => Promise.resolve(null),
          showConfirm: () => Promise.resolve(false),
        },
      },
      stubs: { AboutDialog: true, LangSwitch: true, VersionBadge: true, NewServerModal: true },
    },
  })
}

// 「保存配置 · 加载配置」是同一组里的两个按钮,加载在后
function loadConfigButton(w: ReturnType<typeof mountToolbar>) {
  const group = w.findAll('.toolbar-group').at(-1)!
  return group.findAll('button').at(-1)!
}

beforeEach(() => {
  invokeMock.mockReset()
  invokeMock.mockResolvedValue(3)
  openMock.mockReset()
  refreshTree.mockClear()
  refreshData.mockClear()
})

describe('Toolbar 加载配置', () => {
  it('load_config 成功后同时刷新树与数据(+TB 徽标不再按老快照)', async () => {
    openMock.mockResolvedValue('/tmp/cfg.json')
    const w = mountToolbar()

    await loadConfigButton(w).trigger('click')
    await flushPromises()

    expect(invokeMock).toHaveBeenCalledWith('load_config', { path: '/tmp/cfg.json' })
    expect(refreshTree).toHaveBeenCalledTimes(1)
    expect(refreshData).toHaveBeenCalledTimes(1)
    w.unmount()
  })

  it('取消选择文件时不刷新', async () => {
    openMock.mockResolvedValue(null)
    const w = mountToolbar()

    await loadConfigButton(w).trigger('click')
    await flushPromises()

    expect(refreshTree).not.toHaveBeenCalled()
    expect(refreshData).not.toHaveBeenCalled()
    w.unmount()
  })
})
