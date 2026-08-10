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

// ─── Wifi provisioning over an appliance's setup AP ──────────────────────────
//
// Used by the connect screen when the phone is joined to `Virtues-XXXX` and the
// box on the other end has never been claimed. See
// `virtues_reach_client::provision` for the doctrine; the short version is that
// the owner's home wifi password belongs in a native field rather than a
// captive-portal webview, and the app can hold the setup session across the
// network handoff that provisioning causes.

export interface ProvisionNetwork {
  ssid: string
  /** 0–100, as NetworkManager reports it. */
  signal: number
  /** False for an open network — skip the password field. */
  secured: boolean
}

/**
 * Is this box unclaimed AND are we on its setup network?
 *
 * One question, because the box's own gates answer both at once: the
 * provisioning routes 404 for anyone off the AP subnet or after a device has
 * paired. Cheap enough to ask of every discovered box.
 */
export async function provisionOpen(server: string): Promise<boolean> {
  return await invoke<boolean>('plugin:reach|provision_open', { server })
}

/** Networks the BOX can see — not the phone's list. */
export async function provisionNetworks(server: string): Promise<ProvisionNetwork[]> {
  return await invoke<ProvisionNetwork[]>('plugin:reach|provision_networks', { server })
}

export interface ProvisionJoinResult {
  outcome: 'joined' | 'failed' | 'unknown'
  detail: string | null
}

/**
 * Put the box on the owner's network.
 *
 * **`unknown` is the expected outcome, not an edge case.** The box drops its AP
 * as the first step of the join, so this request usually dies mid-flight — on
 * the success path as often as the failure path. Never render it as an error;
 * go and look for the box instead.
 */
export async function provisionJoin(
  server: string,
  ssid: string,
  psk?: string,
): Promise<ProvisionJoinResult> {
  return await invoke<ProvisionJoinResult>('plugin:reach|provision_join', { server, ssid, psk })
}
