/**
 * API Client for Rust Core Library
 *
 * All calls to /api/* are proxied to Rust backend (localhost:8000)
 * via Vite proxy (see vite.config.ts).
 */

import { sanitizeUrl } from '$lib/utils/urlUtils';

const API_BASE = '/api';

// ============================================================================
// Shared request layer
// ============================================================================

/**
 * Error thrown by the shared `request()` helper when a response is not ok.
 * Carries the HTTP `status` so callers can branch on it (e.g. 402 = wallet
 * expired / subscription lapsed, 401 = unknown key) instead of parsing a
 * stringified statusText. `body` holds the parsed JSON error payload when the
 * server returned one.
 */
export class ApiError extends Error {
	readonly status: number;
	readonly body: unknown;
	constructor(status: number, message: string, body?: unknown) {
		super(message);
		this.name = 'ApiError';
		this.status = status;
		this.body = body;
	}
}

type QueryValue = string | number | boolean | null | undefined;

/**
 * Core fetch wrapper for JSON endpoints under `/api`. Serializes an optional
 * query object, throws {@link ApiError} (with status) on non-2xx, and returns
 * the parsed JSON body (or `undefined` for empty/204 responses).
 *
 * Not for binary/blob endpoints or progress-tracked uploads — those keep their
 * own `fetch`/XHR (see uploadMedia, downloadDriveFile).
 */
export async function request<T>(
	path: string,
	init?: RequestInit & { query?: Record<string, QueryValue> },
): Promise<T> {
	let url = `${API_BASE}${path}`;
	if (init?.query) {
		const qs = new URLSearchParams();
		for (const [key, value] of Object.entries(init.query)) {
			if (value !== undefined && value !== null) qs.set(key, String(value));
		}
		const q = qs.toString();
		if (q) url += `?${q}`;
	}

	const res = await fetch(url, init);

	if (!res.ok) {
		let body: unknown;
		let message = res.statusText || `HTTP ${res.status}`;
		try {
			const text = await res.text();
			if (text) {
				try {
					body = JSON.parse(text);
					const parsed = body as { error?: string; message?: string };
					message = parsed?.error || parsed?.message || message;
				} catch {
					body = text;
					message = text;
				}
			}
		} catch {
			/* keep statusText fallback */
		}
		throw new ApiError(res.status, message, body);
	}

	if (res.status === 204) return undefined as T;
	const text = await res.text();
	return (text ? JSON.parse(text) : undefined) as T;
}

/** GET a JSON endpoint, with an optional query object. */
export function apiGet<T>(path: string, query?: Record<string, QueryValue>): Promise<T> {
	return request<T>(path, { query });
}

/** Send a JSON body (POST/PUT/PATCH/DELETE) and parse the JSON response. */
export function apiSend<T>(method: string, path: string, jsonBody?: unknown): Promise<T> {
	return request<T>(path, {
		method,
		headers: jsonBody !== undefined ? { 'Content-Type': 'application/json' } : undefined,
		body: jsonBody !== undefined ? JSON.stringify(jsonBody) : undefined,
	});
}

// ============================================================================
// Actions — new schema (post cutover + PR 2 endpoints)
// ============================================================================

export type ActionTrigger = 'cron' | 'manual' | 'tool' | 'api' | 'webhook';

export interface ActionRun {
	id: string;
	action_id: string | null;
	status: 'running' | 'success' | 'error' | 'cancelled' | 'skipped';
	started_at: string;
	completed_at: string | null;
	records_processed: number;
	error: string | null;
	trigger: ActionTrigger;
	parent_run_id: string | null;
	transform_stage: string | null;
	result_summary: string | null;
	created_at: string;
}

export interface ActionLastRun {
	status: string;
	started_at: string | null;
	completed_at?: string | null;
	records_processed: number | null;
	error: string | null;
	summary?: string | null;
}

/**
 * Three-runtime model — see ARCHITECTURE.md.
 *
 * - `function`: fork-per-trigger CLI; the default. Server forks the binary
 *   on every trigger, pipes ActionInput/Output JSON.
 * - `app`: long-running supervised HTTP server. Core proxies `/service/<id>/*`
 *   to it; cron/webhook triggers become `POST /__trigger`.
 * - `view`: pure Svelte component, never invoked server-side. Lives at
 *   `apps/web/src/lib/applets/<name>/`.
 */
export type ActionRuntime = 'function' | 'service' | 'view';

export interface Action {
	id: string;
	owner: 'system' | 'user' | 'ai';
	name: string;
	agent: string | null;
	cron_schedule: string | null;
	enabled: boolean;
	config: Record<string, unknown>;
	condition: string | null;
	triggers: ActionTrigger[];
	memory: string | null;
	function_name: string | null;
	credential_id: string | null;
	runtime: ActionRuntime;
	/** Polyglot escape: explicit argv to spawn instead of resolving a Cargo
	 *  binary by `function_name`. Null when the action uses the function_name
	 *  shortcut. */
	command: string[] | null;
	/** Lifecycle: null = forever · "once" = archive after first success ·
	 *  anything else = SQL boolean checked after each success. */
	until: string | null;
	/** Set when the lifecycle completed; archived applets are disabled. */
	archived_at: string | null;
	/** Command applets that run as a long-lived supervised service. */
	supervise: boolean;
	/** True when the applet folder ships a face/ (sandboxed-iframe HTML UI). */
	has_face: boolean;
	is_system: boolean;
	created_at: string;
	updated_at: string;
	last_run: ActionLastRun | null;
}

export interface ActionDetail extends Action {
	recent_runs?: ActionRun[];
}

export async function mintFaceToken(
	actionId: string
): Promise<{ token: string; expires_in_seconds: number }> {
	return request(`/applets/${encodeURIComponent(actionId)}/face-token`);
}

export async function listActions(): Promise<Action[]> {
	const res = await fetch(`${API_BASE}/applets`);
	if (!res.ok) throw new Error(`Failed to list actions: ${res.statusText}`);
	return res.json();
}

export async function getAction(id: string): Promise<Action> {
	const res = await fetch(`${API_BASE}/applets/${encodeURIComponent(id)}`);
	if (!res.ok) throw new Error(`Failed to get action: ${res.statusText}`);
	return res.json();
}

// ─────────────────────────────────────────────────────────────────────────────
// Sidebar pins
// ─────────────────────────────────────────────────────────────────────────────

export interface Pin {
	id: string;
	url: string;
	label: string | null;
	icon: string | null;
	sort_order: number;
	pinned_at: string;
}

export async function listPins(): Promise<Pin[]> {
	const res = await fetch(`${API_BASE}/pins`);
	if (!res.ok) throw new Error(`Failed to list pins: ${res.statusText}`);
	return res.json();
}

export async function createPin(req: {
	url: string;
	label?: string | null;
	icon?: string | null;
}): Promise<Pin> {
	const res = await fetch(`${API_BASE}/pins`, {
		method: 'POST',
		headers: { 'content-type': 'application/json' },
		body: JSON.stringify(req)
	});
	if (!res.ok) throw new Error(`Failed to pin: ${res.statusText}`);
	return res.json();
}

export async function updatePin(
	id: string,
	req: { label?: string | null; icon?: string | null; sort_order?: number }
): Promise<Pin> {
	const res = await fetch(`${API_BASE}/pins/${encodeURIComponent(id)}`, {
		method: 'PATCH',
		headers: { 'content-type': 'application/json' },
		body: JSON.stringify(req)
	});
	if (!res.ok) throw new Error(`Failed to update pin: ${res.statusText}`);
	return res.json();
}

export async function deletePin(id: string): Promise<void> {
	const res = await fetch(`${API_BASE}/pins/${encodeURIComponent(id)}`, { method: 'DELETE' });
	if (!res.ok) throw new Error(`Failed to delete pin: ${res.statusText}`);
}

export async function reorderPins(urls: string[]): Promise<void> {
	const res = await fetch(`${API_BASE}/pins/reorder`, {
		method: 'PUT',
		headers: { 'content-type': 'application/json' },
		body: JSON.stringify({ urls })
	});
	if (!res.ok) throw new Error(`Failed to reorder pins: ${res.statusText}`);
}

