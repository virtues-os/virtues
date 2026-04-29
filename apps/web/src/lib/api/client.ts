/**
 * API Client for Rust Core Library
 *
 * All calls to /api/* are proxied to Rust backend (localhost:8000)
 * via Vite proxy (see vite.config.ts).
 */

import { sanitizeUrl } from '$lib/utils/urlUtils';

const API_BASE = '/api';

// Sources
export async function listSources() {
	const res = await fetch(`${API_BASE}/sources`);
	if (!res.ok) throw new Error(`Failed to list sources: ${res.statusText}`);
	return res.json();
}

export async function getSource(sourceId: string) {
	const res = await fetch(`${API_BASE}/sources/${sourceId}`);
	if (!res.ok) throw new Error(`Failed to get source: ${res.statusText}`);
	return res.json();
}

export async function pauseSource(sourceId: string) {
	const res = await fetch(`${API_BASE}/sources/${sourceId}/pause`, {
		method: 'POST'
	});
	if (!res.ok) throw new Error(`Failed to pause source: ${res.statusText}`);
	return res.json();
}

export async function resumeSource(sourceId: string) {
	const res = await fetch(`${API_BASE}/sources/${sourceId}/resume`, {
		method: 'POST'
	});
	if (!res.ok) throw new Error(`Failed to resume source: ${res.statusText}`);
	return res.json();
}

export async function deleteSource(sourceId: string) {
	const res = await fetch(`${API_BASE}/sources/${sourceId}`, {
		method: 'DELETE'
	});
	if (!res.ok) throw new Error(`Failed to delete source: ${res.statusText}`);
}

// Streams
export async function listStreams(sourceId: string) {
	const res = await fetch(`${API_BASE}/sources/${sourceId}/streams`);
	if (!res.ok) throw new Error(`Failed to list streams: ${res.statusText}`);
	return res.json();
}

export async function enableStream(
	sourceId: string,
	streamName: string,
	request: Record<string, unknown> = {}
) {
	const res = await fetch(`${API_BASE}/sources/${sourceId}/streams/${streamName}/enable`, {
		method: 'POST',
		headers: { 'Content-Type': 'application/json' },
		body: JSON.stringify(request)
	});
	if (!res.ok) throw new Error(`Failed to enable stream: ${res.statusText}`);
}

export async function disableStream(sourceId: string, streamName: string) {
	const res = await fetch(`${API_BASE}/sources/${sourceId}/streams/${streamName}/disable`, {
		method: 'POST'
	});
	if (!res.ok) throw new Error(`Failed to disable stream: ${res.statusText}`);
}

