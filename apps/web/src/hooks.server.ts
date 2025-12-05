/**
 * SvelteKit server hooks
 * Runs once when the server starts
 */
import { sequence } from '@sveltejs/kit/hooks';
import { env } from '$env/dynamic/private';
import { handle as authHandle } from '$lib/server/auth';
import { getPool } from '$lib/server/db';
import { initializeTools } from '$lib/server/tools/loader';
import { cleanupExpiredSessions, cleanupExpiredTokens } from '$lib/server/auth-adapter';
import { checkAuthRateLimit } from '$lib/server/rate-limit';
import type { Handle } from '@sveltejs/kit';

/**
 * Validate required environment variables at startup
 * Fast-fail to prevent confusing errors later
 */
function validateRequiredEnv(): void {
	const required = ['DATABASE_URL', 'AUTH_SECRET'];
	const missing = required.filter((key) => !env[key]);

	if (missing.length > 0) {
		throw new Error(`Missing required environment variables: ${missing.join(', ')}`);
	}

	// OWNER_EMAIL is required in production
	if (env.NODE_ENV === 'production' && !env.OWNER_EMAIL) {
		throw new Error('OWNER_EMAIL is required in production');
	}
}

/**
 * Server initialization
 * Runs once when the SvelteKit server starts
 */
let initialized = false;

async function initializeServer() {
	if (initialized) {
		console.log('[Server] Already initialized, skipping...');
		return;
	}

	// Validate environment variables first (fast-fail)
	validateRequiredEnv();

	console.log('[Server] ========================================');
	console.log('[Server] Starting Virtues Web Server');
	console.log('[Server] ========================================');

	try {
		// Initialize database pool
		console.log('[Server] Initializing database connection...');
		const pool = getPool();
		console.log('[Server] ✓ Database connection ready');

		// Initialize tools
		const mcpServerUrl = env.RUST_API_URL || 'http://localhost:8000';
		console.log(`[Server] Initializing tools with MCP server at ${mcpServerUrl}...`);
		await initializeTools(pool, `${mcpServerUrl}/mcp`);
		console.log('[Server] ✓ Tools initialized');

		// Initialize agent registry
		console.log('[Server] Initializing agent registry...');
		const { initializeAgents } = await import('$lib/server/agents/registry');
		await initializeAgents('User'); // Default username, will be overridden per-request
		console.log('[Server] ✓ Agent registry ready');

		// Cleanup expired sessions and tokens (runs on startup)
		console.log('[Server] Cleaning up expired auth data...');
		const [sessionsDeleted, tokensDeleted] = await Promise.all([
			cleanupExpiredSessions(),
			cleanupExpiredTokens()
		]);
		if (sessionsDeleted > 0 || tokensDeleted > 0) {
			console.log(
				`[Server] ✓ Cleaned up ${sessionsDeleted} expired sessions, ${tokensDeleted} expired tokens`
			);
		}

		initialized = true;
		console.log('[Server] ========================================');
		console.log('[Server] ✅ Virtues Web Server Ready');
		console.log('[Server] ========================================');
	} catch (error) {
		console.error('[Server] ❌ Failed to initialize server:', error);
		throw error;
	}
}

/**
 * Handle hook for incoming requests
 * This runs on every request, but initialization only happens once
 */
const initHandle: Handle = async ({ event, resolve }) => {
	// Initialize on first request (in dev mode, this runs per request until stable)
	if (!initialized) {
		await initializeServer();
	}

	// Continue with request handling
	return resolve(event);
};

/**
 * Rate limit auth endpoints to prevent brute force attacks
 * Applies to POST requests to /auth/* (signin, callback, etc.)
 */
const rateLimitHandle: Handle = async ({ event, resolve }) => {
	const { url, request } = event;

	// Only rate limit POST requests to auth endpoints
	if (request.method === 'POST' && url.pathname.startsWith('/auth/')) {
		// Get client IP from x-forwarded-for (behind proxy) or connection
		const forwardedFor = request.headers.get('x-forwarded-for');
		const ip = forwardedFor?.split(',')[0]?.trim() || event.getClientAddress();

		const { allowed, retryAfter } = checkAuthRateLimit(ip);

		if (!allowed) {
			console.log(`[Auth] Rate limited IP: ${ip}, retry after ${retryAfter}s`);
			// Return 429 Too Many Requests
			return new Response(
				JSON.stringify({
					error: 'Too many login attempts. Please try again later.'
				}),
				{
					status: 429,
					headers: {
						'Content-Type': 'application/json',
						'Retry-After': String(retryAfter)
					}
				}
			);
		}
	}

	return resolve(event);
};

/**
 * Protect all /api/* routes - require authenticated session
 * Returns 401 for unauthenticated requests to API endpoints
 *
 * Must come AFTER authHandle in sequence so event.locals.auth() is available.
 * Per Auth.js best practices: https://authjs.dev/getting-started/session-management/protecting
 */
const apiAuthHandle: Handle = async ({ event, resolve }) => {
	// Only protect /api/* routes
	if (event.url.pathname.startsWith('/api/')) {
		const session = await event.locals.auth();

		if (!session?.user) {
			return new Response(JSON.stringify({ error: 'Unauthorized' }), {
				status: 401,
				headers: { 'Content-Type': 'application/json' }
			});
		}
	}

	return resolve(event);
};

// Combine initialization, rate limiting, auth, and API protection hooks
export const handle = sequence(initHandle, rateLimitHandle, authHandle, apiAuthHandle);
