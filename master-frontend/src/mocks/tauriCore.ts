import type { ConnectionInfo } from '../types'

type InvokeArgs = Record<string, unknown>
type Invocation = {
  command: string
  args?: InvokeArgs
}

type CreateConnectionRequest = {
  target_address: string
  port: number
  common_addresses?: number[]
  broadcast_address?: number
  use_socks5?: boolean
  socks5_proxy_address?: string
  socks5_proxy_port?: number
  socks5_username?: string
  socks5_password?: string
  socks5_remote_dns?: boolean
  use_tls?: boolean
  ca_file?: string
  cert_file?: string
  key_file?: string
  accept_invalid_certs?: boolean
  tls_version?: ConnectionInfo['tls_version']
  t0?: number
  channel_retry_s?: number
  t1?: number
  t2?: number
  t3?: number
  k?: number
  w?: number
  default_qoi?: number
  default_qcc?: number
  interrogate_period_s?: number
  counter_interrogate_period_s?: number
}

export type TauriMockController = {
  connections: ConnectionInfo[]
  invocations: Invocation[]
  reset: () => void
}

declare global {
  interface Window {
    __IEC104_TAURI_MOCK__?: TauriMockController
  }
}

function clone<T>(value: T): T {
  return structuredClone(value)
}

function createController(): TauriMockController {
  const controller: TauriMockController = {
    connections: [],
    invocations: [],
    reset() {
      controller.connections.splice(0)
      controller.invocations.splice(0)
      nextConnectionId = 1
    },
  }
  return controller
}

const controller = window.__IEC104_TAURI_MOCK__ ?? createController()
window.__IEC104_TAURI_MOCK__ = controller
let nextConnectionId = controller.connections.length + 1

function connectionFromRequest(request: CreateConnectionRequest): ConnectionInfo {
  return {
    id: `conn_${nextConnectionId++}`,
    target_address: request.target_address,
    port: request.port,
    common_addresses: request.common_addresses?.length ? request.common_addresses : [1],
    state: 'Disconnected',
    use_socks5: request.use_socks5 ?? false,
    socks5_proxy_address: request.socks5_proxy_address ?? '127.0.0.1',
    socks5_proxy_port: request.socks5_proxy_port ?? 1080,
    socks5_username: request.socks5_username ?? '',
    socks5_password: request.socks5_password ?? '',
    socks5_remote_dns: request.socks5_remote_dns ?? true,
    use_tls: request.use_tls ?? false,
    ca_file: request.ca_file ?? '',
    cert_file: request.cert_file ?? '',
    key_file: request.key_file ?? '',
    accept_invalid_certs: request.accept_invalid_certs ?? false,
    tls_version: request.tls_version ?? 'auto',
    t0: request.t0 ?? 30,
    channel_retry_s: request.channel_retry_s ?? 5,
    t1: request.t1 ?? 15,
    t2: request.t2 ?? 10,
    t3: request.t3 ?? 20,
    k: request.k ?? 12,
    w: request.w ?? 8,
    default_qoi: request.default_qoi ?? 20,
    default_qcc: request.default_qcc ?? 5,
    interrogate_period_s: request.interrogate_period_s ?? 0,
    counter_interrogate_period_s: request.counter_interrogate_period_s ?? 0,
    broadcast_address: request.broadcast_address ?? 0xffff,
    timing_corrections: [],
  }
}

function findConnection(args?: InvokeArgs): ConnectionInfo {
  const id = String(args?.id ?? '')
  const connection = controller.connections.find((item) => item.id === id)
  if (!connection) throw new Error(`Mock connection not found: ${id}`)
  return connection
}

/**
 * Browser-only Tauri IPC replacement for deterministic UI/E2E verification.
 * It is selected exclusively by `vite --mode mock`; production and normal
 * development builds continue resolving `@tauri-apps/api/core`.
 */
export async function invoke<T>(command: string, args?: InvokeArgs): Promise<T> {
  controller.invocations.push({ command, args: args ? clone(args) : undefined })

  switch (command) {
    case 'list_connections':
      return clone(controller.connections) as T
    case 'create_connection': {
      const request = args?.request as CreateConnectionRequest | undefined
      if (!request) throw new Error('Mock create_connection requires request')
      const connection = connectionFromRequest(request)
      controller.connections.push(connection)
      return clone(connection) as T
    }
    case 'delete_connection': {
      const id = String(args?.id ?? '')
      const index = controller.connections.findIndex((item) => item.id === id)
      if (index >= 0) controller.connections.splice(index, 1)
      return undefined as T
    }
    case 'connect_master':
      findConnection(args).state = 'Connected'
      return undefined as T
    case 'disconnect_master':
      findConnection(args).state = 'Disconnected'
      return undefined as T
    case 'get_received_data_since':
      return { seq: 0, total_count: 0, points: [] } as T
    case 'get_communication_logs':
      return [] as T
    case 'check_for_update':
      return null as T
    case 'set_logging_enabled':
    case 'clear_communication_logs':
    case 'save_logs_csv':
    case 'skip_update':
    case 'schedule_update_on_next_launch':
    case 'install_update':
      return undefined as T
    default:
      throw new Error(`Tauri mock has no handler for command: ${command}`)
  }
}
