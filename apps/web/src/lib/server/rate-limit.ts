/**
 * API rate limiting and usage tracking
 *
 * Hardcoded rate limits enforced per-instance (not per-user).
 * In an instance-per-user deployment model, each user gets their own database,
 * so these limits apply to each user's instance independently.
 */

import { getPool } from './db';

// ============================================================================
// Auth Rate Limiting (in-memory, for login endpoint)
// ============================================================================

interface AuthRateLimitRecord {
	count: number;
	resetAt: number;
}

const authAttempts = new Map<string, AuthRateLimitRecord>();

const AUTH_WINDOW_MS = 15 * 60 * 1000; // 15 minutes
const AUTH_MAX_ATTEMPTS = 5;

/**
 * Check if an IP is rate limited for auth/login attempts
 *
 * Uses in-memory storage (no DB required) with silent failure
 * to prevent enumeration attacks.
 *
 * @param ip - The IP address to check
 * @returns Object with `allowed` boolean and optional `retryAfter` in seconds
 */
export function checkAuthRateLimit(ip: string): { allowed: boolean; retryAfter?: number } {
	const now = Date.now();
	const record = authAttempts.get(ip);

	// Clean up expired records periodically (1% chance per request)
	if (Math.random() < 0.01) {
		for (const [key, value] of authAttempts) {
			if (value.resetAt < now) {
				authAttempts.delete(key);
			}
		}
	}

	// No record or expired - allow and start new window
	if (!record || record.resetAt < now) {
		authAttempts.set(ip, { count: 1, resetAt: now + AUTH_WINDOW_MS });
		return { allowed: true };
	}

	// Check if limit exceeded
	if (record.count >= AUTH_MAX_ATTEMPTS) {
		const retryAfter = Math.ceil((record.resetAt - now) / 1000);
		return { allowed: false, retryAfter };
	}

	// Increment count and allow
	record.count++;
	return { allowed: true };
}

/**
 * Get current auth rate limit status for an IP (for debugging/admin)
 */
export function getAuthRateLimitStatus(ip: string): {
	attempts: number;
	remaining: number;
	resetIn?: number;
} {
	const now = Date.now();
	const record = authAttempts.get(ip);

	if (!record || record.resetAt < now) {
		return { attempts: 0, remaining: AUTH_MAX_ATTEMPTS };
	}

	return {
		attempts: record.count,
		remaining: Math.max(0, AUTH_MAX_ATTEMPTS - record.count),
		resetIn: Math.ceil((record.resetAt - now) / 1000)
	};
}

// ============================================================================
// API Rate Limiting (database-backed, for chat/API endpoints)
// ============================================================================
import { env } from '$env/dynamic/private';
import type { Pool } from 'pg';

export class RateLimitError extends Error {
	constructor(
		message: string,
		public readonly current: number,
		public readonly limit: number,
		public readonly resetAt?: Date
	) {
		super(message);
		this.name = 'RateLimitError';
	}
}

/**
 * Rate limits applied to all instances
 * Configurable via environment variables with safe defaults
 */
export interface RateLimits {
	chatRequestsPerDay: number;
	chatTokensPerDay: number;
	backgroundJobsPerDay: number;
}

/**
 * Parse environment variable with validation
 * @param key Environment variable name
 * @param defaultValue Default value if not set
 * @param min Minimum allowed value (enforces safety)
 */
function parseEnvInt(key: string, defaultValue: number, min: number): number {
	const value = env[key];
	if (!value) return defaultValue;

	const parsed = parseInt(value, 10);
	if (Number.isNaN(parsed)) {
		console.warn(`Invalid value for ${key}: "${value}", using default ${defaultValue}`);
		return defaultValue;
	}

	// Enforce minimum to prevent disabling limits
	if (parsed < min) {
		console.warn(
			`Value for ${key} (${parsed}) is below minimum (${min}), using minimum instead`
		);
		return min;
	}

	return parsed;
}

/**
 * Monthly token limits based on TIER env var
 * - starter: 4M tokens/month (~$6 budget)
 * - pro: 15M tokens/month (~$20 budget)
 */
function getMonthlyTokenLimit(): number {
	const tier = env.TIER || 'starter';
	return tier === 'pro' ? 15_000_000 : 4_000_000;
}

export const DEFAULT_RATE_LIMITS: RateLimits = {
	chatRequestsPerDay: parseEnvInt('RATE_LIMIT_CHAT_DAILY', 1000, 1),
	chatTokensPerDay: parseEnvInt('RATE_LIMIT_TOKENS_DAILY', 500_000, 1000),
	backgroundJobsPerDay: parseEnvInt('RATE_LIMIT_JOBS_DAILY', 100, 1)
};

