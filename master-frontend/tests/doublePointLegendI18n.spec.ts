import { beforeEach, describe, expect, it } from 'vitest'
import { mount } from '@vue/test-utils'
import { nextTick } from 'vue'
import { useI18n } from '@shared/i18n'
import DoublePointLegend from '../src/components/DoublePointLegend.vue'

describe('DoublePointLegend localization', () => {
  beforeEach(() => {
    useI18n().setLocale('en-US')
  })

  it('localizes DPI 0/3 tokens', async () => {
    const wrapper = mount(DoublePointLegend, {
      global: { stubs: { Teleport: true } },
    })

    await wrapper.find('.dp-help').trigger('click')
    await nextTick()
    expect(wrapper.text()).toContain('Intermediate')
    expect(wrapper.text()).toContain('Indeterminate')
    expect(wrapper.text()).not.toMatch(/[\u3400-\u9fff]/u)

    useI18n().setLocale('zh-CN')
    await nextTick()
    expect(wrapper.text()).toContain('中间')
    expect(wrapper.text()).toContain('不确定')
  })
})