// ─────────────────────────────────────────────────────────────────────────────
// System (operator surface for app-runtime actions — `/actions#system`)
// ─────────────────────────────────────────────────────────────────────────────

export type AppStatus = 'Starting' | 'Running' | 'Backoff' | 'Crashed' | 'Stopping';

export interface RunningApp {
	action_id: string;
	port: number;
	pid: number | null;
	status: AppStatus;
	started_at: string | null;
	restart_count: number;
}

export type LogStream = 'stdout' | 'stderr';

export interface LogLine {
	stream: LogStream;
	line: string;
	at: string;
}

/** GET /api/system/apps — snapshot of supervised app-runtime children. */
export async function listSystemApps(): Promise<RunningApp[]> {
	const res = await fetch(`${API_BASE}/system/apps`);
	if (!res.ok) throw new Error(`Failed to list system apps: ${res.statusText}`);
	return res.json();
}

/** GET /api/actions/:id/logs — captured stdout/stderr ring buffer for an app. */
export async function getActionLogs(id: string): Promise<LogLine[]> {
	const res = await fetch(`${API_BASE}/applets/${encodeURIComponent(id)}/logs`);
	if (!res.ok) throw new Error(`Failed to get action logs: ${res.statusText}`);
	return res.json();
}

/** POST /api/admin/reconcile — picks up new manifests + diffs running apps. */
export async function adminReconcile(): Promise<{
	upserted: number;
	added: string[];
	removed: string[];
	restarted: string[];
}> {
	const res = await fetch(`${API_BASE}/admin/reconcile`, { method: 'POST' });
	if (!res.ok) throw new Error(`Reconcile failed: ${res.statusText}`);
	return res.json();
}

/**
 * POST /api/admin/applets/import-git — clones a repo into `actions/<slug>/`
 * and runs the standard scanner. Any folder under the slug containing a
 * `manifest.toml` becomes an action. Returns added/updated/removed ids.
 */
export async function importActionsFromGit(body: {
	url: string;
	ref?: string;
}): Promise<{ added: string[]; updated: string[]; removed: string[] }> {
	const res = await fetch(`${API_BASE}/admin/applets/import-git`, {
		method: 'POST',
		headers: { 'Content-Type': 'application/json' },
		body: JSON.stringify(body)
	});
	if (!res.ok) {
		const text = await res.text().catch(() => res.statusText);
		throw new Error(text || `Import failed: ${res.statusText}`);
	}
	return res.json();
}

export interface CreateActionRequest {
	name: string;
	agent?: string;
	cron_schedule?: string;
	triggers?: ActionTrigger[];
	config?: Record<string, unknown>;
}

export async function createAction(body: CreateActionRequest): Promise<Action> {
	const res = await fetch(`${API_BASE}/applets`, {
		method: 'POST',
		headers: { 'Content-Type': 'application/json' },
		body: JSON.stringify(body)
	});
	if (!res.ok) {
		const err = await res.json().catch(() => ({ error: res.statusText }));
		throw new Error(err.error || `Failed to create action: ${res.statusText}`);
	}
	return res.json();
}

export interface PatchActionBody {
	name?: string;
	agent?: string | null;
	cron_schedule?: string | null;
	enabled?: boolean;
	config?: Record<string, unknown>;
	condition?: string | null;
	triggers?: ActionTrigger[];
	memory?: string | null;
}

export async function patchAction(id: string, patch: PatchActionBody): Promise<Action> {
	const res = await fetch(`${API_BASE}/applets/${encodeURIComponent(id)}`, {
		method: 'PATCH',
		headers: { 'Content-Type': 'application/json' },
		body: JSON.stringify(patch)
	});
	if (!res.ok) {
		const err = await res.json().catch(() => ({ error: res.statusText }));
		throw new Error(err.error || `Failed to update action: ${res.statusText}`);
	}
	return res.json();
}

export async function deleteAction(id: string, dropData = false): Promise<void> {
	const q = dropData ? '?drop_data=true' : '';
	const res = await fetch(`${API_BASE}/applets/${encodeURIComponent(id)}${q}`, {
		method: 'DELETE'
	});
	if (!res.ok) {
		const err = await res.json().catch(() => ({ error: res.statusText }));
		throw new Error(err.error || `Failed to delete action: ${res.statusText}`);
	}
}

/** The private tables an applet owns — shown on the delete confirm so the user
 *  can choose whether to also drop its data. Empty when it owns none. */
export interface AppletData {
	schema: string | null;
	tables: string[];
}

export async function getAppletData(id: string): Promise<AppletData> {
	const res = await fetch(`${API_BASE}/applets/${encodeURIComponent(id)}/data`);
	if (!res.ok) return { schema: null, tables: [] };
	return (await res.json()) as AppletData;
}

export interface TriggerActionResponse {
	run_id: string | null;
	action_id: string;
	status: string;
	summary: string;
	error: string | null;
}

export async function runAction(
	id: string,
	payload?: Record<string, unknown>
): Promise<TriggerActionResponse> {
	const res = await fetch(`${API_BASE}/applets/${encodeURIComponent(id)}/run`, {
		method: 'POST',
		headers: { 'Content-Type': 'application/json' },
		body: JSON.stringify(payload ? { payload } : {})
	});
	if (!res.ok && res.status !== 500) {
		const err = await res.json().catch(() => ({ error: res.statusText }));
		throw new Error(err.error || `Failed to run action: ${res.statusText}`);
	}
	return res.json();
}

export async function listActionRuns(
	id: string,
	opts?: { limit?: number; status?: string }
): Promise<ActionRun[]> {
	const params = new URLSearchParams();
	if (opts?.limit != null) params.set('limit', String(opts.limit));
	if (opts?.status) params.set('status', opts.status);
	const qs = params.toString();
	const res = await fetch(
		`${API_BASE}/applets/${encodeURIComponent(id)}/runs${qs ? `?${qs}` : ''}`
	);
	if (!res.ok) throw new Error(`Failed to list runs: ${res.statusText}`);
	return res.json();
}

export async function listRuns(opts?: {
	limit?: number;
	status?: string;
	action_id?: string;
}): Promise<ActionRun[]> {
	const params = new URLSearchParams();
	if (opts?.limit != null) params.set('limit', String(opts.limit));
	if (opts?.status) params.set('status', opts.status);
	if (opts?.action_id) params.set('action_id', opts.action_id);
	const qs = params.toString();
	const res = await fetch(`${API_BASE}/runs${qs ? `?${qs}` : ''}`);
	if (!res.ok) throw new Error(`Failed to list runs: ${res.statusText}`);
	return res.json();
}

/** Get a single action run by ID (used for polling sync job status) */
export async function getJobStatus(
	jobId: string
): Promise<{ id: string; status: string; records_processed: number; error: string | null }> {
	const res = await fetch(`${API_BASE}/applets/runs/${jobId}`);
	if (!res.ok) throw new Error(`Failed to get run status: ${res.statusText}`);
	return res.json();
}

// ============================================================================
// Credentials
// ============================================================================

export interface DeviceInfo {
	device_id: string;
	device_name: string;
	device_model: string;
	os_version: string;
	app_version: string | null;
}

export type CredentialStatus = 'pending' | 'active' | 'revoked';

export interface Credential {
	id: string;
	provider: string;
	name: string;
	auth_type: string;
	status: CredentialStatus;
	is_active: boolean;
	device_info: DeviceInfo | null;
	last_seen_at: string | null;
	created_at: string;
	action_count: number;
	/** Tier-2 init-sync lifecycle for active credentials:
	 *  'connected' → 'backfilling' → 'live'. Absent for pending/revoked. */
	sync_state?: 'connected' | 'backfilling' | 'live';
}

export async function listCredentials(): Promise<Credential[]> {
	const res = await fetch(`${API_BASE}/credentials`);
	if (!res.ok) throw new Error(`Failed to list credentials: ${res.statusText}`);
	return res.json();
}

