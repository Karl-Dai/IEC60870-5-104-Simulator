import { beforeEach, describe, expect, it, vi } from 'vitest'
import { flushPromises, mount, type VueWrapper } from '@vue/test-utils'
import { nextTick, ref } from 'vue'
import { dialogKey } from '@shared/composables/useDialog'
import { useI18n } from '@shared/i18n'
import NewConnectionModal from '../src/components/NewConnectionModal.vue'

const { invokeMock } = vi.hoisted(() => ({ invokeMock: vi.fn() }))
vi.mock('@tauri-apps/api/core', () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}))

type ModalVm = {
  openNew: () => void
}

function findInput(wrapper: VueWrapper, labelText: string) {
  const label = wrapper.findAll('label').find((item) => item.text().includes(labelText))
  expect(label, `label containing "${labelText}"`).toBeDefined()
  return label!.find('input')
}

function mountModal(showAlert = vi.fn(() => Promise.resolve())) {
  return {
    showAlert,
    wrapper: mount(NewConnectionModal, {
      props: { visible: true },
      global: {
        stubs: { Teleport: true, Transition: false },
        provide: {
          [dialogKey as symbol]: { showAlert },
          selectedConnectionId: ref<string | null>(null),
          selectedConnectionState: ref('Disconnected'),
          refreshTree: vi.fn(),
        },
      },
    }),
  }
}

describe('NewConnectionModal SOCKS5', () => {
  beforeEach(() => {
    invokeMock.mockReset()
    localStorage.clear()
    useI18n().setLocale('zh-CN')
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
})