export interface TokenUsage {
	input: number;
	output: number;
	model: string;
}

/**
 * Rate limit information for HTTP headers
 */
export interface RateLimitInfo {
	limit: number;
	remaining: number;
	reset: number; // Unix timestamp
	retryAfter?: number; // Seconds until reset (only on limit exceeded)
}

/**
 * Get rate limit headers for HTTP response
 * Call this after checkRateLimit to add standard rate limit headers
 */
export function getRateLimitHeaders(info: RateLimitInfo): Record<string, string> {
	const headers: Record<string, string> = {
		'X-RateLimit-Limit': String(info.limit),
		'X-RateLimit-Remaining': String(Math.max(0, info.remaining)),
		'X-RateLimit-Reset': String(info.reset)
	};

	if (info.retryAfter !== undefined) {
		headers['Retry-After'] = String(info.retryAfter);
	}

	return headers;
}

/**
 * Check if an endpoint request would exceed rate limits
 *
 * SECURITY: Uses atomic check-then-increment to prevent race conditions.
 * All limits are checked in a single query BEFORE incrementing counts.
 * This prevents concurrent requests from bypassing limits.
 *
 * Uses server-side timestamps (UTC) to prevent client tampering.
 *
 * @param endpoint - The endpoint being called (e.g., 'chat', 'background_job')
 * @param estimatedTokens - Optional estimate of tokens this request will use (for pre-check)
 * @param limits - Rate limits to apply
 * @returns Rate limit info for response headers
 * @throws {RateLimitError} if limits would be exceeded
 */
export async function checkRateLimit(
	endpoint: string,
	estimatedTokens: number = 0,
	limits: RateLimits = DEFAULT_RATE_LIMITS
): Promise<RateLimitInfo> {
	const pool = getPool();
	const now = new Date();
	const dayBucket = getDayBucket(now);
	const monthBucket = getMonthBucket(now);

	// Get the limit for this endpoint
	const dailyRequestLimit =
		endpoint === 'chat'
			? limits.chatRequestsPerDay
			: endpoint === 'background_job'
				? limits.backgroundJobsPerDay
				: Number.MAX_SAFE_INTEGER;

	const monthlyLimit = getMonthlyTokenLimit();

	// ATOMIC: Check ALL limits in a single query BEFORE incrementing
	// This prevents race conditions where concurrent requests bypass limits
	const checkResult = await pool.query<{
		daily_requests: number;
		daily_tokens: number;
		monthly_tokens: number;
		status: 'ok' | 'daily_request_limit' | 'daily_token_limit' | 'monthly_token_limit';
	}>(
		`
		WITH current_usage AS (
			SELECT
				COALESCE((
					SELECT request_count FROM app.api_usage
					WHERE endpoint = $1 AND day_bucket = $2
				), 0) as daily_requests,
				COALESCE((
					SELECT SUM(token_count) FROM app.api_usage
					WHERE day_bucket = $2
				), 0) as daily_tokens,
				COALESCE((
					SELECT tokens_used FROM app.llm_usage
					WHERE month = $3
				), 0) as monthly_tokens
		)
		SELECT
			daily_requests,
			daily_tokens::bigint,
			monthly_tokens::bigint,
			CASE
				WHEN daily_requests >= $4 THEN 'daily_request_limit'
				WHEN daily_tokens + $7 > $5 THEN 'daily_token_limit'
				WHEN monthly_tokens + $7 > $6 THEN 'monthly_token_limit'
				ELSE 'ok'
			END as status
		FROM current_usage
		`,
		[
			endpoint,
			dayBucket,
			monthBucket,
			dailyRequestLimit,
			limits.chatTokensPerDay,
			monthlyLimit,
			estimatedTokens
		]
	);

	const check = checkResult.rows[0];

	// Throw appropriate error if any limit is exceeded
	if (check.status === 'daily_request_limit') {
		const resetAt = new Date(`${dayBucket}T00:00:00.000Z`);
		resetAt.setUTCDate(resetAt.getUTCDate() + 1);
		throw new RateLimitError(
			`Daily rate limit exceeded: ${check.daily_requests}/${dailyRequestLimit} requests`,
			check.daily_requests,
			dailyRequestLimit,
			resetAt
		);
	}

	if (check.status === 'daily_token_limit') {
		const resetAt = new Date(`${dayBucket}T00:00:00.000Z`);
		resetAt.setUTCDate(resetAt.getUTCDate() + 1);
		throw new RateLimitError(
			`Daily token limit exceeded: ${check.daily_tokens}/${limits.chatTokensPerDay} tokens`,
			Number(check.daily_tokens),
			limits.chatTokensPerDay,
			resetAt
		);
	}

	if (check.status === 'monthly_token_limit') {
		const resetAt = new Date(`${monthBucket}T00:00:00.000Z`);
		resetAt.setUTCMonth(resetAt.getUTCMonth() + 1);
		const tier = env.TIER || 'starter';
		throw new RateLimitError(
			`Monthly token limit exceeded (${tier} tier): ${check.monthly_tokens}/${monthlyLimit} tokens`,
			Number(check.monthly_tokens),
			monthlyLimit,
			resetAt
		);
	}

	// Only NOW increment request count (after all checks pass)
	await pool.query(
		`
		INSERT INTO app.api_usage (endpoint, day_bucket, request_count)
		VALUES ($1, $2, 1)
		ON CONFLICT (endpoint, day_bucket)
		DO UPDATE SET
			request_count = app.api_usage.request_count + 1,
			updated_at = NOW()
		`,
		[endpoint, dayBucket]
	);

	// Calculate reset time (next day at midnight UTC)
	const resetAt = new Date(`${dayBucket}T00:00:00.000Z`);
	resetAt.setUTCDate(resetAt.getUTCDate() + 1);

	// Return rate limit info for response headers
	return {
		limit: dailyRequestLimit,
		remaining: dailyRequestLimit - check.daily_requests - 1, // -1 for this request
		reset: Math.floor(resetAt.getTime() / 1000)
	};
}

