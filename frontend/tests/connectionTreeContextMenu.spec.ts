import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { flushPromises, mount } from '@vue/test-utils'
import { ref } from 'vue'
import { dialogKey } from '@shared/composables/useDialog'
import { useI18n } from '@shared/i18n'
import ConnectionTree from '../src/components/ConnectionTree.vue'

const invokeMock = vi.fn()
const alertMock = vi.fn(() => Promise.resolve())
const confirmMock = vi.fn(() => Promise.resolve(true))
const promptMock = vi.fn()

vi.mock('@tauri-apps/api/core', () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}))

function mountTree(state = 'Stopped') {
  invokeMock.mockImplementation((command: string) => {
    if (command === 'list_servers') {
      return Promise.resolve([{
        id: 'server_1',
        bind_address: '0.0.0.0',
        port: 2404,
        state,
        station_count: 1,
        client_count: 2,
      }])
    }
    if (command === 'list_stations') {
      return Promise.resolve([{
        common_address: 456,
        name: '220TVAA',
        point_count: 3,
        category_counts: {
          single_point: 2,
          float_measured: 1,
        },
      }])
    }
    if (command === 'list_client_connections') {
      return Promise.resolve([
        { peer_address: '10.0.0.2:51001', data_transfer_active: true },
        { peer_address: '10.0.0.3:51002', data_transfer_active: false },
      ])
    }
    return Promise.resolve(undefined)
  })

  return mount(ConnectionTree, {
    attachTo: document.body,
    global: {
      stubs: { Teleport: true, Transition: false },
      provide: {
        [dialogKey as symbol]: {
          showAlert: alertMock,
          showConfirm: confirmMock,
          showPrompt: promptMock,
        },
        treeRefreshKey: ref(0),
        dataRefreshKey: ref(0),
        selectedServerId: ref<string | null>(null),
        selectedCA: ref<number | null>(null),
        selectedCategory: ref<string | null>(null),
        categoryCounts: ref(new Map<string, number>()),
      },
    },
  })
}

let wrapper: ReturnType<typeof mountTree>
let outside: HTMLButtonElement
beforeEach(async () => {
  invokeMock.mockReset()
  useI18n().setLocale('en-US')
  outside = document.createElement('button')
  document.body.append(outside)
  wrapper = mountTree()
  await flushPromises()
})
afterEach(() => { wrapper?.unmount(); outside?.remove() })
async function openMenu(selector = '.server-node') {
  await wrapper.get(selector).trigger('contextmenu', { clientX: 160, clientY: 120 })
  expect(wrapper.find('.context-menu').exists()).toBe(true)
}

describe('ConnectionTree context-menu dismissal', () => {
  it('closes on outside pointerdown even when the target stops propagation', async () => {
    await openMenu()
    outside.addEventListener('pointerdown', event => event.stopPropagation())
    outside.dispatchEvent(new Event('pointerdown', { bubbles: true }))
    await flushPromises()
    expect(wrapper.find('.context-menu').exists()).toBe(false)
  })

  it('closes on keyboard-generated outside click without swallowing the action', async () => {
    const action = vi.fn()
    outside.addEventListener('click', action)
    await openMenu()
    outside.click()
    await flushPromises()
    expect(action).toHaveBeenCalledOnce()
    expect(wrapper.find('.context-menu').exists()).toBe(false)
  })

  it.each(['.server-node', '.station-node', '.category-node', '.node-arrow'])('closes when clicking %s with click.stop', async selector => {
    await openMenu()
    await wrapper.findAll(selector)[0].trigger('click')
    expect(wrapper.find('.context-menu').exists()).toBe(false)
  })

  it('keeps menu pointerdown intact so the selected item can execute', async () => {
    await openMenu()
    const settings = wrapper.findAll('.context-menu-item').find(item => item.text().includes('Address / TLS'))!
    await settings.trigger('pointerdown')
    expect(wrapper.find('.context-menu').exists()).toBe(true)
    await settings.trigger('click')
    expect(wrapper.emitted('edit-server')).toEqual([['server_1']])
    expect(wrapper.find('.context-menu').exists()).toBe(false)
  })

  it('closes station menus with Escape and scroll, and server menus on window blur', async () => {
    await openMenu('.station-node')
    document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }))
    await flushPromises()
    expect(wrapper.find('.context-menu').exists()).toBe(false)
    await openMenu('.station-node')
    wrapper.get('.connection-tree').element.dispatchEvent(new Event('scroll'))
    await flushPromises()
    expect(wrapper.find('.context-menu').exists()).toBe(false)
    await openMenu()
    window.dispatchEvent(new Event('blur'))
    await flushPromises()
    expect(wrapper.find('.context-menu').exists()).toBe(false)
  })
})
