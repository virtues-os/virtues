/**
 * API Client for Rust Core Library
 *
 * All calls to /api/* are proxied to Rust backend (localhost:8000)
 * via Vite proxy (see vite.config.ts).
 */

const API_BASE = '/api';

// Catalog
export async function listCatalogSources() {
	const res = await fetch(`${API_BASE}/catalog/sources`);
	if (!res.ok) throw new Error(`Failed to list catalog sources: ${res.statusText}`);
	return res.json();
}

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
	request: any = {}
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
	request: any = {}
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

// Jobs
export interface Job {
	id: string;
	job_type: 'sync' | 'transform';
	status: 'pending' | 'running' | 'succeeded' | 'failed' | 'cancelled';
	source_id?: string;
	source_name?: string; // Enriched in page load, not from API
	stream_name?: string;
	sync_mode?: 'full_refresh' | 'incremental';
	transform_id?: string;
	started_at: string;
	completed_at?: string;
	records_processed: number;
	error_message?: string;
	error_class?: string;
	metadata: any;
	created_at: string;
	updated_at: string;
}

export async function getJobStatus(jobId: string): Promise<Job> {
	const res = await fetch(`${API_BASE}/jobs/${jobId}`);
	if (!res.ok) throw new Error(`Failed to get job status: ${res.statusText}`);
	return res.json();
}

export async function queryJobs(params: {
	source_id?: string;
	status?: string[]; // e.g., ['succeeded', 'failed']
	limit?: number;
}): Promise<Job[]> {
	const queryParams = new URLSearchParams();
	if (params.source_id) queryParams.set('source_id', params.source_id);
	if (params.status && params.status.length > 0) {
		queryParams.set('status', params.status.join(','));
	}
	if (params.limit) queryParams.set('limit', params.limit.toString());

	const res = await fetch(`${API_BASE}/jobs?${queryParams}`);
	if (!res.ok) throw new Error(`Failed to query jobs: ${res.statusText}`);
	return res.json();
}

export async function cancelJob(jobId: string): Promise<void> {
	const res = await fetch(`${API_BASE}/jobs/${jobId}/cancel`, {
		method: 'POST'
	});
	if (!res.ok) throw new Error(`Failed to cancel job: ${res.statusText}`);
}

// Activity Metrics
export interface ActivityMetrics {
	summary: MetricsSummary;
	by_job_type: JobTypeStats[];
	by_stream: StreamStats[];
	time_windows: TimeWindowMetrics;
	recent_errors: RecentError[];
}

export interface MetricsSummary {
	total_jobs: number;
	succeeded: number;
	failed: number;
	cancelled: number;
	active: number;
	success_rate_percent: number;
	total_records_processed: number;
	avg_duration_seconds: number | null;
}

export interface JobTypeStats {
	job_type: string;
	total: number;
	succeeded: number;
	failed: number;
	avg_duration_seconds: number | null;
	total_records: number;
}

export interface StreamStats {
	stream_name: string;
	job_count: number;
	success_rate_percent: number;
	last_sync_at: string | null;
	total_records: number;
}

export interface TimeWindowMetrics {
	last_24h: PeriodStats;
	last_7d: PeriodStats;
	last_30d: PeriodStats;
}

export interface PeriodStats {
	jobs_completed: number;
	jobs_failed: number;
	success_rate_percent: number;
	records_processed: number;
}

export interface RecentError {
	job_id: string;
	job_type: string;
	stream_name: string | null;
	error_message: string;
	error_class: string | null;
	failed_at: string;
}

export async function getActivityMetrics(): Promise<ActivityMetrics> {
	const res = await fetch(`${API_BASE}/metrics/activity`);
	if (!res.ok) {
		throw new Error(`Failed to get activity metrics: ${res.statusText}`);
	}
	return res.json();
}

// OAuth
export async function initiateOAuth(provider: string, redirectUri?: string, state?: string) {
	const params = new URLSearchParams();
	if (redirectUri) params.set('redirect_uri', redirectUri);
	if (state) params.set('state', state);

	const url = `${API_BASE}/sources/${provider}/authorize${params.toString() ? `?${params}` : ''}`;
	const res = await fetch(url, { method: 'POST' });
	if (!res.ok) throw new Error(`Failed to initiate OAuth: ${res.statusText}`);
	return res.json() as Promise<{ authorization_url: string; state: string }>;
}

