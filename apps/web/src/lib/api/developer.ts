/**
 * Query (Sources · Query): SQL against your own server, read only.
 *
 * The server runs each query inside a READ ONLY transaction
 * (`virtues-core/src/api/developer.rs`), so nothing typed here can change a
 * table. Deleting rows of your own records goes through `deleteRows` in
 * `./deletions`, which tombstones them.
 *
 * TODO(2026-11-19): move getDeveloperTables here from client.ts.
 */
import { apiSend } from './client';

export interface SqlResult {
	columns: string[];
	rows: Record<string, unknown>[];
}

export const runSql = (sql: string) => apiSend<SqlResult>('POST', '/developer/sql', { sql });
