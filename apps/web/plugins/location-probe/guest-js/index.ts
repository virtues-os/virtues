import { invoke } from '@tauri-apps/api/core'

export interface ProbeRow {
  ts: string
  lat: number
  lon: number
  source: string
  appState: string
  launchReason: string
}

/** Install the CLLocationManager delegate + start significant-location-change. */
export async function startProbe(): Promise<boolean> {
  const r = await invoke<{ started: boolean }>('plugin:location-probe|start_probe')
  return r.started
}

/** Read the rows the native background code has written to SQLite. */
export async function readRows(limit = 200): Promise<ProbeRow[]> {
  const r = await invoke<{ rows: ProbeRow[] }>('plugin:location-probe|read_rows', {
    payload: { limit },
  })
  return r.rows
}
