import { beforeEach, describe, expect, it, vi } from 'vitest'
import { flushPromises, mount, type VueWrapper } from '@vue/test-utils'
import { nextTick, ref } from 'vue'
import { dialogKey } from '@shared/composables/useDialog'
import { useI18n } from '@shared/i18n'
import NewConnectionModal from '../src/components/NewConnectionModal.vue'

const { invokeMock, openMock } = vi.hoisted(() => ({ invokeMock: vi.fn(), openMock: vi.fn() }))
vi.mock('@tauri-apps/api/core', () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}))
vi.mock('@tauri-apps/plugin-dialog', () => ({
  open: (...args: unknown[]) => openMock(...args),
}))

type ModalVm = {
  openNew: () => void
  openEditConnection: (connectionId: string) => Promise<void>
}

function findInput(wrapper: VueWrapper, labelText: string) {
  const label = wrapper.findAll('label').find((item) => item.text().includes(labelText))
  expect(label, `label containing "${labelText}"`).toBeDefined()
  return label!.find('input')
}

function findFilePathInput(wrapper: VueWrapper, labelText: string) {
  const field = wrapper.findAll('.file-path-field').find((item) => item.text().includes(labelText))
  expect(field, `file path field containing "${labelText}"`).toBeDefined()
  return field!.find('input')
}

function mountModal(
  showAlert = vi.fn(() => Promise.resolve()),
  showConfirm = vi.fn(() => Promise.resolve(false)),
) {
  const selectedConnectionId = ref<string | null>(null)
  const selectedConnectionState = ref('Disconnected')
  const selectedCA = ref<number | null>(null)
  const selectedCategory = ref<string | null>(null)
  const refreshTree = vi.fn()
  return {
    showAlert,
    showConfirm,
    selectedConnectionId,
    selectedConnectionState,
    selectedCA,
    selectedCategory,
    refreshTree,
    wrapper: mount(NewConnectionModal, {
      props: { visible: true },
      global: {
        stubs: { Teleport: true, Transition: false },
        provide: {
          [dialogKey as symbol]: { showAlert, showConfirm },
          selectedConnectionId,
          selectedConnectionState,
          selectedCA,
          selectedCategory,
          refreshTree,
        },
      },
    }),
  }
}

function editableConnection(state = 'Disconnected') {
  return {
    id: 'conn_1', target_address: '127.0.0.1', port: 2404,
    common_addresses: [1], state,
    use_socks5: false, socks5_proxy_address: '127.0.0.1', socks5_proxy_port: 1080,
    socks5_username: '', socks5_password: '', socks5_remote_dns: true,
    use_tls: false, ca_file: '', cert_file: '', key_file: '',
    accept_invalid_certs: false, tls_version: 'auto',
    t0: 30, channel_retry_s: 5, t1: 15, t2: 10, t3: 20, k: 12, w: 8,
    default_qoi: 20, default_qcc: 5, interrogate_period_s: 0,
    counter_interrogate_period_s: 0, broadcast_address: 0xFFFF,
  }
}

