import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { flushPromises, mount, type VueWrapper } from '@vue/test-utils'
import FilePathInput from '@shared/components/FilePathInput.vue'

const invokeMock = vi.hoisted(() => vi.fn())
vi.mock('@tauri-apps/api/core', () => ({ invoke: invokeMock }))
let wrapper: VueWrapper | undefined

beforeEach(() => invokeMock.mockReset())
afterEach(() => wrapper?.unmount())

describe('TLS file selection', () => {
  it('fills the certificate path selected by the native picker', async () => {
    invokeMock.mockResolvedValue('/证书/client cert.pem')
    wrapper = mount(FilePathInput, { props: { modelValue: '', label: 'CA', kind: 'certificate' } })
    await wrapper.get('button').trigger('click')
    await flushPromises()
    expect(invokeMock).toHaveBeenCalledWith('choose_tls_file', expect.objectContaining({ kind: 'certificate', title: 'CA' }))
    expect(wrapper.emitted('update:modelValue')).toEqual([['/证书/client cert.pem']])
  })

  it('preserves the existing private-key path when selection is cancelled', async () => {
    invokeMock.mockResolvedValue(null)
    wrapper = mount(FilePathInput, { props: { modelValue: '/existing.key', label: 'Private key', kind: 'private-key' } })
    await wrapper.get('button').trigger('click')
    await flushPromises()
    expect(invokeMock).toHaveBeenCalledWith('choose_tls_file', expect.objectContaining({ kind: 'private-key' }))
    expect(wrapper.emitted('update:modelValue')).toBeUndefined()
    expect((wrapper.get('input').element as HTMLInputElement).value).toBe('/existing.key')
    await wrapper.get('input').setValue('/manually entered.key')
    expect(wrapper.emitted('update:modelValue')).toEqual([['/manually entered.key']])
  })
})