export async function renameCredential(id: string, name: string): Promise<void> {
	const res = await fetch(`${API_BASE}/credentials/${encodeURIComponent(id)}`, {
		method: 'PATCH',
		headers: { 'Content-Type': 'application/json' },
		body: JSON.stringify({ name })
	});
	if (!res.ok) {
		const err = await res.json().catch(() => ({ error: res.statusText }));
		throw new Error(err.error || `Failed to rename credential: ${res.statusText}`);
	}
}

export async function revokeCredential(id: string): Promise<void> {
	const res = await fetch(`${API_BASE}/credentials/${encodeURIComponent(id)}`, {
		method: 'DELETE'
	});
	if (!res.ok) {
		const err = await res.json().catch(() => ({ error: res.statusText }));
		throw new Error(err.error || `Failed to revoke credential: ${res.statusText}`);
	}
}

// Source catalog
//
// One tile per [[source]] in actions/templates.toml. Drives the Sources tab grid.
// `auth_kind` tells the UI which connect flow to dispatch on click.
export type SourceAuthKind = 'self_issued_bearer' | 'via_proxy' | 'api_key';


export interface SourceCatalogItem {
	id: string;
	name: string;
	icon: string | null;
	description: string | null;
	auth_kind: SourceAuthKind;
	credential_count: number;
}

/**
 * Fetch the source catalog (one tile per `[[source]]` in templates.toml).
 */
export async function listSourceCatalog(): Promise<SourceCatalogItem[]> {
	const res = await fetch(`${API_BASE}/sources`);
	if (!res.ok) throw new Error(`Failed to list sources: ${res.statusText}`);
	return res.json();
}


// ─────────────────────────────────────────────────────────────────────────────
// Source-connect flows (drive the 5 thin handlers in virtues-core/src/api/source_auth.rs)
// ─────────────────────────────────────────────────────────────────────────────

// ─── Unified pairing (`/api/pair/*`) ─────────────────────────────────────────
// One token mechanism for every device: the owner's authenticated session mints
// a token, the new device redeems it at `/api/pair/consume`. The phone scans the
// QR (`/pair#t=<token>`); the Mac app / collector takes the token directly.

export interface PairMintResponse {
	id: string;
	token: string;
	pair_url: string;
	qr_svg: string;
	expires_at: string;
}

/** POST /api/pair/mint — auth'd. Mint a `pending` token to add a device. */
export async function pairMint(intendedKind?: string): Promise<PairMintResponse> {
	const res = await fetch(`${API_BASE}/pair/mint`, {
		method: 'POST',
		headers: { 'Content-Type': 'application/json' },
		body: JSON.stringify({ intended_kind: intendedKind ?? null })
	});
	if (!res.ok) {
		const err = await res.json().catch(() => ({ error: res.statusText }));
		throw new Error(err.error || `pair_mint failed: ${res.statusText}`);
	}
	return res.json();
}

/** POST /api/pair/deny/:id — auth'd. Cancel an outstanding token (e.g. modal close). */
export async function pairDeny(id: string): Promise<void> {
	await fetch(`${API_BASE}/pair/deny/${encodeURIComponent(id)}`, { method: 'POST' }).catch(
		() => {
			/* benign — token may have already been consumed/expired */
		}
	);
}

export interface PairStatusResponse {
	status: string; // pending | authorized | consumed | denied | expired
	consumed_by_device: string | null;
	consumed_by_label: string | null;
}

/** GET /api/pair/status/:id — auth'd. Poll for the new device redeeming. */
export async function pairStatus(id: string): Promise<PairStatusResponse> {
	const res = await fetch(`${API_BASE}/pair/status/${encodeURIComponent(id)}`);
	if (!res.ok) throw new Error(`pair_status failed: ${res.statusText}`);
	return res.json();
}

/**
 * DELETE /api/devices/:id — auth'd. Revoke a device (soft-delete + credential
 * teardown).
 */
export async function deleteDevice(id: string): Promise<void> {
	await fetch(`${API_BASE}/devices/${encodeURIComponent(id)}`, { method: 'DELETE' }).catch(() => {
		/* benign — device may already be revoked */
	});
}

export interface ChatImportResponse {
	status: string;
	summary: string;
	run_id: string | null;
}

/**
 * POST /api/chat-import/upload — multipart upload of a chat export (Tier 3
 * one-time import). Parsed + ingested box-side; returns the "Imported N
 * messages" summary once the run completes.
 */
export async function uploadChatImport(
	file: File,
	provider: string
): Promise<ChatImportResponse> {
	const form = new FormData();
	form.append('provider', provider);
	form.append('file', file);
	const res = await fetch(`${API_BASE}/chat-import/upload`, {
		method: 'POST',
		body: form
	});
	if (!res.ok) {
		const err = await res.json().catch(() => ({ error: res.statusText }));
		throw new Error(err.error || `chat_import upload failed: ${res.statusText}`);
	}
	return res.json();
}

export interface MintCollectorResponse {
	token: string;
	expires_at: string;
}

/**
 * POST /api/pair/mint-collector — auth'd. Mint + self-authorize a token for
 * installing the local collector on THIS machine (handed to
 * `installCollector(token)` via the Tauri bridge).
 */
export async function mintCollectorToken(): Promise<MintCollectorResponse> {
	const res = await fetch(`${API_BASE}/pair/mint-collector`, { method: 'POST' });
	if (!res.ok) {
		const err = await res.json().catch(() => ({ error: res.statusText }));
		throw new Error(err.error || `mint_collector failed: ${res.statusText}`);
	}
	return res.json();
}

export interface OauthStartResponse {
	redirect_url: string;
}

/** POST /api/connect/:source_id/start — sign state, return proxy redirect URL. */
export async function oauthStart(
	source_id: string,
	opts: { existing_credential_id?: string; return_url?: string } = {}
): Promise<OauthStartResponse> {
	const res = await fetch(
		`${API_BASE}/connect/${encodeURIComponent(source_id)}/start`,
		{
			method: 'POST',
			headers: { 'Content-Type': 'application/json' },
			body: JSON.stringify(opts)
		}
	);
	if (!res.ok) {
		const err = await res.json().catch(() => ({ error: res.statusText }));
		throw new Error(err.error || `oauth_start failed: ${res.statusText}`);
	}
	return res.json();
}

export interface ApiKeyCompleteResponse {
	credential_id: string;
}

/** POST /api/connect/:source_id/complete — encrypt + store a pasted token. */
export async function apikeyComplete(
	source_id: string,
	name: string,
	fields: Record<string, string>
): Promise<ApiKeyCompleteResponse> {
	const res = await fetch(
		`${API_BASE}/connect/${encodeURIComponent(source_id)}/complete`,
		{
			method: 'POST',
			headers: { 'Content-Type': 'application/json' },
			body: JSON.stringify({ name, fields })
		}
	);
	if (!res.ok) {
		const err = await res.json().catch(() => ({ error: res.statusText }));
		throw new Error(err.error || `apikey_complete failed: ${res.statusText}`);
	}
	return res.json();
}

// Device Pairing
import type {
	PairingInitResponse,
	PairingStatus,
	PendingPairing
} from '$lib/types/device-pairing';

/**
 * Initiate device pairing via the unified `/api/pair/mint` flow.
 *
 * Returns a `PairingInitResponse` whose `source_id` is the pair-token id (polled
 * via {@link getPairingStatus}), plus the QR/token redemption payload the new
 * device scans (`qr_svg` encodes `/pair#t=<token>`) or types (`token`).
 *
 * @param deviceType - "ios" → `mobile_app`, otherwise `desktop_app`.
 * @param _name - Display name (the device labels itself at consume time).
 */
export async function initiatePairing(
	deviceType: string,
	_name: string
): Promise<PairingInitResponse> {
	const intendedKind = deviceType === 'ios' ? 'mobile_app' : 'desktop_app';
	const minted = await pairMint(intendedKind);
	return {
		source_id: minted.id,
		token: minted.token,
		qr_svg: minted.qr_svg,
		pair_url: minted.pair_url
	};
}

