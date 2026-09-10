export interface ToolbarMenuItem {
  id: string
  label: string
  title?: string
  disabled?: boolean
  busy?: boolean
  danger?: boolean
  separator?: boolean
  keepOpen?: boolean
  action: () => unknown
}
