import { json } from '@sveltejs/kit';
import type { RequestHandler } from './$types';
import { getPool } from '$lib/server/db';

interface ProfilePayload {
	full_name?: string;
	preferred_name?: string;
	birth_date?: string;
	occupation?: string;
	employer?: string;
	theme?: string;
	crux?: string;
	is_onboarding?: boolean;
	onboarding_step?: number | null;
	// Technology onboarding step fields
	technology_vision?: string;
	pain_point_primary?: string;
	pain_point_secondary?: string | null;
	excited_features?: string[];
}

const SINGLETON_ID = '00000000-0000-0000-0000-000000000001';

/**
 * GET /api/profile
 *
 * Retrieves the user profile (singleton).
 */
export const GET: RequestHandler = async () => {
	const pool = getPool();

	try {
		const result = await pool.query(
			`SELECT
				id,
				full_name,
				preferred_name,
				birth_date,
				occupation,
				employer,
				theme,
				crux,
				is_onboarding,
				onboarding_step,
				axiology_complete,
				home_place_id,
				technology_vision,
				pain_point_primary,
				pain_point_secondary,
				excited_features,
				created_at,
				updated_at
			FROM data.user_profile
			WHERE id = $1`,
			[SINGLETON_ID]
		);

		if (result.rows.length === 0) {
			return json({ error: 'Profile not found' }, { status: 404 });
		}

		return json(result.rows[0]);
	} catch (error) {
		console.error('[/api/profile] Error fetching profile:', error);
		return json(
			{
				error: 'Failed to fetch profile',
				details: error instanceof Error ? error.message : 'Unknown error'
			},
			{ status: 500 }
		);
	}
};

/**
 * PUT /api/profile
 *
 * Updates the user profile (singleton).
 */
export const PUT: RequestHandler = async ({ request }) => {
	const pool = getPool();

	try {
		const body: ProfilePayload = await request.json();

		// Build dynamic UPDATE query based on provided fields
		const updates: string[] = [];
		const values: unknown[] = [];
		let paramIndex = 1;

		if (body.full_name !== undefined) {
			updates.push(`full_name = $${paramIndex++}`);
			values.push(body.full_name || null);
		}

		if (body.preferred_name !== undefined) {
			updates.push(`preferred_name = $${paramIndex++}`);
			values.push(body.preferred_name || null);
		}

		if (body.birth_date !== undefined) {
			updates.push(`birth_date = $${paramIndex++}`);
			values.push(body.birth_date || null);
		}

		if (body.occupation !== undefined) {
			updates.push(`occupation = $${paramIndex++}`);
			values.push(body.occupation || null);
		}

		if (body.employer !== undefined) {
			updates.push(`employer = $${paramIndex++}`);
			values.push(body.employer || null);
		}

		if (body.theme !== undefined) {
			updates.push(`theme = $${paramIndex++}`);
			values.push(body.theme || null);
		}

		if (body.crux !== undefined) {
			updates.push(`crux = $${paramIndex++}`);
			values.push(body.crux || null);
		}

		if (body.is_onboarding !== undefined) {
			updates.push(`is_onboarding = $${paramIndex++}`);
			values.push(body.is_onboarding);
		}

		if (body.onboarding_step !== undefined) {
			updates.push(`onboarding_step = $${paramIndex++}`);
			values.push(body.onboarding_step);
		}

		// Technology onboarding step fields
		if (body.technology_vision !== undefined) {
			updates.push(`technology_vision = $${paramIndex++}`);
			values.push(body.technology_vision || null);
		}

		if (body.pain_point_primary !== undefined) {
			updates.push(`pain_point_primary = $${paramIndex++}`);
			values.push(body.pain_point_primary || null);
		}

		if (body.pain_point_secondary !== undefined) {
			updates.push(`pain_point_secondary = $${paramIndex++}`);
			values.push(body.pain_point_secondary || null);
		}

		if (body.excited_features !== undefined) {
			updates.push(`excited_features = $${paramIndex++}`);
			values.push(body.excited_features || null);
		}

		if (updates.length === 0) {
			return json({ error: 'No fields to update' }, { status: 400 });
		}

		// Add updated_at timestamp
		updates.push(`updated_at = NOW()`);

		// Add WHERE clause parameter
		values.push(SINGLETON_ID);

		const query = `
			UPDATE data.user_profile
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
		console.error('[/api/profile] Error updating profile:', error);
		return json(
			{
				error: 'Failed to update profile',
				details: error instanceof Error ? error.message : 'Unknown error'
			},
			{ status: 500 }
		);
	}
};
