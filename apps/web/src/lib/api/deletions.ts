/**
 * Deleting ingested records (virtues-core `api/deletions.rs`).
 *
 * A deletion is proposed in chat by the assistant's `delete_data` tool, or
 * made directly in the table viewer. Allowing one and deleting rows both need
 * the app's owner proof, so they go through {@link apiSendProven}.
 */
import { apiGet, apiSend, apiSendProven } from './client';

/** A row the card shows so you can check what a deletion covers. */
export interface DeletionSample {
	at: string | null;
	label: string;
	preview: string;
}

/** One table's share of a deletion, counted when the card asks. */
export interface DeletionTarget {
	table: string;
	/** What its rows are called: "Messages". */
	label: string;
	count: number;
	earliest: string | null;
	latest: string | null;
	samples: DeletionSample[];
}

/** What a deletion removed, by place. Counts and dates only. */
export interface DeletionReceipt {
	records?: number;
	search_entries?: number;
	links?: number;
	files?: number;
	tombstones?: number;
	/** Raw records taken out of the files each sync kept. */
	raw_records?: number;
	/** Deleted records your server found no raw copy for. */
	raw_missing?: number;
	tables?: { table: string; label: string; records: number }[];
	earliest?: string | null;
	latest?: string | null;
	/** What it did not reach: `backups`, `pages_and_chats`. */
	kept?: string[];
	/** Set once you let its records be collected again. */
	tombstones_lifted?: number;
}

export interface Deletion {
	id: string;
	chat_id: string | null;
	/** What is being deleted, as the card's title says it. */
	summary: string;
	status: 'proposed' | 'running' | 'done' | 'failed' | 'cancelled';
	/** Present while it waits for you. */
	targets: DeletionTarget[];
	preview_error: string | null;
	receipt: DeletionReceipt;
	error: string | null;
	created_at: string;
	approved_at: string | null;
	finished_at: string | null;
}

const path = (id: string, rest = '') => `/deletions/${encodeURIComponent(id)}${rest}`;

export const getDeletion = (id: string) => apiGet<Deletion>(path(id));

/** Allow it. Your server starts deleting; follow it with {@link getDeletion}. */
export const allowDeletion = (id: string) => apiSendProven<Deletion>('POST', path(id, '/allow'));

export const cancelDeletion = (id: string) => apiSend<Deletion>('POST', path(id, '/cancel'));

/** Let a finished deletion's records be collected again. */
export const liftTombstones = (id: string) => apiSend<{ lifted: number }>('DELETE', path(id, '/tombstones'));

/** Delete rows of one data table by id, from the table viewer. */
export const deleteRows = (table: string, ids: string[]) =>
	apiSendProven<DeletionReceipt>('POST', '/developer/rows/delete', { table, ids });
