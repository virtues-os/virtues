import { getPool } from '$lib/server/db';
import { redirect } from '@sveltejs/kit';
import type { LayoutServerLoad } from './$types';

const SINGLETON_ID = '00000000-0000-0000-0000-000000000001';

export const load: LayoutServerLoad = async (event) => {
	const session = await event.locals.auth();

	// Redirect to login if not authenticated
	if (!session?.user) {
		throw redirect(303, '/login');
	}

	// Check if user needs to complete onboarding
	const pool = getPool();
	const result = await pool.query(
		`SELECT is_onboarding FROM data.user_profile WHERE id = $1`,
		[SINGLETON_ID]
	);

	if (result.rows.length > 0 && result.rows[0].is_onboarding === true) {
		throw redirect(303, '/onboarding/welcome');
	}

	return {
		session
	};
};