export async function handleOAuthCallback(params: {
	code?: string;
	access_token?: string;
	refresh_token?: string;
	expires_in?: number;
	provider: string;
	state?: string;
	workspace_id?: string;
	workspace_name?: string;
	bot_id?: string;
}) {
	const queryParams = new URLSearchParams();
	Object.entries(params).forEach(([key, value]) => {
		if (value !== undefined) {
			queryParams.set(key, value.toString());
		}
	});

	const res = await fetch(`${API_BASE}/sources/callback?${queryParams}`);
	if (!res.ok) throw new Error(`Failed to complete OAuth: ${res.statusText}`);
	return res.json();
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
 * Initiate device pairing - generates a 6-character pairing code
 * @param deviceType - Type of device (e.g., "ios", "mac")
 * @param name - Display name for the device
 * @returns Pairing code, source ID, and expiration time
 */
export async function initiatePairing(
	deviceType: string,
	name: string
): Promise<PairingInitResponse> {
	const request: InitiatePairingRequest = {
		device_type: deviceType,
		name
	};

	const res = await fetch(`${API_BASE}/devices/pairing/initiate`, {
		method: 'POST',
		headers: { 'Content-Type': 'application/json' },
		body: JSON.stringify(request)
	});

	if (!res.ok) {
		const error = await res.json().catch(() => ({ error: res.statusText }));
		throw new Error(error.error || `Failed to initiate pairing: ${res.statusText}`);
	}

	return res.json();
}

/**
 * Complete device pairing (typically called by device, not web UI)
 * @param code - 6-character pairing code
 * @param deviceInfo - Device information (ID, name, model, OS version)
 * @returns Device token and available streams
 */
export async function completePairing(
	code: string,
	deviceInfo: CompletePairingRequest['device_info']
): Promise<PairingCompleteResponse> {
	const request: CompletePairingRequest = {
		code,
		device_info: deviceInfo
	};

	const res = await fetch(`${API_BASE}/devices/pairing/complete`, {
		method: 'POST',
		headers: { 'Content-Type': 'application/json' },
		body: JSON.stringify(request)
	});

	if (!res.ok) {
		const error = await res.json().catch(() => ({ error: res.statusText }));
		throw new Error(error.error || `Failed to complete pairing: ${res.statusText}`);
	}

	return res.json();
}

/**
 * Check the status of a device pairing
 * @param sourceId - Source ID from initiatePairing
 * @returns Current pairing status (pending, active, or revoked)
 */
export async function getPairingStatus(sourceId: string): Promise<PairingStatus> {
	const res = await fetch(`${API_BASE}/devices/pairing/${sourceId}`);

	if (!res.ok) {
		throw new Error(`Failed to get pairing status: ${res.statusText}`);
	}

	return res.json();
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

// Storage Objects
export interface StreamObject {
	id: string;
	source_connection_id: string;
	source_name: string;
	source_type: string;
	stream_name: string;
	s3_key: string;
	record_count: number;
	size_bytes: number;
	min_timestamp: string | null;
	max_timestamp: string | null;
	created_at: string;
}

export interface ObjectContent {
	id: string;
	s3_key: string;
	records: unknown[];
	record_count: number;
}

/**
 * List recent storage objects
 * @param limit - Maximum number of objects to return (default: 10)
 * @returns Array of stream object summaries
 */
export async function listStorageObjects(limit?: number): Promise<StreamObject[]> {
	const params = new URLSearchParams();
	if (limit) params.set('limit', limit.toString());

	const res = await fetch(`${API_BASE}/storage/objects?${params}`);
	if (!res.ok) throw new Error(`Failed to list storage objects: ${res.statusText}`);
	return res.json();
}

/**
 * Get decrypted content of a storage object
 * @param objectId - UUID of the stream object
 * @returns Decrypted JSONL records
 */
export async function getStorageObjectContent(objectId: string): Promise<ObjectContent> {
	const res = await fetch(`${API_BASE}/storage/objects/${objectId}/content`);
	if (!res.ok) {
		const error = await res.json().catch(() => ({ error: res.statusText }));
		throw new Error(error.error || `Failed to get object content: ${res.statusText}`);
	}
	return res.json();
}