describe('NewConnectionModal SOCKS5', () => {
  beforeEach(() => {
    invokeMock.mockReset()
    openMock.mockReset()
    localStorage.clear()
    useI18n().setLocale('zh-CN')
  })

  it.each(['Connected', 'Connecting', 'Error'])('取消编辑时保留 %s 连接', async (state) => {
    invokeMock.mockResolvedValue([editableConnection(state)])
    const { wrapper, showConfirm, selectedConnectionState } = mountModal()
    selectedConnectionState.value = state
    await (wrapper.vm as unknown as ModalVm).openEditConnection('conn_1')
    expect(showConfirm).toHaveBeenCalledOnce()
    expect(invokeMock.mock.calls.map(call => call[0])).toEqual(['list_connections'])
    expect(selectedConnectionState.value).toBe(state)
    expect(wrapper.emitted('update:visible')).toBeUndefined()
    wrapper.unmount()
  })

  it('断连失败时不打开编辑且保留原连接状态', async () => {
    invokeMock.mockImplementation((command) => command === 'list_connections'
      ? Promise.resolve([editableConnection('Connected')])
      : Promise.reject(new Error('disconnect failed')))
    const { wrapper, showAlert, selectedConnectionState } = mountModal(undefined, vi.fn(async () => true))
    selectedConnectionState.value = 'Connected'
    await (wrapper.vm as unknown as ModalVm).openEditConnection('conn_1')
    expect(showAlert).toHaveBeenCalledWith('Error: disconnect failed')
    expect(wrapper.emitted('update:visible')).toBeUndefined()
    expect(selectedConnectionState.value).toBe('Connected')
    expect(invokeMock.mock.calls.map(call => call[0])).toEqual(['list_connections', 'disconnect_master'])
    wrapper.unmount()
  })

  it('连续点击编辑只显示一次断连确认', async () => {
    let confirm!: (value: boolean) => void
    const showConfirm = vi.fn(() => new Promise<boolean>(resolve => { confirm = resolve }))
    invokeMock.mockResolvedValue([editableConnection('Connected')])
    const { wrapper } = mountModal(undefined, showConfirm)
    const vm = wrapper.vm as unknown as ModalVm
    const first = vm.openEditConnection('conn_1')
    await flushPromises()
    await vm.openEditConnection('conn_1')
    expect(showConfirm).toHaveBeenCalledOnce()
    confirm(false)
    await first
    wrapper.unmount()
  })

  it('新配置创建失败时不删除旧配置，允许重新保存', async () => {
    invokeMock.mockImplementation((command) => command === 'list_connections'
      ? Promise.resolve([editableConnection()])
      : Promise.reject(new Error('invalid config')))
    const { wrapper, showAlert, selectedConnectionId } = mountModal()
    selectedConnectionId.value = 'conn_1'
    await (wrapper.vm as unknown as ModalVm).openEditConnection('conn_1')
    await wrapper.find('.btn-primary').trigger('click')
    await flushPromises()
    expect(invokeMock.mock.calls.map(call => call[0])).toEqual(['list_connections', 'create_connection'])
    expect(selectedConnectionId.value).toBe('conn_1')
    expect(showAlert).toHaveBeenCalledWith('Error: invalid config')
    expect(wrapper.find('.btn-primary').attributes('disabled')).toBeUndefined()
    expect(wrapper.find('.modal-title').text()).toBe('编辑连接')
    wrapper.unmount()
  })

  it('创建完成后才替换旧连接，保存期间阻止重复提交和关闭', async () => {
    let finish!: (value: unknown) => void
    invokeMock.mockImplementation((command) => {
      if (command === 'list_connections') return Promise.resolve([editableConnection()])
      if (command === 'create_connection') return new Promise(resolve => { finish = resolve })
      return Promise.resolve()
    })
    const { wrapper, selectedConnectionId, selectedCA, selectedCategory, refreshTree, showConfirm } = mountModal()
    selectedConnectionId.value = 'conn_1'
    selectedCA.value = 99
    selectedCategory.value = 'single_point'
    await (wrapper.vm as unknown as ModalVm).openEditConnection('conn_1')
    expect(showConfirm).not.toHaveBeenCalled()
    await wrapper.find('.btn-primary').trigger('click')
    await wrapper.find('.btn-primary').trigger('click')
    await wrapper.find('.modal-backdrop').trigger('mousedown')
    expect(wrapper.find('fieldset').attributes('disabled')).toBeDefined()
    expect(invokeMock.mock.calls.map(call => call[0])).toEqual(['list_connections', 'create_connection'])
    expect(wrapper.emitted('update:visible')).toEqual([[true]])
    finish({ id: 'conn_2', timing_corrections: [] })
    await flushPromises()
    expect(invokeMock).toHaveBeenLastCalledWith('delete_connection', { id: 'conn_1' })
    expect(selectedConnectionId.value).toBe('conn_2')
    expect(selectedCA.value).toBeNull()
    expect(selectedCategory.value).toBeNull()
    expect(refreshTree).toHaveBeenCalledOnce()
    expect(wrapper.emitted('update:visible')?.at(-1)).toEqual([false])
    wrapper.unmount()
  })

  it('旧连接无法替换时清理候选连接，保留原选择和编辑表单', async () => {
    invokeMock.mockImplementation((command, args) => {
      if (command === 'list_connections') return Promise.resolve([editableConnection()])
      if (command === 'create_connection') return Promise.resolve({ id: 'conn_2' })
      if (args.id === 'conn_1') return Promise.reject(new Error('retire failed'))
      return Promise.resolve()
    })
    const { wrapper, selectedConnectionId, showAlert } = mountModal()
    selectedConnectionId.value = 'conn_1'
    await (wrapper.vm as unknown as ModalVm).openEditConnection('conn_1')
    await wrapper.find('.btn-primary').trigger('click')
    await flushPromises()
    expect(invokeMock).toHaveBeenLastCalledWith('delete_connection', { id: 'conn_2' })
    expect(selectedConnectionId.value).toBe('conn_1')
    expect(wrapper.find('.modal-title').text()).toBe('编辑连接')
    expect(showAlert).toHaveBeenCalledWith('Error: retire failed')
    wrapper.unmount()
  })

  it('保留 TLS 连接显式留空的证书路径，不混入其他连接的默认路径', async () => {
    localStorage.setItem('iec104master.newConnForm.v2', JSON.stringify({
      ca_file: '/other/ca.pem', cert_file: '/other/client.pem', key_file: '/other/key.pem',
    }))
    invokeMock.mockResolvedValue([{ ...editableConnection(), use_tls: true }])
    const { wrapper } = mountModal()
    await (wrapper.vm as unknown as ModalVm).openEditConnection('conn_1')
    for (const label of ['CA 证书路径', '客户端证书路径', '客户端密钥路径']) {
      expect((findFilePathInput(wrapper, label).element as HTMLInputElement).value).toBe('')
    }
    wrapper.unmount()
  })

  it('reveals proxy fields and submits a complete SOCKS5 request', async () => {
    invokeMock.mockResolvedValue({ timing_corrections: [] })
    const { wrapper } = mountModal()

    await findInput(wrapper, '通过 SOCKS5 代理连接').setValue(true)
    await findInput(wrapper, 'SOCKS5 代理地址').setValue('proxy.example.com')
    await findInput(wrapper, 'SOCKS5 代理端口').setValue('1088')
    await findInput(wrapper, '用户名（可选）').setValue('alice')
    await findInput(wrapper, '密码（可选）').setValue('s3cret')
    await findInput(wrapper, '由代理远程解析目标域名').setValue(false)
    await nextTick()

    const persisted = JSON.parse(localStorage.getItem('iec104master.newConnForm.v2') ?? '{}')
    expect(persisted.socks5_password).toBe('')

    await wrapper.find('.btn-primary').trigger('click')
    await flushPromises()

    expect(invokeMock).toHaveBeenCalledOnce()
    expect(invokeMock).toHaveBeenCalledWith('create_connection', {
      request: expect.objectContaining({
        use_socks5: true,
        socks5_proxy_address: 'proxy.example.com',
        socks5_proxy_port: 1088,
        socks5_username: 'alice',
        socks5_password: 's3cret',
        socks5_remote_dns: false,
      }),
    })
  })

  it('blocks incomplete username/password authentication before IPC', async () => {
    const { wrapper, showAlert } = mountModal()

    await findInput(wrapper, '通过 SOCKS5 代理连接').setValue(true)
    await findInput(wrapper, '用户名（可选）').setValue('alice')
    await wrapper.find('.btn-primary').trigger('click')
    await flushPromises()

    expect(invokeMock).not.toHaveBeenCalled()
    expect(showAlert).toHaveBeenCalledWith('SOCKS5 用户名和密码必须同时填写或同时留空')
  })

  it('scrubs hidden credentials when SOCKS5 is disabled before submit', async () => {
    invokeMock.mockResolvedValue({ timing_corrections: [] })
    const { wrapper } = mountModal()

    await findInput(wrapper, '通过 SOCKS5 代理连接').setValue(true)
    await findInput(wrapper, '用户名（可选）').setValue('alice')
    await findInput(wrapper, '密码（可选）').setValue('s3cret')
    await findInput(wrapper, '通过 SOCKS5 代理连接').setValue(false)
    await wrapper.find('.btn-primary').trigger('click')
    await flushPromises()

    expect(invokeMock).toHaveBeenCalledWith('create_connection', {
      request: expect.objectContaining({
        use_socks5: false,
        socks5_username: '',
        socks5_password: '',
      }),
    })
  })

  it('restores the safe new-connection form without a cached password', async () => {
    localStorage.setItem('iec104master.newConnForm.v2', JSON.stringify({
      use_socks5: true,
      socks5_proxy_address: '127.0.0.1',
      socks5_proxy_port: 1080,
      socks5_username: 'alice',
      socks5_password: '',
    }))
    const { wrapper } = mountModal()

    ;(wrapper.vm as unknown as ModalVm).openNew()
    await nextTick()

    expect((findInput(wrapper, '密码（可选）').element as HTMLInputElement).value).toBe('')
    expect((findInput(wrapper, '用户名（可选）').element as HTMLInputElement).value).toBe('alice')
  })

  it('submits Channel Retry independently from T0', async () => {
    invokeMock.mockResolvedValue({ timing_corrections: [] })
    const { wrapper } = mountModal()

    await findInput(wrapper, 't0').setValue('30')
    await findInput(wrapper, 'Channel Retry（重试间隔）').setValue('7')
    await wrapper.find('.btn-primary').trigger('click')
    await flushPromises()

    expect(invokeMock).toHaveBeenCalledWith('create_connection', {
      request: expect.objectContaining({
        t0: 30,
        channel_retry_s: 7,
      }),
    })
  })

  it('selects TLS certificate and key files and submits their paths', async () => {
    invokeMock.mockResolvedValue({ timing_corrections: [] })
    openMock
      .mockResolvedValueOnce('/tmp/ca.crt')
      .mockResolvedValueOnce('/tmp/client.pem')
      .mockResolvedValueOnce('/tmp/client.key')
    const { wrapper } = mountModal()

    await findInput(wrapper, '启用 TLS').setValue(true)
    const browseButtons = wrapper.findAll('.file-path-button')
    expect(browseButtons).toHaveLength(3)
    for (const button of browseButtons) {
      await button.trigger('click')
      await flushPromises()
    }

    expect((findFilePathInput(wrapper, 'CA 证书路径').element as HTMLInputElement).value).toBe('/tmp/ca.crt')
    expect((findFilePathInput(wrapper, '客户端证书路径').element as HTMLInputElement).value).toBe('/tmp/client.pem')
    expect((findFilePathInput(wrapper, '客户端密钥路径').element as HTMLInputElement).value).toBe('/tmp/client.key')
    expect(openMock).toHaveBeenNthCalledWith(1, expect.objectContaining({
      filters: [{ name: '证书文件', extensions: ['crt', 'cer', 'pem'] }],
    }))
    expect(openMock).toHaveBeenNthCalledWith(3, expect.objectContaining({
      filters: [{ name: '私钥文件', extensions: ['key', 'pem'] }],
    }))

    await wrapper.find('.btn-primary').trigger('click')
    await flushPromises()

    expect(invokeMock).toHaveBeenCalledWith('create_connection', {
      request: expect.objectContaining({
        ca_file: '/tmp/ca.crt',
        cert_file: '/tmp/client.pem',
        key_file: '/tmp/client.key',
      }),
    })
  })

  it('confirms, disconnects, and opens enabled TLS/SOCKS5 settings for direct editing', async () => {
    const connection = {
      id: 'conn-secure',
      target_address: 'secure.example.com',
      port: 2404,
      common_addresses: [1],
      state: 'Connected',
      use_socks5: true,
      socks5_proxy_address: 'proxy.example.com',
      socks5_proxy_port: 1080,
      socks5_username: 'alice',
      socks5_password: 'secret',
      socks5_remote_dns: true,
      use_tls: true,
      ca_file: '/tmp/ca.crt',
      cert_file: '/tmp/client.crt',
      key_file: '/tmp/client.key',
      accept_invalid_certs: false,
      tls_version: 'auto',
      t0: 30,
      channel_retry_s: 5,
      t1: 15,
      t2: 10,
      t3: 20,
      k: 12,
      w: 8,
      default_qoi: 20,
      default_qcc: 5,
      interrogate_period_s: 0,
      counter_interrogate_period_s: 0,
      broadcast_address: 0xFFFF,
    }
    invokeMock.mockImplementation((command: string) => {
      if (command === 'list_connections') return Promise.resolve([connection])
      return Promise.resolve(undefined)
    })
    const showConfirm = vi.fn(() => Promise.resolve(true))
    const {
      wrapper,
      selectedConnectionId,
      selectedConnectionState,
      refreshTree,
    } = mountModal(undefined, showConfirm)
    selectedConnectionId.value = connection.id
    selectedConnectionState.value = 'Connected'

    await (wrapper.vm as unknown as ModalVm).openEditConnection(connection.id)
    await flushPromises()

    expect(showConfirm).toHaveBeenCalledWith('编辑连接会先断开当前连接，是否继续？')
    expect(invokeMock).toHaveBeenCalledWith('disconnect_master', { id: connection.id })
    expect(selectedConnectionState.value).toBe('Disconnected')
    expect(refreshTree).toHaveBeenCalledOnce()
    expect((findInput(wrapper, '通过 SOCKS5 代理连接').element as HTMLInputElement).checked).toBe(true)
    expect((findInput(wrapper, '启用 TLS').element as HTMLInputElement).checked).toBe(true)
    expect((findFilePathInput(wrapper, '客户端证书路径').element as HTMLInputElement).value)
      .toBe('/tmp/client.crt')
    expect((findFilePathInput(wrapper, '客户端密钥路径').element as HTMLInputElement).value)
      .toBe('/tmp/client.key')
  })
})
