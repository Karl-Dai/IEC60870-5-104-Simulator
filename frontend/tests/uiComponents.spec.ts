import { describe, expect, it } from 'vitest'
import { mount } from '@vue/test-utils'
import AppButton from '@shared/components/ui/AppButton.vue'
import AppInput from '@shared/components/ui/AppInput.vue'
import AppSelect from '@shared/components/ui/AppSelect.vue'
import AppCheckbox from '@shared/components/ui/AppCheckbox.vue'
import AppToggle from '@shared/components/ui/AppToggle.vue'
import AppTooltip from '@shared/components/ui/AppTooltip.vue'
import AppIcon from '@shared/components/ui/AppIcon.vue'

describe('ui/AppButton', () => {
  it('renders variant and size classes, disables while loading', () => {
    const wrapper = mount(AppButton, {
      props: { variant: 'primary', size: 'sm', loading: true },
      slots: { default: '保存' },
    })
    const btn = wrapper.find('button')
    expect(btn.classes()).toContain('app-btn-primary')
    expect(btn.classes()).toContain('app-btn-sm')
    expect(btn.attributes('disabled')).toBeDefined()
    expect(wrapper.find('.spinner').exists()).toBe(true)
  })

  it('renders an icon when the icon prop is set', () => {
    const wrapper = mount(AppButton, { props: { icon: 'plus' }, slots: { default: '添加' } })
    expect(wrapper.find('svg.app-icon').exists()).toBe(true)
  })
})

describe('ui/AppInput', () => {
  it('binds v-model and marks the error state', async () => {
    const wrapper = mount(AppInput, { props: { error: true, placeholder: 'IOA' } })
    const input = wrapper.find('input')
    expect(input.classes()).toContain('error')
    expect(input.attributes('aria-invalid')).toBe('true')
    await input.setValue('1001')
    expect(wrapper.emitted('update:modelValue')?.[0]).toEqual(['1001'])
  })
})

describe('ui/AppSelect', () => {
  it('lists options and emits the selected value', async () => {
    const wrapper = mount(AppSelect, {
      props: { options: [{ value: 'a', label: '甲' }, { value: 'b', label: '乙' }] },
    })
    const select = wrapper.find('select')
    expect(select.findAll('option')).toHaveLength(2)
    await select.setValue('b')
    expect(wrapper.emitted('update:modelValue')?.[0]).toEqual(['b'])
  })
})

describe('ui/AppCheckbox', () => {
  it('toggles v-model and reflects the checked class', async () => {
    const wrapper = mount(AppCheckbox, { props: { label: '启用' } })
    const input = wrapper.find('input[type=checkbox]')
    await input.setValue(true)
    expect(wrapper.emitted('update:modelValue')?.[0]).toEqual([true])
    expect(wrapper.find('.app-checkbox').classes()).toContain('checked')
  })
})

describe('ui/AppToggle', () => {
  it('exposes switch semantics', async () => {
    const wrapper = mount(AppToggle, { props: { label: '模拟' } })
    const input = wrapper.find('input[role=switch]')
    expect(input.attributes('aria-checked')).toBe('false')
    await input.setValue(true)
    expect(wrapper.emitted('update:modelValue')?.[0]).toEqual([true])
  })
})

describe('ui/AppTooltip', () => {
  it('carries the tip text in data-tip for the CSS bubble', () => {
    const wrapper = mount(AppTooltip, { props: { text: '提示' }, slots: { default: '?' } })
    expect(wrapper.find('.app-tooltip').attributes('data-tip')).toBe('提示')
  })
})

describe('ui/AppIcon', () => {
  it('renders a sized svg for a known icon', () => {
    const wrapper = mount(AppIcon, { props: { name: 'check', size: 16 } })
    const svg = wrapper.find('svg')
    expect(svg.exists()).toBe(true)
    expect(svg.attributes('width')).toBe('16')
    expect(svg.find('path').exists()).toBe(true)
  })
})