/**
 * Poll pairing status by the token id. Maps the unified token lifecycle onto
 * the modal's `pending | active | revoked` shape: `consumed` → active (the new
 * device redeemed), `denied`/`expired` → revoked, everything else → pending.
 */
export async function getPairingStatus(sourceId: string): Promise<PairingStatus> {
	const s = await pairStatus(sourceId);
	if (s.status === 'consumed') {
		return {
			status: 'active',
			device_info: {
				device_id: s.consumed_by_device ?? '',
				device_name: s.consumed_by_label ?? '',
				device_model: '',
				os_version: ''
			}
		};
	}
	if (s.status === 'denied' || s.status === 'expired') {
		return { status: 'revoked' };
	}
	return { status: 'pending' };
}

/**
 * List all pending device pairings (not yet completed)
 * @returns Array of pending pairings with codes and expiration times
 */
export async function listPendingPairings(): Promise<{ pairings: PendingPairing[] }> {
	const res = await fetch(`${API_BASE}/devices/pending-pairings`);

	if (!res.ok) {
		throw new Error(`Failed to list pending pairings: ${res.statusText}`);
	}

	return res.json();
}

// Profile
export interface Profile {
	preferred_name?: string | null;
	occupation?: string | null;
	employer?: string | null;
	theme?: string | null;
	update_check_hour?: number | null;
	home_timezone?: string | null;
	home_place_id?: string | null;
	home_city?: string | null;
	home_country?: string | null;
	onboarding_status?: string | null;
}

export async function getProfile(): Promise<Profile> {
	const res = await fetch(`${API_BASE}/profile`);
	if (!res.ok) throw new Error(`Failed to get profile: ${res.statusText}`);
	return res.json();
}

export async function updateProfile(profile: Partial<Profile>): Promise<Profile> {
	const res = await fetch(`${API_BASE}/profile`, {
		method: 'PUT',
		headers: { 'Content-Type': 'application/json' },
		body: JSON.stringify(profile)
	});
	if (!res.ok) throw new Error(`Failed to update profile: ${res.statusText}`);
	return res.json();
}

// =============================================================================
// Drive - Personal File Storage
// =============================================================================

export interface DriveFile {
	id: string;
	path: string;
	filename: string;
	mime_type: string | null;
	size_bytes: number;
	is_folder: boolean;
	parent_id: string | null;
	sha256_hash: string | null;
	deleted_at: string | null;
	created_at: string;
	updated_at: string;
}

export interface DriveUsage {
	/** Total bytes used (drive_bytes + data_lake_bytes) */
	total_bytes: number;
	/** User-uploaded files in /home/user/drive/ */
	drive_bytes: number;
	/** ELT archives in /home/user/data-lake/ */
	data_lake_bytes: number;
	/** Quota limit based on tier */
	quota_bytes: number;
	/** Number of user files */
	file_count: number;
	/** Number of user folders */
	folder_count: number;
	/** Usage percentage (total_bytes / quota_bytes * 100) */
	usage_percent: number;
	/** Tier name (standard, pro) */
	tier: string;
}

/**
 * Get drive storage usage and quota information
 */
export async function getDriveUsage(): Promise<DriveUsage> {
	const res = await fetch(`${API_BASE}/drive/usage`);
	if (!res.ok) throw new Error(`Failed to get drive usage: ${res.statusText}`);
	return res.json();
}

/**
 * List files in a directory
 * @param path - Directory path (empty string for root)
 */
export async function listDriveFiles(path: string = ''): Promise<DriveFile[]> {
	const params = new URLSearchParams();
	if (path) params.set('path', path);

	const res = await fetch(`${API_BASE}/drive/files?${params}`);
	if (!res.ok) {
		const error = await res.json().catch(() => ({ error: res.statusText }));
		throw new Error(error.error || `Failed to list files: ${res.statusText}`);
	}
	return res.json();
}

/**
 * Get file metadata by ID
 */
export async function getDriveFile(fileId: string): Promise<DriveFile> {
	const res = await fetch(`${API_BASE}/drive/files/${fileId}`);
	if (!res.ok) {
		const error = await res.json().catch(() => ({ error: res.statusText }));
		throw new Error(error.error || `Failed to get file: ${res.statusText}`);
	}
	return res.json();
}

/** One raw life-graph record (the data viewer / citation target). */
export interface OntologyRecord {
	ontology: string;
	record_id: string;
	display_name: string;
	table_name: string;
	timestamp_column: string;
	/** The full row as a plain object (all columns). */
	row: Record<string, unknown>;
}

/** Fetch a single raw record by ontology + id — backs the data viewer. */
export async function getRecord(ontology: string, recordId: string): Promise<OntologyRecord> {
	const res = await fetch(
		`${API_BASE}/records/${encodeURIComponent(ontology)}/${encodeURIComponent(recordId)}`
	);
	if (!res.ok) {
		const error = await res.json().catch(() => ({ error: res.statusText }));
		throw new Error(error.error || `Failed to get record: ${res.statusText}`);
	}
	return res.json();
}

/**
 * Upload a file to drive
 * @param path - Target folder path
 * @param file - File to upload
 * @param onProgress - Optional progress callback (0-100)
 */
export async function uploadDriveFile(
	path: string,
	file: File,
	onProgress?: (percent: number) => void
): Promise<DriveFile> {
	const formData = new FormData();
	formData.append('file', file);
	formData.append('path', path);
	formData.append('filename', file.name);

	// Use XMLHttpRequest for progress tracking
	return new Promise((resolve, reject) => {
		const xhr = new XMLHttpRequest();
		xhr.open('POST', `${API_BASE}/drive/upload`);

		xhr.upload.onprogress = (e) => {
			if (e.lengthComputable && onProgress) {
				onProgress(Math.round((e.loaded / e.total) * 100));
			}
		};

		xhr.onload = () => {
			if (xhr.status >= 200 && xhr.status < 300) {
				resolve(JSON.parse(xhr.responseText));
			} else {
				try {
					const error = JSON.parse(xhr.responseText);
					reject(new Error(error.error || `Upload failed: ${xhr.statusText}`));
				} catch {
					reject(new Error(`Upload failed: ${xhr.statusText}`));
				}
			}
		};

		xhr.onerror = () => reject(new Error('Upload failed: network error'));
		xhr.send(formData);
	});
}

/**
 * Download a file from drive
 */
export async function downloadDriveFile(fileId: string): Promise<{ file: DriveFile; blob: Blob }> {
	const res = await fetch(`${API_BASE}/drive/files/${fileId}/download`);
	if (!res.ok) {
		const error = await res.json().catch(() => ({ error: res.statusText }));
		throw new Error(error.error || `Failed to download file: ${res.statusText}`);
	}

	// Get filename from Content-Disposition header
	const contentDisposition = res.headers.get('Content-Disposition');
	let filename = 'download';
	if (contentDisposition) {
		const match = contentDisposition.match(/filename="?([^";\n]+)"?/);
		if (match) filename = match[1];
	}

	const blob = await res.blob();
	return {
		file: { filename } as DriveFile,
		blob
	};
}

/**
 * Delete a file or folder
 */
export async function deleteDriveFile(fileId: string): Promise<void> {
	const res = await fetch(`${API_BASE}/drive/files/${fileId}`, { method: 'DELETE' });
	if (!res.ok) {
		const error = await res.json().catch(() => ({ error: res.statusText }));
		throw new Error(error.error || `Failed to delete file: ${res.statusText}`);
	}
}

/**
 * Create a folder
 */
export async function createDriveFolder(path: string, name: string): Promise<DriveFile> {
	const res = await fetch(`${API_BASE}/drive/folders`, {
		method: 'POST',
		headers: { 'Content-Type': 'application/json' },
		body: JSON.stringify({ path, name })
	});
	if (!res.ok) {
		const error = await res.json().catch(() => ({ error: res.statusText }));
		throw new Error(error.error || `Failed to create folder: ${res.statusText}`);
	}
	return res.json();
}

/**
 * Move or rename a file/folder
 */