export async function syncStream(
	sourceId: string,
	streamName: string,
	request: Record<string, unknown> = {}
): Promise<{ job_id: string; status: string; started_at: string }> {
	const res = await fetch(`${API_BASE}/sources/${sourceId}/streams/${streamName}/sync`, {
		method: 'POST',
		headers: { 'Content-Type': 'application/json' },
		body: JSON.stringify(request)
	});
	if (!res.ok) {
		const error = await res.json().catch(() => ({ error: res.statusText }));
		throw new Error(error.error || `Failed to sync stream: ${res.statusText}`);
	}
	return res.json();
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

export interface Action {
	id: string;
	owner: 'system' | 'user';
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
	is_system: boolean;
	created_at: string;
	updated_at: string;
	last_run: ActionLastRun | null;
}

export interface ActionDetail extends Action {
	recent_runs?: ActionRun[];
}

export async function listActions(): Promise<Action[]> {
	const res = await fetch(`${API_BASE}/actions`);
	if (!res.ok) throw new Error(`Failed to list actions: ${res.statusText}`);
	return res.json();
}

export async function getAction(id: string): Promise<Action> {
	const res = await fetch(`${API_BASE}/actions/${encodeURIComponent(id)}`);
	if (!res.ok) throw new Error(`Failed to get action: ${res.statusText}`);
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
	const res = await fetch(`${API_BASE}/actions`, {
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
	const res = await fetch(`${API_BASE}/actions/${encodeURIComponent(id)}`, {
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

export async function deleteAction(id: string): Promise<void> {
	const res = await fetch(`${API_BASE}/actions/${encodeURIComponent(id)}`, {
		method: 'DELETE'
	});
	if (!res.ok) {
		const err = await res.json().catch(() => ({ error: res.statusText }));
		throw new Error(err.error || `Failed to delete action: ${res.statusText}`);
	}
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
	const res = await fetch(`${API_BASE}/actions/${encodeURIComponent(id)}/run`, {
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
		`${API_BASE}/actions/${encodeURIComponent(id)}/runs${qs ? `?${qs}` : ''}`
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
	const res = await fetch(`${API_BASE}/actions/runs/${jobId}`);
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

export interface Credential {
	id: string;
	provider: string;
	name: string;
	auth_type: string;
	is_active: boolean;
	device_info: DeviceInfo | null;
	last_seen_at: string | null;
	created_at: string;
	action_count: number;
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
 *
 * Note: this is named `listSourceCatalog` (not `listSources`) because the
 * legacy `listSources()` at the top of this file still exists for dead
 * endpoints awaiting cleanup. Both eventually collapse to one name.
 */
export async function listSourceCatalog(): Promise<SourceCatalogItem[]> {
	const res = await fetch(`${API_BASE}/sources`);
	if (!res.ok) throw new Error(`Failed to list sources: ${res.statusText}`);
	return res.json();
}

// Backwards-compat aliases (deleted in Phase 6 cleanup).
export type ConnectorAuthKind = SourceAuthKind;
export type ConnectorCatalogItem = SourceCatalogItem;
export const listConnectors = listSourceCatalog;

// ─────────────────────────────────────────────────────────────────────────────
// Source-connect flows (drive the 5 thin handlers in core/src/api/source_auth.rs)
// ─────────────────────────────────────────────────────────────────────────────

export interface PairInitiateResponse {
	credential_id: string;
	qr_payload: string;
}

/** POST /api/pairing/initiate — mint a pending credential for a self_issued_bearer source. */
export async function pairInitiate(
	source_id: string,
	name: string
): Promise<PairInitiateResponse> {
	const res = await fetch(`${API_BASE}/pairing/initiate`, {
		method: 'POST',
		headers: { 'Content-Type': 'application/json' },
		body: JSON.stringify({ source_id, name })
	});
	if (!res.ok) {
		const err = await res.json().catch(() => ({ error: res.statusText }));
		throw new Error(err.error || `pair_initiate failed: ${res.statusText}`);
	}
	return res.json();
}

export interface PairCompleteResponse {
	credential_id: string;
	action_ids: Record<string, string>;
}

/** POST /api/pairing/complete/:credential_id — finalize self_issued_bearer + return fan-out. */
export async function pairComplete(
	credential_id: string,
	token: string,
	device_info: Record<string, unknown> = {}
): Promise<PairCompleteResponse> {
	const res = await fetch(
		`${API_BASE}/pairing/complete/${encodeURIComponent(credential_id)}`,
		{
			method: 'POST',
			headers: { 'Content-Type': 'application/json' },
			body: JSON.stringify({ token, device_info })
		}
	);
	if (!res.ok) {
		const err = await res.json().catch(() => ({ error: res.statusText }));
		throw new Error(err.error || `pair_complete failed: ${res.statusText}`);
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

// OAuth
/**
 * Initiate OAuth authorization flow
 *
 * @param provider - OAuth provider name (e.g., "google", "notion")
 * @param returnUrl - Full URL where user should be redirected after OAuth completes.
 *   This is validated against an allowlist on the backend. Examples:
 *   - `http://localhost:5173/data/sources/add` (web dev)
 *   - `https://app.virtues.com/data/sources/add` (web prod)
 *   - `virtues://oauth/callback` (iOS app)
 *   - `/data/sources/add` (relative path)
 *
 * The backend stores this in a signed state token and redirects the user here
 * after OAuth completes, appending `?source_id=xxx&connected=true`.
 */
export async function initiateOAuth(provider: string, returnUrl?: string) {
	const params = new URLSearchParams();
	if (returnUrl) params.set('state', returnUrl);

	const url = `${API_BASE}/sources/${provider}/authorize${params.toString() ? `?${params}` : ''}`;
	const res = await fetch(url, { method: 'POST' });
	if (!res.ok) throw new Error(`Failed to initiate OAuth: ${res.statusText}`);
	return res.json() as Promise<{ authorization_url: string; state: string }>;
}

// Device Pairing
import type {
	InitiatePairingRequest,
	PairingInitResponse,
	CompletePairingRequest,
	PairingCompleteResponse,
	PairingStatus,
	PendingPairing
} from '$lib/types/device-pairing';

/**
 * Initiate device pairing — wraps the Phase 6+ `/api/pairing/initiate` endpoint.
 *
 * The legacy 6-char pairing code is gone; instead the new endpoint returns
 * a `credential_id` + `qr_payload`. We adapt to the legacy `PairingInitResponse`
 * shape (with `source_id` populated from `credential_id`) so the modal that
 * still reads this type keeps working without a rewrite.
 *
 * @param deviceType - Source id (e.g., "ios", "mac"). Becomes the `source_id`
 *   on the credentials row.
 * @param name - Display name for the credential.
 */
export async function initiatePairing(
	deviceType: string,
	name: string
): Promise<PairingInitResponse> {
	const { credential_id } = await pairInitiate(deviceType, name);
	return {
		source_id: credential_id,
		code: '',
		expires_at: new Date(Date.now() + 10 * 60 * 1000).toISOString()
	};
}

/**
 * Check pairing status by polling the credential row.
 *
 * The legacy `GET /api/devices/pairing/:source_id` endpoint is gone.
 * We poll the credentials list and look up the row by id.
 */
export async function getPairingStatus(sourceId: string): Promise<PairingStatus> {
	const list = await listCredentials();
	const row = list.find((c) => c.id === sourceId);
	if (!row) {
		return { status: 'pending' };
	}
	if (row.is_active) {
		return {
			status: 'active',
			device_info: row.device_info ?? {
				device_id: '',
				device_name: row.name,
				device_model: '',
				os_version: ''
			}
		};
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

// Plaid Link
export interface PlaidLinkTokenResponse {
	link_token: string;
	expiration: string;
}

export interface PlaidExchangeTokenRequest {
	public_token: string;
	institution_id?: string;
	institution_name?: string;
}

export interface ConnectedAccountSummary {
	account_id: string;
	name: string;
	/** Plaid's standardized account type: depository, credit, loan, investment, brokerage, other */
	account_type: string;
	/** More specific subtype: checking, savings, credit card, mortgage, 401k, etc. */
	subtype?: string;
	/** Last 4 digits of account number */
	mask?: string;
}

export interface PlaidExchangeTokenResponse {
	source_id: string;
	item_id: string;
	institution_name?: string;
	/** Summary of connected accounts for display */
	connected_accounts: ConnectedAccountSummary[];
}

/**
 * Create a Plaid Link token for initializing Plaid Link SDK
 * @returns Link token and expiration time
 */
export async function createPlaidLinkToken(): Promise<PlaidLinkTokenResponse> {
	const res = await fetch(`${API_BASE}/plaid/link-token`, {
		method: 'POST',
		headers: { 'Content-Type': 'application/json' },
		body: JSON.stringify({})
	});

	if (!res.ok) {
		const error = await res.json().catch(() => ({ error: res.statusText }));
		throw new Error(error.error || `Failed to create Plaid link token: ${res.statusText}`);
	}

	return res.json();
}

/**
 * Exchange a Plaid public token for an access token
 * Called after user completes Plaid Link flow
 * @param params - Public token and optional institution info
 * @returns Source ID and Item ID
 */
export async function exchangePlaidToken(
	params: PlaidExchangeTokenRequest
): Promise<PlaidExchangeTokenResponse> {
	const res = await fetch(`${API_BASE}/plaid/exchange-token`, {
		method: 'POST',
		headers: { 'Content-Type': 'application/json' },
		body: JSON.stringify(params)
	});

	if (!res.ok) {
		const error = await res.json().catch(() => ({ error: res.statusText }));
		throw new Error(error.error || `Failed to exchange Plaid token: ${res.statusText}`);
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
	timezone?: string | null;
	home_place_id?: string | null;
	home_city?: string | null;
	home_country?: string | null;
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
	updates: { title?: string; icon?: string | null }
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
// Spaces API
// =============================================================================

export interface Space {
	id: string;
	name: string;
	icon: string | null;
	is_system: boolean;
	sort_order: number;
	theme_id: string;
	accent_color: string | null;
	active_tab_state_json: string | null;
	created_at: string;
	updated_at: string;
}

export interface SpaceSummary {
	id: string;
	name: string;
	icon: string | null;
	is_system: boolean;
	sort_order: number;
	theme_id: string;
	accent_color: string | null;
	created_at: string;
	updated_at: string;
}

export interface SpaceListResponse {
	spaces: SpaceSummary[];
}

/**
 * List all spaces
 */
export async function listSpaces(): Promise<SpaceListResponse> {
	const res = await fetch(`${API_BASE}/spaces`);
	if (!res.ok) throw new Error(`Failed to list spaces: ${res.statusText}`);
	return res.json();
}

/**
 * Get a single space by ID
 */
export async function getSpace(id: string): Promise<Space> {
	const res = await fetch(`${API_BASE}/spaces/${id}`);
	if (!res.ok) throw new Error(`Failed to get space: ${res.statusText}`);
	return res.json();
}

// createSpace removed — single workspace model

/**
 * Update an existing space
 */
export async function updateSpace(
	id: string,
	updates: {
		name?: string;
		icon?: string;
		sort_order?: number;
		theme_id?: string;
		accent_color?: string;
	}
): Promise<Space> {
	const res = await fetch(`${API_BASE}/spaces/${id}`, {
		method: 'PUT',
		headers: { 'Content-Type': 'application/json' },
		body: JSON.stringify(updates)
	});
	if (!res.ok) throw new Error(`Failed to update space: ${res.statusText}`);
	return res.json();
}

// deleteSpace + saveSpaceTabState removed — single workspace model

// =============================================================================
// Views API (replaces Explorer Nodes)
// =============================================================================

export interface View {
	id: string;
	space_id: string;
	parent_view_id: string | null;
	name: string;
	icon: string | null;
	sort_order: number;
	view_type: 'manual' | 'smart';
	query_config: string | null;
	is_system: boolean;
	created_at: string;
	updated_at: string;
}

export interface ViewSummary {
	id: string;
	space_id: string;
	parent_view_id: string | null;
	name: string;
	icon: string | null;
	sort_order: number;
	view_type: 'manual' | 'smart';
	query_config: string | null;
	is_system: boolean;
}

export interface ViewListResponse {
	views: ViewSummary[];
}

export interface ViewEntity {
	id: string;
	name: string;
	namespace: string;
	icon: string;
	updated_at?: string;
}

/** Space item entity — ViewEntity with sort_order for unified ordering with folders */
export interface SpaceItemEntity extends ViewEntity {
	sort_order: number;
}

export interface ViewResolutionResponse {
	entities: ViewEntity[];
	total: number;
	has_more: boolean;
}

export interface CreateViewRequest {
	name: string;
	icon?: string;
	view_type: 'manual' | 'smart';
	parent_view_id?: string;
	query_config?: object;
}

/**
 * List all views for a space
 */
export async function listViews(spaceId: string): Promise<ViewListResponse> {
	const res = await fetch(`${API_BASE}/spaces/${spaceId}/views`);
	if (!res.ok) throw new Error(`Failed to list views: ${res.statusText}`);
	return res.json();
}

/**
 * Get a single view by ID
 */
export async function getView(viewId: string): Promise<View> {
	const res = await fetch(`${API_BASE}/views/${viewId}`);
	if (!res.ok) throw new Error(`Failed to get view: ${res.statusText}`);
	return res.json();
}

/**
 * Create a new view in a space
 */
export async function createView(
	spaceId: string,
	request: CreateViewRequest
): Promise<View> {
	const res = await fetch(`${API_BASE}/views`, {
		method: 'POST',
		headers: { 'Content-Type': 'application/json' },
		body: JSON.stringify({
			space_id: spaceId,
			...request
		})
	});
	if (!res.ok) throw new Error(`Failed to create view: ${res.statusText}`);
	return res.json();
}

/**
 * Update an existing view
 */
export async function updateView(
	viewId: string,
	updates: {
		name?: string;
		icon?: string;
		sort_order?: number;
		query_config?: object;
	}
): Promise<View> {
	const res = await fetch(`${API_BASE}/views/${viewId}`, {
		method: 'PUT',
		headers: { 'Content-Type': 'application/json' },
		body: JSON.stringify(updates)
	});
	if (!res.ok) throw new Error(`Failed to update view: ${res.statusText}`);
	return res.json();
}

/**
 * Delete a view by ID
 */
export async function deleteView(viewId: string): Promise<void> {
	const res = await fetch(`${API_BASE}/views/${viewId}`, { method: 'DELETE' });
	if (!res.ok) throw new Error(`Failed to delete view: ${res.statusText}`);
}

/**
 * Resolve a view to its entities
 */
export async function resolveView(viewId: string): Promise<ViewResolutionResponse> {
	const res = await fetch(`${API_BASE}/views/${viewId}/resolve`, {
		method: 'POST'
	});
	if (!res.ok) throw new Error(`Failed to resolve view: ${res.statusText}`);
	return res.json();
}

/**
 * Add an item to a manual view
 * @param viewId - The view to add the item to
 * @param url - The URL of the item (e.g., '/page/page_xyz', '/person/person_abc')
 */
export async function addViewItem(viewId: string, url: string): Promise<void> {
	const sanitizedUrl = sanitizeUrl(url);
	const res = await fetch(`${API_BASE}/views/${viewId}/items`, {
		method: 'POST',
		headers: { 'Content-Type': 'application/json' },
		body: JSON.stringify({ url: sanitizedUrl })
	});
	if (!res.ok) throw new Error(`Failed to add item to view: ${res.statusText}`);
}

/**
 * Remove an item from a manual view
 * @param viewId - The view to remove the item from
 * @param url - The URL of the item (e.g., '/page/page_xyz', '/person/person_abc')
 */
export async function removeViewItem(viewId: string, url: string): Promise<void> {
	const res = await fetch(`${API_BASE}/views/${viewId}/items`, {
		method: 'DELETE',
		headers: { 'Content-Type': 'application/json' },
		body: JSON.stringify({ url })
	});
	if (!res.ok) throw new Error(`Failed to remove item from view: ${res.statusText}`);
}

/**
 * View item as stored in the database
 */
export interface ViewItem {
	id: number;
	view_id: string;
	url: string;
	sort_order: number;
	created_at: string;
}

/**
 * List items in a manual view
 */
export async function listViewItems(viewId: string): Promise<ViewItem[]> {
	const res = await fetch(`${API_BASE}/views/${viewId}/items`);
	if (!res.ok) throw new Error(`Failed to list view items: ${res.statusText}`);
	return res.json();
}

/**
 * Reorder items in a manual view
 */
export async function reorderViewItems(viewId: string, urlOrder: string[]): Promise<void> {
	const res = await fetch(`${API_BASE}/views/${viewId}/items/reorder`, {
		method: 'PUT',
		headers: { 'Content-Type': 'application/json' },
		body: JSON.stringify({ url_order: urlOrder })
	});
	if (!res.ok) throw new Error(`Failed to reorder view items: ${res.statusText}`);
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
// Space Items API (root-level items at space level, not in any folder)
// =============================================================================

/**
 * List items at space root level (not inside any folder)
 * @param spaceId - The space ID
 * @returns Resolved entities for the space's root items
 */
export async function listSpaceItems(spaceId: string): Promise<SpaceItemEntity[]> {
	const res = await fetch(`${API_BASE}/spaces/${spaceId}/items`);
	if (!res.ok) throw new Error(`Failed to list space items: ${res.statusText}`);
	return res.json();
}

/**
 * Add an item to space root level
 * @param spaceId - The space ID
 * @param url - The URL of the item (e.g., '/page/page_xyz', '/person/person_abc')
 */
export async function addSpaceItem(spaceId: string, url: string): Promise<void> {
	const sanitizedUrl = sanitizeUrl(url);
	const res = await fetch(`${API_BASE}/spaces/${spaceId}/items`, {
		method: 'POST',
		headers: { 'Content-Type': 'application/json' },
		body: JSON.stringify({ url: sanitizedUrl })
	});
	if (!res.ok) throw new Error(`Failed to add space item: ${res.statusText}`);
}

/**
 * Remove an item from space root level
 * @param spaceId - The space ID
 * @param url - The URL of the item to remove
 */
export async function removeSpaceItem(spaceId: string, url: string): Promise<void> {
	const res = await fetch(`${API_BASE}/spaces/${spaceId}/items`, {
		method: 'DELETE',
		headers: { 'Content-Type': 'application/json' },
		body: JSON.stringify({ url })
	});
	if (!res.ok) throw new Error(`Failed to remove space item: ${res.statusText}`);
}

/**
 * Reorder items at space root level
 * @param spaceId - The space ID
 * @param urlOrder - Array of URLs in the new desired order
 */
export async function reorderSpaceItems(
	spaceId: string,
	items: Array<{ url: string; sort_order: number }>
): Promise<void> {
	const res = await fetch(`${API_BASE}/spaces/${spaceId}/items/reorder`, {
		method: 'PUT',
		headers: { 'Content-Type': 'application/json' },
		body: JSON.stringify({ items })
	});
	if (!res.ok) throw new Error(`Failed to reorder space items: ${res.statusText}`);
}

// =============================================================================
// Pages API
// =============================================================================

export interface Page {
	id: string;
	title: string;
	content: string;
	space_id: string | null;
	icon: string | null;
	cover_url: string | null;
	tags: string | null; // JSON array string: '["tag1", "tag2"]'
	created_at: string;
	updated_at: string;
}

export interface PageSummary {
	id: string;
	title: string;
	space_id: string | null;
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

export interface EntitySearchResult {
	id: string;
	name: string;
	entity_type: string;
	icon: string;
}

export interface EntitySearchResponse {
	results: EntitySearchResult[];
}

/**
 * List all pages with optional pagination and workspace filter
 */
export async function listPages(limit?: number, offset?: number, space_id?: string): Promise<PageListResponse> {
	const params = new URLSearchParams();
	if (limit !== undefined) params.set('limit', String(limit));
	if (offset !== undefined) params.set('offset', String(offset));
	if (space_id !== undefined) params.set('space_id', space_id);

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
	space_id: string | null = null,
	options?: { icon?: string; cover_url?: string; tags?: string }
): Promise<Page> {
	const res = await fetch(`${API_BASE}/pages`, {
		method: 'POST',
		headers: { 'Content-Type': 'application/json' },
		body: JSON.stringify({ title, content, spaceId: space_id, ...options })
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
		space_id?: string | null;
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
export async function searchEntities(query: string): Promise<EntitySearchResponse> {
	const res = await fetch(`${API_BASE}/pages/search/entities?q=${encodeURIComponent(query)}`);
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
// Projects API
// ============================================================================

export interface Project {
	id: string;
	name: string;
	icon: string | null;
	description: string | null;
	sort_order: number;
	created_at: string;
	updated_at: string;
}

export interface ProjectSummary extends Project {
	item_count: number;
}

export interface ProjectItem {
	id: string;
	project_id: string;
	url: string;
	name: string | null;
	description: string | null;
	sort_order: number;
	added_at: string;
}

export interface ProjectDetail extends Project {
	items: ProjectItem[];
}

export interface ProjectListResponse {
	projects: ProjectSummary[];
}

export async function listProjects(): Promise<ProjectListResponse> {
	const res = await fetch(`${API_BASE}/projects`);
	if (!res.ok) throw new Error(`Failed to list projects: ${res.statusText}`);
	return res.json();
}

export async function getProject(id: string): Promise<ProjectDetail> {
	const res = await fetch(`${API_BASE}/projects/${id}`);
	if (!res.ok) throw new Error(`Failed to get project: ${res.statusText}`);
	return res.json();
}

export async function createProject(
	name: string,
	options?: { icon?: string | null; description?: string | null },
): Promise<Project> {
	const res = await fetch(`${API_BASE}/projects`, {
		method: 'POST',
		headers: { 'Content-Type': 'application/json' },
		body: JSON.stringify({ name, icon: options?.icon ?? null, description: options?.description ?? null }),
	});
	if (!res.ok) throw new Error(`Failed to create project: ${res.statusText}`);
	return res.json();
}

export async function updateProject(
	id: string,
	updates: {
		name?: string;
		icon?: string | null;
		description?: string | null;
		sort_order?: number;
	},
): Promise<Project> {
	const res = await fetch(`${API_BASE}/projects/${id}`, {
		method: 'PATCH',
		headers: { 'Content-Type': 'application/json' },
		body: JSON.stringify(updates),
	});
	if (!res.ok) throw new Error(`Failed to update project: ${res.statusText}`);
	return res.json();
}

export async function deleteProject(id: string): Promise<void> {
	const res = await fetch(`${API_BASE}/projects/${id}`, { method: 'DELETE' });
	if (!res.ok) throw new Error(`Failed to delete project: ${res.statusText}`);
}

export async function addProjectItem(
	projectId: string,
	url: string,
	options?: { name?: string | null; description?: string | null },
): Promise<ProjectItem> {
	const res = await fetch(`${API_BASE}/projects/${projectId}/items`, {
		method: 'POST',
		headers: { 'Content-Type': 'application/json' },
		body: JSON.stringify({ url, name: options?.name ?? null, description: options?.description ?? null }),
	});
	if (!res.ok) throw new Error(`Failed to add project item: ${res.statusText}`);
	return res.json();
}

export async function removeProjectItem(projectId: string, url: string): Promise<void> {
	const res = await fetch(`${API_BASE}/projects/${projectId}/items`, {
		method: 'DELETE',
		headers: { 'Content-Type': 'application/json' },
		body: JSON.stringify({ url }),
	});
	if (!res.ok) throw new Error(`Failed to remove project item: ${res.statusText}`);
}

export async function reorderProjectItems(projectId: string, urls: string[]): Promise<void> {
	const res = await fetch(`${API_BASE}/projects/${projectId}/items/reorder`, {
		method: 'PUT',
		headers: { 'Content-Type': 'application/json' },
		body: JSON.stringify({ urls }),
	});
	if (!res.ok) throw new Error(`Failed to reorder project items: ${res.statusText}`);
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

