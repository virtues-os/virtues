import { invoke } from '@tauri-apps/api/core'

export interface ReachStatus {
  paired: boolean
  session: 'authed' | 'rejected' | 'unknown' | 'unpaired'
  loopbackUrl: string
}

/** Pair with a box by LAN address + 6-digit code. */
export async function pair(server: string, code: string): Promise<ReachStatus> {
  return await invoke<ReachStatus>('plugin:reach|pair', { payload: { server, code } })
}

/** Current reach state (paired? session authed/rejected/unknown?). */
export async function reachStatus(): Promise<ReachStatus> {
  return await invoke<ReachStatus>('plugin:reach|reach_status')
}

/** Clear local pairing creds. */
export async function forget(): Promise<void> {
  await invoke('plugin:reach|forget')
}

export interface FoundServer {
  name: string
  origin: string
}

/** Browse the LAN (Bonjour) for Virtues boxes to auto-fill the address. */
export async function discover(): Promise<FoundServer[]> {
  const r = await invoke<{ servers: FoundServer[] }>('plugin:reach|discover')
  return r.servers
}