export async function moveDriveFile(fileId: string, newPath: string): Promise<DriveFile> {
	const res = await fetch(`${API_BASE}/drive/files/${fileId}/move`, {
		method: 'PUT',
		headers: { 'Content-Type': 'application/json' },
		body: JSON.stringify({ new_path: newPath })
	});
	if (!res.ok) {
		const error = await res.json().catch(() => ({ error: res.statusText }));
		throw new Error(error.error || `Failed to move file: ${res.statusText}`);
	}
	return res.json();
}

/**
 * List files in trash
 */
export async function listDriveTrash(): Promise<DriveFile[]> {
	const res = await fetch(`${API_BASE}/drive/trash`);
	if (!res.ok) {
		const error = await res.json().catch(() => ({ error: res.statusText }));
		throw new Error(error.error || `Failed to list trash: ${res.statusText}`);
	}
	return res.json();
}

/**
 * Restore a file from trash
 */
export async function restoreDriveFile(fileId: string): Promise<DriveFile> {
	const res = await fetch(`${API_BASE}/drive/files/${fileId}/restore`, {
		method: 'POST'
	});
	if (!res.ok) {
		const error = await res.json().catch(() => ({ error: res.statusText }));
		throw new Error(error.error || `Failed to restore file: ${res.statusText}`);
	}
	return res.json();
}

/**
 * Permanently delete a file (skip trash)
 */
export async function purgeDriveFile(fileId: string): Promise<void> {
	const res = await fetch(`${API_BASE}/drive/files/${fileId}/purge`, {
		method: 'DELETE'
	});
	if (!res.ok) {
		const error = await res.json().catch(() => ({ error: res.statusText }));
		throw new Error(error.error || `Failed to permanently delete file: ${res.statusText}`);
	}
}

/**
 * Empty entire trash (permanently delete all trashed files)
 */
export async function emptyDriveTrash(): Promise<{ deleted_count: number }> {
	const res = await fetch(`${API_BASE}/drive/trash/empty`, {
		method: 'POST'
	});
	if (!res.ok) {
		const error = await res.json().catch(() => ({ error: res.statusText }));
		throw new Error(error.error || `Failed to empty trash: ${res.statusText}`);
	}
	return res.json();
}

// =============================================================================
// Media - Content-addressed storage for page-embedded media
// =============================================================================

export interface MediaFile {
	id: string;
	url: string;
	filename: string;
	mime_type: string | null;
	size_bytes: number;
	width: number | null;
	height: number | null;
	deduplicated: boolean;
}

/**
 * Upload a media file (image, video, audio) for embedding in pages.
 * Uses content-addressed storage - duplicate uploads return existing file.
 */
export async function uploadMedia(
	file: File,
	onProgress?: (percent: number) => void
): Promise<MediaFile> {
	const formData = new FormData();
	formData.append('file', file);
	formData.append('filename', file.name);

	// Use XMLHttpRequest for progress tracking
	return new Promise((resolve, reject) => {
		const xhr = new XMLHttpRequest();
		xhr.open('POST', `${API_BASE}/media/upload`);

		xhr.upload.onprogress = (e) => {
			if (e.lengthComputable && onProgress) {
				onProgress(Math.round((e.loaded / e.total) * 100));
			}
		};

		xhr.onload = () => {
			if (xhr.status >= 200 && xhr.status < 300) {
				resolve(JSON.parse(xhr.responseText));
			} else {
				try {
					const error = JSON.parse(xhr.responseText);
					reject(new Error(error.error || `Upload failed: ${xhr.statusText}`));
				} catch {
					reject(new Error(`Upload failed: ${xhr.statusText}`));
				}
			}
		};

		xhr.onerror = () => reject(new Error('Network error during upload'));
		xhr.send(formData);
	});
}

/**
 * Get media file metadata by ID
 */
export async function getMedia(fileId: string): Promise<MediaFile> {
	const res = await fetch(`${API_BASE}/media/${fileId}`);
	if (!res.ok) {
		const error = await res.json().catch(() => ({ error: res.statusText }));
		throw new Error(error.error || `Failed to get media: ${res.statusText}`);
	}
	return res.json();
}

// =============================================================================
// Chats - Chat Management
// =============================================================================

export interface ChatMessage {
	role: 'user' | 'assistant' | 'system';
	content: string;
	timestamp: string;
}

export interface CreateChatResponse {
	id: string;
	title: string;
	message_count: number;
	created_at: string;
}

/**
 * Create a new chat with initial messages
 * Used for intro chats and pre-populated conversations
 */
export async function createChat(
	title: string,
	messages: ChatMessage[]
): Promise<CreateChatResponse> {
	const res = await fetch(`${API_BASE}/chats`, {
		method: 'POST',
		headers: { 'Content-Type': 'application/json' },
		body: JSON.stringify({ title, messages })
	});

	if (!res.ok) {
		const error = await res.json().catch(() => ({ error: res.statusText }));
		throw new Error(error.error || `Failed to create chat: ${res.statusText}`);
	}

	return res.json();
}

/**
 * Update a chat (title and/or icon)
 */
export async function updateChat(
	chatId: string,
	updates: { title?: string; icon?: string | null; notebookId?: string | null }
): Promise<{ conversation_id: string; title: string; icon?: string | null; updated_at: string }> {
	const res = await fetch(`${API_BASE}/chats/${chatId}`, {
		method: 'PATCH',
		headers: { 'Content-Type': 'application/json' },
		body: JSON.stringify(updates)
	});

	if (!res.ok) {
		const error = await res.json().catch(() => ({ error: res.statusText }));
		throw new Error(error.error || `Failed to update chat: ${res.statusText}`);
	}

	return res.json();
}

/**
 * Delete a chat
 */
export async function deleteChat(chatId: string): Promise<{ deleted: boolean }> {
	const res = await fetch(`${API_BASE}/chats/${chatId}`, {
		method: 'DELETE'
	});

	if (!res.ok) {
		const error = await res.json().catch(() => ({ error: res.statusText }));
		throw new Error(error.error || `Failed to delete chat: ${res.statusText}`);
	}

	return res.json();
}

// =============================================================================
// Notebooks API — the "room" a chat lives in
//
// A Notebook is a manual collection the user returns to: a project, pet, hobby,
// goal, or topic. It gathers entities, chats, and pages as URL-native members
// and carries a single accent tint plus a catch-up memo (`current_status`).
// A chat lives in at most one Notebook (see `updateChat`'s `notebookId`).
// =============================================================================

/** Core Notebook row (no counts). Returned by create/update. */
export interface Notebook {
	id: string;
	name: string;
	icon: string | null;
	accent_color: string | null;
	current_status: string | null;
	current_status_at: string | null;
	instructions: string | null;
	sort_order: number;
	created_at: string;
	updated_at: string;
}

/** List-view summary — adds member and chat counts. */
export interface NotebookSummary extends Notebook {
	item_count: number;
	chat_count: number;
}

/** A single URL-native member of a Notebook. */
export interface NotebookItem {
	url: string;
	sort_order: number;
	added_at: string;
}

/** GET /api/notebooks/:id — a Notebook plus its ordered members. */
export interface NotebookDetail extends Notebook {
	items: NotebookItem[];
}

/** GET /api/notebooks — all Notebooks with counts. */
export async function listNotebooks(): Promise<{ notebooks: NotebookSummary[] }> {
	const res = await fetch(`${API_BASE}/notebooks`);
	if (!res.ok) throw new Error(`Failed to list notebooks: ${res.statusText}`);
	return res.json();
}

/** GET /api/notebooks/:id — a Notebook with its ordered members. */
export async function getNotebook(id: string): Promise<NotebookDetail> {
	const res = await fetch(`${API_BASE}/notebooks/${encodeURIComponent(id)}`);
	if (!res.ok) throw new Error(`Failed to get notebook: ${res.statusText}`);
	return res.json();
}

