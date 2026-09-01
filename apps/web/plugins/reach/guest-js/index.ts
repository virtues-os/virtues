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
  /** false = ready to set up, true = already claimed, null/undefined = box too old to say. */
  claimed?: boolean | null
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

// ─── Improv BLE setup (the primary path) ─────────────────────────────────────

export interface ImprovBox {
  /** Opaque handle for the calls below. Valid until the next discover. */
  id: string
  name: string
  /** 0x02 = needs wifi, 0x04 = already online (advertisement's state byte). */
  improvState: number
  rssi: number
}

/** Scan for unclaimed boxes advertising the Improv service. */
export async function improvDiscover(seconds = 4): Promise<ImprovBox[]> {
  const r = await invoke<{ boxes: ImprovBox[] }>('plugin:reach|improv_discover', { seconds })
  return r.boxes
}

export interface ImprovNetwork {
  ssid: string
  signal: number
  secured: boolean
}

/**
 * Open a session with an unclaimed box by typing the four words its panel
 * shows (RPC 0x86). `gated: false` means old firmware with no phrase gate —
 * the session is simply open.
 */
export async function improvClaim(
  id: string,
  phrase: string,
  label?: string,
): Promise<{ ok: boolean; gated?: boolean; error?: string }> {
  return await invoke('plugin:reach|improv_claim', { id, phrase, label })
}

/** Ask THAT BOX what wifi it can see, over BLE. */
export async function improvWifiScan(
  id: string,
): Promise<{ networks: ImprovNetwork[]; error?: string }> {
  return await invoke('plugin:reach|improv_wifi_scan', { id })
}

/**
 * Send credentials and watch the join live. Resolves with the box's URL on
 * success. Progress events arrive as `improv-progress` on the plugin channel.
 */
export async function improvProvision(
  id: string,
  ssid: string,
  password: string,
  /** 802.1X username; present routes the join over the 0x81 enterprise extension. */
  identity?: string,
): Promise<{ ok: boolean; url?: string; error?: string }> {
  return await invoke('plugin:reach|improv_provision', { id, ssid, password, identity })
}

/**
 * Pair over BLE (RPC 0x83), for LANs that block peer-to-peer.
 *
 * Takes no code: 0x83 became codeless and session-authorized on 2026-08-24,
 * and the Rust command mints the identity and label itself — passing a code
 * here was passing a field nothing read. Key custody stays in Rust on both
 * platforms, so this wrapper only names the box.
 */
export async function improvPair(
  id: string,
): Promise<{ ok: boolean; response?: string; error?: string }> {
  return await invoke('plugin:reach|improv_pair', { id })
}

/** Drop the BLE connection when leaving setup. Always safe. */
export async function improvDisconnect(): Promise<void> {
  await invoke('plugin:reach|improv_disconnect')
}
