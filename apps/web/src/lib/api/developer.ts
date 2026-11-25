/**
 * Query (Sources · Query): SQL against your own server, read only.
 *
 * The server runs each query inside a READ ONLY transaction
 * (`virtues-core/src/api/developer.rs`), so nothing typed here can change a
 * table. It can read every table, so the request carries the app's owner
 * proof. Deleting rows of your own records goes through `deleteRows` in
 * `./deletions`, which tombstones them.
 *
 * TODO(2026-11-19): move getDeveloperTables here from client.ts.
 */
import { apiSendProven } from './client';

export interface SqlResult {
	columns: string[];
	rows: Record<string, unknown>[];
}

export const runSql = (sql: string) => apiSendProven<SqlResult>('POST', '/developer/sql', { sql });