/** POST /api/notebooks — create a Notebook. */
export async function createNotebook(body: {
	name: string;
	icon?: string | null;
	accent_color?: string | null;
}): Promise<Notebook> {
	const res = await fetch(`${API_BASE}/notebooks`, {
		method: 'POST',
		headers: { 'Content-Type': 'application/json' },
		body: JSON.stringify(body)
	});
	if (!res.ok) throw new Error(`Failed to create notebook: ${res.statusText}`);
	return res.json();
}

/**
 * PUT /api/notebooks/:id — update a Notebook. For the nullable fields
 * (`icon`/`accent_color`/`current_status`): omit the key to leave unchanged,
 * send `null` to clear, send a value to set.
 */
export async function updateNotebook(
	id: string,
	patch: {
		name?: string;
		icon?: string | null;
		accent_color?: string | null;
		current_status?: string | null;
		instructions?: string | null;
		sort_order?: number;
	}
): Promise<Notebook> {
	const res = await fetch(`${API_BASE}/notebooks/${encodeURIComponent(id)}`, {
		method: 'PUT',
		headers: { 'Content-Type': 'application/json' },
		body: JSON.stringify(patch)
	});
	if (!res.ok) throw new Error(`Failed to update notebook: ${res.statusText}`);
	return res.json();
}

/** DELETE /api/notebooks/:id */
export async function deleteNotebook(id: string): Promise<void> {
	const res = await fetch(`${API_BASE}/notebooks/${encodeURIComponent(id)}`, { method: 'DELETE' });
	if (!res.ok) throw new Error(`Failed to delete notebook: ${res.statusText}`);
}

// =============================================================================
// View Entity (sidebar smart-section rows — resolved client-side from
// /api/chats and listPages; the folder/view CRUD API was removed.)
// =============================================================================

export interface ViewEntity {
	id: string;
	name: string;
	namespace: string;
	icon: string;
	updated_at?: string;
}

// =============================================================================
// Developer SQL API
// =============================================================================

export interface SqlResult {
	columns: string[];
	rows: Record<string, unknown>[];
	row_count: number;
}

/**
 * Execute a read-only SQL query via the developer endpoint
 */
export async function executeSql(sql: string): Promise<SqlResult> {
	const res = await fetch(`${API_BASE}/developer/sql`, {
		method: 'POST',
		headers: { 'Content-Type': 'application/json' },
		body: JSON.stringify({ query: sql })
	});
	if (!res.ok) {
		const error = await res.text();
		throw new Error(`SQL execution failed: ${error}`);
	}
	return res.json();
}

// =============================================================================
// Notebook Items API — the URL-native members of a Notebook
//
// (Listing comes back inside `getNotebook(id)` as `NotebookDetail.items`; there is
// no separate GET.)
// =============================================================================

/** POST /api/notebooks/:id/items — add a member URL to a Notebook. */
export async function addNotebookItem(notebookId: string, url: string): Promise<NotebookItem> {
	const sanitizedUrl = sanitizeUrl(url);
	const res = await fetch(`${API_BASE}/notebooks/${encodeURIComponent(notebookId)}/items`, {
		method: 'POST',
		headers: { 'Content-Type': 'application/json' },
		body: JSON.stringify({ url: sanitizedUrl })
	});
	if (!res.ok) throw new Error(`Failed to add notebook item: ${res.statusText}`);
	return res.json();
}

/** DELETE /api/notebooks/:id/items — remove a member URL from a Notebook. */
export async function removeNotebookItem(notebookId: string, url: string): Promise<void> {
	const res = await fetch(`${API_BASE}/notebooks/${encodeURIComponent(notebookId)}/items`, {
		method: 'DELETE',
		headers: { 'Content-Type': 'application/json' },
		body: JSON.stringify({ url })
	});
	if (!res.ok) throw new Error(`Failed to remove notebook item: ${res.statusText}`);
}

/** PUT /api/notebooks/:id/items/reorder — set the member order by URL. */
export async function reorderNotebookItems(notebookId: string, urls: string[]): Promise<void> {
	const res = await fetch(`${API_BASE}/notebooks/${encodeURIComponent(notebookId)}/items/reorder`, {
		method: 'PUT',
		headers: { 'Content-Type': 'application/json' },
		body: JSON.stringify({ urls })
	});
	if (!res.ok) throw new Error(`Failed to reorder notebook items: ${res.statusText}`);
}

// =============================================================================
// Pages API
// =============================================================================

export interface Page {
	id: string;
	title: string;
	content: string;
	notebook_id: string | null;
	icon: string | null;
	cover_url: string | null;
	tags: string | null; // JSON array string: '["tag1", "tag2"]'
	created_at: string;
	updated_at: string;
}

export interface PageSummary {
	id: string;
	title: string;
	notebook_id: string | null;
	icon: string | null;
	cover_url: string | null;
	tags: string | null; // JSON array string: '["tag1", "tag2"]'
	created_at: string;
	updated_at: string;
}

export interface PageListResponse {
	pages: PageSummary[];
	total: number;
	limit: number;
	offset: number;
}

export interface RefSearchResult {
	id: string;
	name: string;
	entity_type: string;
	icon: string;
}

export interface RefSearchResponse {
	results: RefSearchResult[];
}

/**
 * List all pages with optional pagination and workspace filter
 */
export async function listPages(limit?: number, offset?: number, notebook_id?: string): Promise<PageListResponse> {
	const params = new URLSearchParams();
	if (limit !== undefined) params.set('limit', String(limit));
	if (offset !== undefined) params.set('offset', String(offset));
	if (notebook_id !== undefined) params.set('notebook_id', notebook_id);

	const url = params.toString() ? `${API_BASE}/pages?${params}` : `${API_BASE}/pages`;
	const res = await fetch(url);

	if (!res.ok) throw new Error(`Failed to list pages: ${res.statusText}`);
	return res.json();
}

/**
 * Get a single page by ID
 */
export async function getPage(id: string): Promise<Page> {
	const res = await fetch(`${API_BASE}/pages/${id}`);
	if (!res.ok) throw new Error(`Failed to get page: ${res.statusText}`);
	return res.json();
}

/**
 * Create a new page
 */
export async function createPage(
	title: string,
	content: string = '',
	notebook_id: string | null = null,
	options?: { icon?: string; cover_url?: string; tags?: string }
): Promise<Page> {
	const res = await fetch(`${API_BASE}/pages`, {
		method: 'POST',
		headers: { 'Content-Type': 'application/json' },
		body: JSON.stringify({ title, content, notebookId: notebook_id, ...options })
	});

	if (!res.ok) throw new Error(`Failed to create page: ${res.statusText}`);
	return res.json();
}

/**
 * Update an existing page
 */
export async function updatePage(
	id: string,
	updates: {
		title?: string;
		content?: string;
		notebook_id?: string | null;
		icon?: string | null;
		cover_url?: string | null;
		tags?: string | null;
	}
): Promise<Page> {
	const res = await fetch(`${API_BASE}/pages/${id}`, {
		method: 'PUT',
		headers: { 'Content-Type': 'application/json' },
		body: JSON.stringify(updates)
	});

	if (!res.ok) throw new Error(`Failed to update page: ${res.statusText}`);
	return res.json();
}

/**
 * Delete a page by ID
 */
export async function deletePage(id: string): Promise<void> {
	const res = await fetch(`${API_BASE}/pages/${id}`, {
		method: 'DELETE'
	});

	if (!res.ok) throw new Error(`Failed to delete page: ${res.statusText}`);
}

/**
 * Search entities for autocomplete in the page editor
 * Used when typing [[ to link to entities
 */
export async function searchRefs(query: string): Promise<RefSearchResponse> {
	const res = await fetch(`${API_BASE}/pages/search/refs?q=${encodeURIComponent(query)}`);
	if (!res.ok) throw new Error(`Failed to search entities: ${res.statusText}`);
	return res.json();
}

// Page Sharing
export interface PageShare {
	id: string;
	page_id: string;
	token: string;
	created_at: string;
}

export interface SharedPage {
	title: string;
	content: string;
	icon: string | null;
	cover_url: string | null;
	share_token: string;
}

