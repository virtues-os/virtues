/**
 * Custom PostgreSQL adapter for Auth.js
 *
 * Uses the app schema for auth tables:
 * - app.auth_user
 * - app.auth_session
 * - app.auth_verification_token
 */
import type { Adapter, AdapterSession, AdapterUser, VerificationToken } from '@auth/core/adapters';
import { getPool } from './db';

export function createPostgresAdapter(): Adapter {
	const pool = getPool();

	return {
		async createUser(user) {
			const result = await pool.query<AdapterUser>(
				`INSERT INTO app.auth_user (email, email_verified)
				 VALUES ($1, $2)
				 RETURNING id, email, email_verified AS "emailVerified", NULL AS name, NULL AS image`,
				[user.email, user.emailVerified]
			);
			return result.rows[0];
		},

		async getUser(id) {
			const result = await pool.query<AdapterUser>(
				`SELECT id, email, email_verified AS "emailVerified", NULL AS name, NULL AS image
				 FROM app.auth_user WHERE id = $1`,
				[id]
			);
			return result.rows[0] ?? null;
		},

		async getUserByEmail(email) {
			const result = await pool.query<AdapterUser>(
				`SELECT id, email, email_verified AS "emailVerified", NULL AS name, NULL AS image
				 FROM app.auth_user WHERE email = $1`,
				[email]
			);
			return result.rows[0] ?? null;
		},

		async getUserByAccount() {
			// We only use email provider, no linked accounts
			return null;
		},

		async updateUser(user) {
			const result = await pool.query<AdapterUser>(
				`UPDATE app.auth_user
				 SET email = COALESCE($2, email),
				     email_verified = COALESCE($3, email_verified)
				 WHERE id = $1
				 RETURNING id, email, email_verified AS "emailVerified", NULL AS name, NULL AS image`,
				[user.id, user.email, user.emailVerified]
			);
			return result.rows[0];
		},

		async deleteUser(userId) {
			await pool.query('DELETE FROM app.auth_user WHERE id = $1', [userId]);
		},

		async linkAccount(account) {
			// We only use email provider, no linked accounts needed
			return account;
		},

		async unlinkAccount() {
			// We only use email provider, no linked accounts
		},

		async createSession(session) {
			const result = await pool.query<AdapterSession>(
				`INSERT INTO app.auth_session (session_token, user_id, expires)
				 VALUES ($1, $2, $3)
				 RETURNING id, session_token AS "sessionToken", user_id AS "userId", expires`,
				[session.sessionToken, session.userId, session.expires]
			);
			return result.rows[0];
		},

		async getSessionAndUser(sessionToken) {
			const result = await pool.query<{
				sessionToken: string;
				odataUserId: string;
				expires: Date;
				odataId: string;
				email: string;
				emailVerified: Date | null;
			}>(
				`SELECT
				   s.session_token AS "sessionToken", s.user_id AS "odataUserId", s.expires,
				   u.id AS "odataId", u.email, u.email_verified AS "emailVerified"
				 FROM app.auth_session s
				 JOIN app.auth_user u ON s.user_id = u.id
				 WHERE s.session_token = $1 AND s.expires > NOW()`,
				[sessionToken]
			);

			if (!result.rows[0]) return null;

			const row = result.rows[0];
			return {
				session: {
					sessionToken: row.sessionToken,
					userId: row.odataUserId,
					expires: row.expires
				},
				user: {
					id: row.odataId,
					email: row.email,
					emailVerified: row.emailVerified,
					name: null,
					image: null
				}
			};
		},

		async updateSession(session) {
			const result = await pool.query<AdapterSession>(
				`UPDATE app.auth_session
				 SET expires = COALESCE($2, expires)
				 WHERE session_token = $1
				 RETURNING id, session_token AS "sessionToken", user_id AS "userId", expires`,
				[session.sessionToken, session.expires]
			);
			return result.rows[0] ?? null;
		},

		async deleteSession(sessionToken) {
			await pool.query('DELETE FROM app.auth_session WHERE session_token = $1', [sessionToken]);
		},

		async createVerificationToken(token) {
			const result = await pool.query<VerificationToken>(
				`INSERT INTO app.auth_verification_token (identifier, token, expires)
				 VALUES ($1, $2, $3)
				 RETURNING identifier, token, expires`,
				[token.identifier, token.token, token.expires]
			);
			return result.rows[0];
		},

		async useVerificationToken({ identifier, token }) {
			const result = await pool.query<VerificationToken>(
				`DELETE FROM app.auth_verification_token
				 WHERE identifier = $1 AND token = $2
				 RETURNING identifier, token, expires`,
				[identifier, token]
			);
			return result.rows[0] ?? null;
		}
	};
}

/**
 * Cleanup expired sessions from the database
 * Call this during server startup or via cron job
 * @returns Number of deleted sessions
 */
export async function cleanupExpiredSessions(): Promise<number> {
	const pool = getPool();
	const result = await pool.query(`DELETE FROM app.auth_session WHERE expires < NOW()`);
	return result.rowCount ?? 0;
}

/**
 * Cleanup expired verification tokens from the database
 * Call this during server startup or via cron job
 * @returns Number of deleted tokens
 */
export async function cleanupExpiredTokens(): Promise<number> {
	const pool = getPool();
	const result = await pool.query(`DELETE FROM app.auth_verification_token WHERE expires < NOW()`);
	return result.rowCount ?? 0;
}
