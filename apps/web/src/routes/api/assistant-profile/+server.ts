import { json } from '@sveltejs/kit';
import type { RequestHandler } from './$types';
import { getPool } from '$lib/server/db';

interface AssistantProfilePayload {
	assistant_name?: string;
	default_agent_id?: string;
	default_model_id?: string;
	enabled_tools?: Record<string, boolean>;
	ui_preferences?: Record<string, unknown>;
}

const SINGLETON_ID = '00000000-0000-0000-0000-000000000001';

/**
 * GET /api/assistant-profile
 *
 * Retrieves the assistant profile (singleton).
 */
export const GET: RequestHandler = async () => {
	const pool = getPool();

	try {
		const result = await pool.query(
			`SELECT
				id,
				assistant_name,
				default_agent_id,
				default_model_id,
				enabled_tools,
				ui_preferences,
				created_at,
				updated_at
			FROM app.assistant_profile
			WHERE id = $1`,
			[SINGLETON_ID]
		);

		if (result.rows.length === 0) {
			return json({ error: 'Assistant profile not found' }, { status: 404 });
		}

		return json(result.rows[0]);
	} catch (error) {
		console.error('[/api/assistant-profile] Error fetching profile:', error);
		return json(
			{
				error: 'Failed to fetch assistant profile',
				details: error instanceof Error ? error.message : 'Unknown error'
			},
			{ status: 500 }
		);
	}
};

/**
 * PUT /api/assistant-profile
 *
 * Updates the assistant profile (singleton).
 */
export const PUT: RequestHandler = async ({ request }) => {
	const pool = getPool();

	try {
		const body: AssistantProfilePayload = await request.json();

		// Build dynamic UPDATE query based on provided fields
		const updates: string[] = [];
		const values: unknown[] = [];
		let paramIndex = 1;

		if (body.assistant_name !== undefined) {
			updates.push(`assistant_name = $${paramIndex++}`);
			values.push(body.assistant_name || null);
		}

		if (body.default_agent_id !== undefined) {
			updates.push(`default_agent_id = $${paramIndex++}`);
			values.push(body.default_agent_id || null);
		}

		if (body.default_model_id !== undefined) {
			updates.push(`default_model_id = $${paramIndex++}`);
			values.push(body.default_model_id || null);
		}

		if (body.enabled_tools !== undefined) {
			updates.push(`enabled_tools = $${paramIndex++}`);
			values.push(JSON.stringify(body.enabled_tools));
		}

		if (body.ui_preferences !== undefined) {
			updates.push(`ui_preferences = $${paramIndex++}`);
			values.push(JSON.stringify(body.ui_preferences));
		}

		if (updates.length === 0) {
			return json({ error: 'No fields to update' }, { status: 400 });
		}

		// Add updated_at timestamp
		updates.push(`updated_at = NOW()`);

		// Add WHERE clause parameter
		values.push(SINGLETON_ID);

		const query = `
			UPDATE app.assistant_profile
			SET ${updates.join(', ')}
			WHERE id = $${paramIndex}
			RETURNING *
		`;

		const result = await pool.query(query, values);

		return json({
			success: true,
			profile: result.rows[0]
		});
	} catch (error) {
		console.error('[/api/assistant-profile] Error updating profile:', error);
		return json(
			{
				error: 'Failed to update assistant profile',
				details: error instanceof Error ? error.message : 'Unknown error'
			},
			{ status: 500 }
		);
	}
};