export async function createPageShare(pageId: string): Promise<PageShare> {
	const res = await fetch(`${API_BASE}/pages/${pageId}/share`, { method: 'POST' });
	if (!res.ok) throw new Error(`Failed to create share: ${res.statusText}`);
	return res.json();
}

export async function getPageShare(pageId: string): Promise<PageShare | null> {
	const res = await fetch(`${API_BASE}/pages/${pageId}/share`);
	if (!res.ok) throw new Error(`Failed to get share: ${res.statusText}`);
	return await res.json() ?? null;
}

export async function deletePageShare(pageId: string): Promise<void> {
	const res = await fetch(`${API_BASE}/pages/${pageId}/share`, { method: 'DELETE' });
	if (!res.ok) throw new Error(`Failed to delete share: ${res.statusText}`);
}

export async function getSharedPage(token: string): Promise<SharedPage> {
	const res = await fetch(`${API_BASE}/s/${token}`);
	if (!res.ok) throw new Error(`Page not found`);
	return res.json();
}

// ============================================================================
// Reflections API (pages linked to a day)
// ============================================================================

/** Get all reflections for a date. */
export async function getReflectionsForDate(date: string): Promise<Page[]> {
	const res = await fetch(`${API_BASE}/pages/reflections/${date}`);
	if (!res.ok) throw new Error(`Failed to get reflections: ${res.statusText}`);
	return res.json();
}

/** Create a new reflection page for a date. */
export async function createReflection(date: string): Promise<Page> {
	const res = await fetch(`${API_BASE}/pages/reflections/${date}`, { method: 'POST' });
	if (!res.ok) throw new Error(`Failed to create reflection: ${res.statusText}`);
	return res.json();
}

// ============================================================================
// Backlinks / References API
// ============================================================================

/** A page that links TO the queried page (an inbound reference). */
export interface Backlink {
	id: string;
	title: string;
	icon: string | null;
	/** One-line plain-text snippet of the surrounding context. */
	snippet: string;
	updated_at: string;
}

/** Get inbound references (pages that link to the given page). */
export async function getPageBacklinks(pageId: string): Promise<Backlink[]> {
	const res = await fetch(`${API_BASE}/pages/${pageId}/backlinks`);
	if (!res.ok) throw new Error(`Failed to get backlinks: ${res.statusText}`);
	const data = await res.json();
	return data.backlinks ?? [];
}

// ============================================================================
// Ontologies API
// ============================================================================

export interface OntologyColumnInfo {
	name: string;
	data_type: string;
	is_nullable: boolean;
}

export interface OntologyDataResponse {
	table_name: string;
	display_name: string;
	domain: string;
	columns: OntologyColumnInfo[];
	key_columns: string[];
	timestamp_column: string;
	rows: Record<string, unknown>[];
	total_count: number;
	limit: number;
	offset: number;
}

export interface OntologyOverview {
	name: string;
	domain: string;
	record_count: number;
	sample_record: Record<string, unknown> | null;
}

export async function listAvailableOntologies(): Promise<string[]> {
	const res = await fetch(`${API_BASE}/ontologies/available`);
	if (!res.ok) throw new Error(`Failed to list ontologies: ${res.statusText}`);
	return res.json();
}

export async function getOntologiesOverview(): Promise<OntologyOverview[]> {
	const res = await fetch(`${API_BASE}/ontologies/overview`);
	if (!res.ok) throw new Error(`Failed to get ontologies overview: ${res.statusText}`);
	return res.json();
}

export async function queryOntologyData(
	tableName: string,
	params?: {
		limit?: number;
		offset?: number;
		sort?: string;
		dir?: string;
		date?: string;
		search?: string;
	},
): Promise<OntologyDataResponse> {
	const searchParams = new URLSearchParams();
	if (params?.limit != null) searchParams.set('limit', String(params.limit));
	if (params?.offset != null) searchParams.set('offset', String(params.offset));
	if (params?.sort) searchParams.set('sort', params.sort);
	if (params?.dir) searchParams.set('dir', params.dir);
	if (params?.date) searchParams.set('date', params.date);
	if (params?.search) searchParams.set('search', params.search);

	const qs = searchParams.toString();
	const res = await fetch(`${API_BASE}/ontologies/${tableName}/data${qs ? `?${qs}` : ''}`);
	if (!res.ok) throw new Error(`Failed to query ontology data: ${res.statusText}`);
	return res.json();
}

// ============================================================================
// Setup state API
// ============================================================================

export interface SetupStep {
	id: string;
	title: string;
	done: boolean;
	/** Server-authored copy for the step's current state — render verbatim. */
	detail?: string;
	/**
	 * Cosmetic hint only (e.g. "ipv6_direct", "byo", or a network class).
	 * Behavior must key off `done`; unknown/missing kinds render like today.
	 */
	kind?: string;
}

export interface SetupState {
	setup: SetupStep[];
	setup_complete: boolean;
	onboarding: SetupStep[];
}

export async function getSetupState(): Promise<SetupState> {
	const res = await fetch(`${API_BASE}/setup/state`);
	if (!res.ok) throw new Error(`Failed to get setup state: ${res.statusText}`);
	return res.json();
}


// ============================================================================
// Tier 2 wrappers — routed through the shared request() layer (ApiError + status).
// Inputs are typed precisely; outputs use a generic passthrough (<T = unknown>)
// so each call site reuses the response type it already declares locally.
// ============================================================================

// ── Assistant profile ────────────────────────────────────────────────────────
export function getAssistantProfile<T = unknown>(): Promise<T> {
	return apiGet<T>('/assistant-profile');
}
export function updateAssistantProfile<T = unknown>(patch: Record<string, unknown>): Promise<T> {
	return apiSend<T>('PUT', '/assistant-profile', patch);
}

// ── Billing / wallet ─────────────────────────────────────────────────────────
export function getBillingLinkStatus<T = unknown>(): Promise<T> {
	return apiGet<T>('/billing/link/status');
}
export function startBillingLink<T = unknown>(): Promise<T> {
	return apiSend<T>('POST', '/billing/link/start');
}
export function openBillingPortal<T = unknown>(): Promise<T> {
	return apiSend<T>('POST', '/billing/portal');
}
export function getBillingState<T = unknown>(): Promise<T> {
	return apiGet<T>('/billing/state');
}
export function setBillingAutoTopup<T = unknown>(enabled: boolean): Promise<T> {
	return apiSend<T>('POST', '/billing/auto-topup', { enabled });
}
export function getBillingUsage<T = unknown>(): Promise<T> {
	return apiGet<T>('/billing/usage');
}
export function getByoKey<T = unknown>(): Promise<T> {
	return apiGet<T>('/settings/byo-key');
}
export function setByoKey<T = unknown>(body: Record<string, unknown>): Promise<T> {
	return apiSend<T>('POST', '/settings/byo-key', body);
}
export function deleteByoKey<T = unknown>(sudoRequestId?: string): Promise<T> {
	return apiSend<T>('DELETE', '/settings/byo-key', { sudo_request_id: sudoRequestId });
}

// ── MCP / tools ──────────────────────────────────────────────────────────────
export function listTools<T = unknown>(): Promise<T> {
	return apiGet<T>('/tools');
}
export function listMcpServers<T = unknown>(): Promise<T> {
	return apiGet<T>('/mcp/servers');
}
export function getMcpServer<T = unknown>(id: string): Promise<T> {
	return apiGet<T>(`/mcp/servers/${encodeURIComponent(id)}`);
}
export function createMcpServer<T = unknown>(body: Record<string, unknown>): Promise<T> {
	return apiSend<T>('POST', '/mcp/servers', body);
}
export function deleteMcpServer<T = unknown>(id: string): Promise<T> {
	return apiSend<T>('DELETE', `/mcp/servers/${encodeURIComponent(id)}`);
}
export function connectMcpServer<T = unknown>(id: string): Promise<T> {
	return apiSend<T>('POST', `/mcp/servers/${encodeURIComponent(id)}/connect`);
}
export function disconnectMcpServer<T = unknown>(id: string): Promise<T> {
	return apiSend<T>('POST', `/mcp/servers/${encodeURIComponent(id)}/disconnect`);
}
export function toggleMcpTool<T = unknown>(toolId: string): Promise<T> {
	return apiSend<T>('PATCH', `/mcp/tools/${encodeURIComponent(toolId)}/toggle`);
}

