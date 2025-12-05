import pg from 'pg';
import { drizzle, type NodePgDatabase } from 'drizzle-orm/node-postgres';
import { env } from '$env/dynamic/private';
import * as schema from './schema';

const { Pool } = pg;

let pool: pg.Pool | null = null;
let db: NodePgDatabase<typeof schema> | null = null;

export function getPool(): pg.Pool {
	if (!pool) {
		const databaseUrl = env.DATABASE_URL;
		if (!databaseUrl) {
			throw new Error('DATABASE_URL environment variable is not set');
		}
		pool = new Pool({
			connectionString: databaseUrl,
			max: 20, // Maximum pool size
			idleTimeoutMillis: 30000, // Close idle connections after 30s
			connectionTimeoutMillis: 5000 // Fail fast on connection issues
		});

		// Handle connection errors
		pool.on('error', (err) => {
			console.error('[DB] Unexpected pool error:', err);
		});
	}
	return pool;
}

export function getDb(): NodePgDatabase<typeof schema> {
	if (!db) {
		db = drizzle(getPool(), { schema });
	}
	return db;
}

/**
 * Gracefully close the database connection pool
 * Call this during server shutdown to prevent connection leaks
 */
export async function closePool(): Promise<void> {
	if (pool) {
		console.log('[DB] Closing connection pool...');
		await pool.end();
		pool = null;
		db = null;
		console.log('[DB] Connection pool closed');
	}
}

// Handle process termination signals for graceful shutdown
if (typeof process !== 'undefined') {
	const shutdown = async (signal: string) => {
		console.log(`[DB] Received ${signal}, initiating graceful shutdown...`);
		await closePool();
		process.exit(0);
	};

	process.on('SIGTERM', () => shutdown('SIGTERM'));
	process.on('SIGINT', () => shutdown('SIGINT'));
}