/**
 * Record token usage after a successful API call
 *
 * IMPORTANT: This function MUST be called after checkRateLimit() to ensure
 * the api_usage row exists. Calling this without checkRateLimit() will throw an error.
 *
 * Note: Request count is already incremented by checkRateLimit().
 * This function only updates token counts and cost estimates.
 *
 * Also records to LLM metering tables for multi-tenant billing:
 * - app.llm_usage: monthly aggregates
 * - app.llm_requests: individual request log
 *
 * @throws {Error} if no row exists (checkRateLimit not called first)
 * @throws {Error} if token counts exceed safe integer range
 */
export async function recordUsage(endpoint: string, tokens: TokenUsage): Promise<void> {
	const pool = getPool();
	const now = new Date();
	const dayBucket = getDayBucket(now);
	const monthBucket = getMonthBucket(now);
	const cost = calculateCost(tokens);
	const costCents = Math.round(cost * 100);

	// Validate token counts don't exceed safe integer range
	// PostgreSQL INTEGER max is 2,147,483,647
	// JavaScript MAX_SAFE_INTEGER is 9,007,199,254,740,991
	const totalTokens = tokens.input + tokens.output;
	if (
		totalTokens > Number.MAX_SAFE_INTEGER ||
		totalTokens > 2147483647 ||
		tokens.input > 2147483647 ||
		tokens.output > 2147483647
	) {
		throw new Error(
			`Token count exceeds safe range: input=${tokens.input}, output=${tokens.output}, total=${totalTokens}`
		);
	}

	// Update daily usage (request count already incremented by checkRateLimit)
	const result = await pool.query(
		`
		UPDATE app.api_usage
		SET
			token_count = token_count + $3,
			input_tokens = input_tokens + $4,
			output_tokens = output_tokens + $5,
			estimated_cost_usd = estimated_cost_usd + $6,
			updated_at = NOW()
		WHERE endpoint = $1 AND day_bucket = $2
		`,
		[endpoint, dayBucket, totalTokens, tokens.input, tokens.output, cost]
	);

	// Verify the UPDATE affected at least one row
	// If no rows were affected, it means checkRateLimit() wasn't called first
	if (result.rowCount === 0) {
		throw new Error(
			`Failed to record usage: no row found for endpoint="${endpoint}", day="${dayBucket}". ` +
				`Ensure checkRateLimit() is called before recordUsage().`
		);
	}

	// Record to LLM metering tables for multi-tenant billing
	// These are used by Atlas for billing calculations
	await Promise.all([
		// Update monthly aggregate
		pool.query(
			`
			INSERT INTO app.llm_usage (month, tokens_used, cost_cents)
			VALUES ($1, $2, $3)
			ON CONFLICT (month)
			DO UPDATE SET
				tokens_used = app.llm_usage.tokens_used + $2,
				cost_cents = app.llm_usage.cost_cents + $3
			`,
			[monthBucket, totalTokens, costCents]
		),
		// Log individual request
		pool.query(
			`
			INSERT INTO app.llm_requests (model, input_tokens, output_tokens)
			VALUES ($1, $2, $3)
			`,
			[tokens.model, tokens.input, tokens.output]
		)
	]);
}

