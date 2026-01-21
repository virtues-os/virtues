import { redirect } from '@sveltejs/kit';
import type { PageLoad } from './$types';

// Redirect /onboarding to /onboarding/welcome
export const load: PageLoad = async () => {
	throw redirect(302, '/onboarding/welcome');
};