// ── Personas ─────────────────────────────────────────────────────────────────
export function listPersonas<T = unknown>(): Promise<T> {
	return apiGet<T>('/personas');
}
export function createPersona<T = unknown>(body: { title: string; content: string }): Promise<T> {
	return apiSend<T>('POST', '/personas', body);
}
export function updatePersona<T = unknown>(id: string, updates: object): Promise<T> {
	return apiSend<T>('PUT', `/personas/${encodeURIComponent(id)}`, updates);
}
export function deletePersona<T = unknown>(id: string): Promise<T> {
	return apiSend<T>('DELETE', `/personas/${encodeURIComponent(id)}`);
}
export function unhidePersona<T = unknown>(id: string): Promise<T> {
	return apiSend<T>('POST', `/personas/${encodeURIComponent(id)}/unhide`);
}
export function resetPersonas<T = unknown>(): Promise<T> {
	return apiSend<T>('POST', '/personas/reset');
}

// ── Chats (extras beyond createChat/updateChat/deleteChat above) ──────────────
export function listChats<T = unknown>(): Promise<T> {
	return apiGet<T>('/chats');
}
export function getChat<T = unknown>(id: string): Promise<T> {
	return apiGet<T>(`/chats/${encodeURIComponent(id)}`);
}
export function getChatUsage<T = unknown>(id: string): Promise<T> {
	return apiGet<T>(`/chats/${encodeURIComponent(id)}/usage`);
}
export function setChatTitle<T = unknown>(body: Record<string, unknown>): Promise<T> {
	return apiSend<T>('POST', '/chats/title', body);
}
export function cancelChat<T = unknown>(chatId: string): Promise<T> {
	return apiSend<T>('POST', '/chat/cancel', { chatId });
}
export function compactChat<T = unknown>(id: string, force = true): Promise<T> {
	return apiSend<T>('POST', `/chats/${encodeURIComponent(id)}/compact`, { force });
}
export function getChatPermissions<T = unknown>(id: string): Promise<T> {
	return apiGet<T>(`/chats/${encodeURIComponent(id)}/permissions`);
}
export function addChatPermission<T = unknown>(id: string, body: Record<string, unknown>): Promise<T> {
	return apiSend<T>('POST', `/chats/${encodeURIComponent(id)}/permissions`, body);
}
export function removeChatPermission<T = unknown>(id: string, entityId: string): Promise<T> {
	return apiSend<T>(
		'DELETE',
		`/chats/${encodeURIComponent(id)}/permissions/${encodeURIComponent(entityId)}`,
	);
}

// ── Models ───────────────────────────────────────────────────────────────────
export function listModels<T = unknown>(): Promise<T> {
	return apiGet<T>('/models');
}
export function getModel<T = unknown>(id: string): Promise<T> {
	return apiGet<T>(`/models/${encodeURIComponent(id)}`);
}
export function getRecommendedModels<T = unknown>(): Promise<T> {
	return apiGet<T>('/models/recommended');
}

// ── Page versions (yjs history) ──────────────────────────────────────────────
export function createPageVersion<T = unknown>(pageId: string, body: Record<string, unknown>): Promise<T> {
	return apiSend<T>('POST', `/pages/${encodeURIComponent(pageId)}/versions`, body);
}
export function listPageVersions<T = unknown>(pageId: string, limit?: number): Promise<T> {
	return apiGet<T>(`/pages/${encodeURIComponent(pageId)}/versions`, { limit });
}
export function getPageVersion<T = unknown>(versionId: string): Promise<T> {
	return apiGet<T>(`/pages/versions/${encodeURIComponent(versionId)}`);
}

// ── Setup (extras beyond getSetupState) ──────────────────────────────────────
export function setupLinkPoll<T = unknown>(): Promise<T> {
	return apiSend<T>('POST', '/setup/link/poll');
}
export function setupSubscribeStart<T = unknown>(): Promise<T> {
	return apiSend<T>('POST', '/setup/subscribe/start');
}
export function setupLoginStart<T = unknown>(email: string): Promise<T> {
	return apiSend<T>('POST', '/setup/login/start', { email });
}

// ── Sudo (privilege elevation) ───────────────────────────────────────────────
export function requestSudo<T = unknown>(action: string, actionPayload?: unknown): Promise<T> {
	return apiSend<T>('POST', '/sudo/request', { action, action_payload: actionPayload });
}
export function getSudoStatus<T = unknown>(id: string): Promise<T> {
	return apiGet<T>(`/sudo/status/${encodeURIComponent(id)}`);
}

// ── Mentions queue ───────────────────────────────────────────────────────────
export function getMentionQueue<T = unknown>(): Promise<T> {
	return apiGet<T>('/mentions/queue');
}
export function resolveMention<T = unknown>(path: string, body: Record<string, unknown>): Promise<T> {
	return apiSend<T>('POST', `/mentions/${encodeURIComponent(path)}`, body);
}

// ── Data lake ────────────────────────────────────────────────────────────────
export function getLakeSummary<T = unknown>(): Promise<T> {
	return apiGet<T>('/lake/summary');
}
export function getLakeStreams<T = unknown>(): Promise<T> {
	return apiGet<T>('/lake/streams');
}

// ── System / telemetry / usage ───────────────────────────────────────────────
export function getSystemTelemetry<T = unknown>(): Promise<T> {
	return apiGet<T>('/system/telemetry');
}
export function getSystemHistory<T = unknown>(): Promise<T> {
	return apiGet<T>('/system/history');
}
export function getMetricsActivity<T = unknown>(): Promise<T> {
	return apiGet<T>('/metrics/activity');
}
export function getAiCalls<T = unknown>(): Promise<T> {
	return apiGet<T>('/telemetry/ai-calls');
}
export function getAuthAudit<T = unknown>(): Promise<T> {
	return apiGet<T>('/audit/auth');
}
export function getUsageSummary<T = unknown>(): Promise<T> {
	return apiGet<T>('/usage/summary');
}
export function getSubscription<T = unknown>(): Promise<T> {
	return apiGet<T>('/subscription');
}

// ── Narrative identity (wiki) ────────────────────────────────────────────────
export function getNarrativeIdentity<T = unknown>(): Promise<T> {
	return apiGet<T>('/wiki/narrative-identity');
}
export function updateNarrativeIdentity<T = unknown>(body: Record<string, unknown>): Promise<T> {
	return apiSend<T>('PUT', '/wiki/narrative-identity', body);
}

// ── Devices / pairing (extras beyond pairMint/pairDeny/pairStatus) ────────────
export function listDevices<T = unknown>(): Promise<T> {
	return apiGet<T>('/devices');
}
export function pairConfirm<T = unknown>(id: string): Promise<T> {
	return apiSend<T>('POST', `/pair/confirm/${encodeURIComponent(id)}`);
}
export function pairConsume<T = unknown>(body?: Record<string, unknown>): Promise<T> {
	return apiSend<T>('POST', '/pair/consume', body);
}

// ── Misc singletons ──────────────────────────────────────────────────────────
export function getDeveloperTables<T = unknown>(): Promise<T> {
	return apiGet<T>('/developer/tables');
}
export function getDriveMedia<T = unknown>(): Promise<T> {
	return apiGet<T>('/drive/media');
}
export function searchUnsplash<T = unknown>(body: Record<string, unknown>): Promise<T> {
	return apiSend<T>('POST', '/unsplash/search', body);
}
export function getServerInfo<T = unknown>(): Promise<T> {
	return apiGet<T>('/app/server-info');
}
export function triggerAction<T = unknown>(id: string): Promise<T> {
	return apiSend<T>('POST', `/applets/${encodeURIComponent(id)}/trigger`);
}
export function aiComplete<T = unknown>(req: Record<string, unknown>): Promise<T> {
	return apiSend<T>('POST', '/ai/complete', req);
}