/**
 * Get current usage statistics for display in UI
 */
export interface UsageStats {
	dailyRequests: number;
	dailyTokens: number;
	dailyCost: number;
	limits: RateLimits;
}

export async function getUsageStats(endpoint: string): Promise<UsageStats> {
	const pool = getPool();
	const now = new Date();
	const dayBucket = getDayBucket(now);
	const limits = DEFAULT_RATE_LIMITS;

	const [dailyUsage, totalDailyTokens, dailyCost] = await Promise.all([
		getDailyUsage(pool, endpoint, dayBucket),
		getTotalDailyTokens(pool, dayBucket),
		getDailyCost(pool, dayBucket)
	]);

	return {
		dailyRequests: dailyUsage.requestCount,
		dailyTokens: totalDailyTokens,
		dailyCost,
		limits
	};
}

// ============================================================================
// Helper Functions
// ============================================================================

function getDayBucket(date: Date): string {
	const d = new Date(date);
	d.setUTCHours(0, 0, 0, 0);
	// Return date as string in YYYY-MM-DD format to match PostgreSQL DATE type
	// This matches Rust's NaiveDate representation
	return d.toISOString().split('T')[0];
}

function getMonthBucket(date: Date): string {
	const d = new Date(date);
	d.setUTCHours(0, 0, 0, 0);
	d.setUTCDate(1); // First day of month
	// Return date as string in YYYY-MM-DD format (first of month)
	return d.toISOString().split('T')[0];
}

interface DailyUsage {
	requestCount: number;
	tokenCount: number;
}

async function getDailyUsage(pool: Pool, endpoint: string, dayBucket: string): Promise<DailyUsage> {
	const result = await pool.query(
		`
		SELECT
			COALESCE(request_count, 0) as request_count,
			COALESCE(token_count, 0) as token_count
		FROM app.api_usage
		WHERE endpoint = $1 AND day_bucket = $2
		`,
		[endpoint, dayBucket]
	);

	return {
		requestCount: result.rows[0]?.request_count ?? 0,
		tokenCount: result.rows[0]?.token_count ?? 0
	};
}

async function getTotalDailyTokens(pool: Pool, dayBucket: string): Promise<number> {
	const result = await pool.query(
		`
		SELECT COALESCE(SUM(token_count), 0)::int as total_tokens
		FROM app.api_usage
		WHERE day_bucket = $1
		`,
		[dayBucket]
	);

	return result.rows[0]?.total_tokens ?? 0;
}

async function getDailyCost(pool: Pool, dayBucket: string): Promise<number> {
	const result = await pool.query(
		`
		SELECT COALESCE(SUM(estimated_cost_usd), 0)::numeric as total_cost
		FROM app.api_usage
		WHERE day_bucket = $1
		`,
		[dayBucket]
	);

	return parseFloat(result.rows[0]?.total_cost ?? 0);
}

/**
 * Calculate estimated cost based on token usage and model
 * Pricing as of November 2024 (subject to change)
 */
function calculateCost(tokens: TokenUsage): number {
	const modelLower = tokens.model.toLowerCase();

	// Anthropic Claude pricing (per million tokens)
	let inputPrice = 3.0; // Default to Sonnet
	let outputPrice = 15.0;

	if (modelLower.includes('sonnet')) {
		if (modelLower.includes('4')) {
			inputPrice = 3.0;
			outputPrice = 15.0;
		} else {
			inputPrice = 3.0;
			outputPrice = 15.0;
		}
	} else if (modelLower.includes('haiku')) {
		inputPrice = 0.8;
		outputPrice = 4.0;
	} else if (modelLower.includes('opus')) {
		inputPrice = 15.0;
		outputPrice = 75.0;
	} else if (modelLower.includes('gpt')) {
		// OpenAI pricing estimates
		if (modelLower.includes('gpt-5')) {
			inputPrice = 5.0;
			outputPrice = 15.0;
		} else if (modelLower.includes('oss')) {
			inputPrice = 0.0; // Open source models via AI Gateway
			outputPrice = 0.0;
		} else {
			inputPrice = 2.5;
			outputPrice = 10.0;
		}
	} else if (modelLower.includes('gemini')) {
		inputPrice = 1.25;
		outputPrice = 5.0;
	}

	const inputCost = (tokens.input * inputPrice) / 1_000_000;
	const outputCost = (tokens.output * outputPrice) / 1_000_000;

	return inputCost + outputCost;
}
