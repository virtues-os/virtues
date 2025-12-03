import { json } from '@sveltejs/kit';
import type { RequestHandler } from './$types';
import { getPool } from '$lib/server/db';

interface LocationInput {
	formatted_address: string;
	latitude: number;
	longitude: number;
	google_place_id?: string;
	label: string;
}

interface LocationsPayload {
	locations: LocationInput[];
}

/**
 * POST /api/profile/locations
 *
 * Saves known locations as entities_place records.
 * Used during onboarding to store user's significant places.
 */
export const POST: RequestHandler = async ({ request }) => {
	const pool = getPool();

	try {
		const body: LocationsPayload = await request.json();

		if (!body.locations || !Array.isArray(body.locations)) {
			return json({ error: 'Invalid request: locations array required' }, { status: 400 });
		}

		const createdIds: string[] = [];
		let firstLocationId: string | null = null;

		for (const location of body.locations) {
			// Validate required fields
			if (!location.formatted_address || !location.label || location.latitude == null || location.longitude == null) {
				continue;
			}

			// Build metadata with additional details
			const metadata = {
				formatted_address: location.formatted_address,
				google_place_id: location.google_place_id,
				is_known_location: true,
				source: 'onboarding'
			};

			// Insert into entities_place with PostGIS geography
			const result = await pool.query(
				`INSERT INTO data.entities_place (
					canonical_name,
					geo_center,
					metadata
				) VALUES (
					$1,
					ST_SetSRID(ST_MakePoint($2, $3), 4326)::geography,
					$4
				)
				RETURNING id`,
				[location.label, location.longitude, location.latitude, metadata]
			);

			if (result.rows[0]?.id) {
				createdIds.push(result.rows[0].id);

				// Track first location for potential home_place_id
				if (!firstLocationId) {
					firstLocationId = result.rows[0].id;
				}
			}
		}

		// Set the first location as home_place_id if not already set
		if (firstLocationId) {
			await pool.query(
				`UPDATE data.user_profile SET home_place_id = $1 WHERE home_place_id IS NULL`,
				[firstLocationId]
			);
		}

		return json({
			success: true,
			created: createdIds.length,
			ids: createdIds
		});
	} catch (error) {
		console.error('[/api/profile/locations] Error saving locations:', error);
		return json(
			{
				error: 'Failed to save locations',
				details: error instanceof Error ? error.message : 'Unknown error'
			},
			{ status: 500 }
		);
	}
};

/**
 * GET /api/profile/locations
 *
 * Retrieves user's known locations (places marked with is_known_location: true)
 */
export const GET: RequestHandler = async () => {
	const pool = getPool();

	try {
		const result = await pool.query(`
			SELECT
				id,
				canonical_name,
				ST_Y(geo_center::geometry) as latitude,
				ST_X(geo_center::geometry) as longitude,
				metadata
			FROM data.entities_place
			WHERE metadata->>'is_known_location' = 'true'
			ORDER BY created_at ASC
		`);

		const locations = result.rows.map((row) => ({
			id: row.id,
			label: row.canonical_name,
			latitude: row.latitude,
			longitude: row.longitude,
			formatted_address: row.metadata?.formatted_address,
			google_place_id: row.metadata?.google_place_id
		}));

		return json({ locations });
	} catch (error) {
		console.error('[/api/profile/locations] Error fetching locations:', error);
		return json(
			{
				error: 'Failed to fetch locations',
				details: error instanceof Error ? error.message : 'Unknown error'
			},
			{ status: 500 }
		);
	}
};
