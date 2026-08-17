import { redirect } from '@sveltejs/kit';
import type { PageLoad } from './$types';

// `/setup` moved to `/onboarding` (2026-08-13). The two halves had swapped
// names: SETUP is the box coming up — pair, wifi, link, all of it driven from
// the desktop app before the SPA exists — and ONBOARDING is this, the part that
// happens inside the app once the box is reachable. The route group was already
// called `(onboarding)`; only the page inside it still said `setup`.
//
// Kept as a redirect rather than deleted: the box's own copy points here, the
// URL is linked from the app, and SPA delivery is OTA — a bundle baked before
// the rename can meet a box after it.
export const load: PageLoad = () => {
	throw redirect(308, '/onboarding');
};
