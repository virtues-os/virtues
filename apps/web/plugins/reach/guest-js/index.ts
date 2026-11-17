import { invoke } from '@tauri-apps/api/core'

export interface ReachStatus {
  paired: boolean
  session: 'authed' | 'rejected' | 'unknown' | 'unpaired'
  loopbackUrl: string
  /** Why this device cannot use the pairing it has, in words for the owner.
   *  `paired` stays true while it is set. Absent before surface 15. */
  problem?: string | null
  /** The device key is gone, so pairing again is the only fix. */
  keyMissing?: boolean
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
  /** false = ready to set up, true = already claimed, null/undefined = box too old to say. */
  claimed?: boolean | null
}

/** Browse the LAN (Bonjour) for Virtues boxes to auto-fill the address. */
export async function discover(): Promise<FoundServer[]> {
  const r = await invoke<{ servers: FoundServer[] }>('plugin:reach|discover')
  return r.servers
}

/**
 * Sign one HTTP request for the server with this device's paired key: the
 * owner proof the server asks for before revealing a password or a key,
 * adding a device or changing the AI key.
 * `method` and `path` are exactly what the request carries; `nonce` comes from
 * `POST /api/owner-proof/nonce`. Returns the `X-Virtues-Owner-Proof` header
 * value. The key stays in Rust.
 */
export async function ownerProofSign(method: string, path: string, nonce: string): Promise<string> {
  return await invoke('plugin:reach|owner_proof_sign', { method, path, nonce })
}
