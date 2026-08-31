import { redirect } from '@sveltejs/kit';
import type { PageLoad } from './$types';

// `/setup` moved to `/onboarding` (2026-08-13), which itself shrank to the
// founder's letter and moved to `/founders-letter` (2026-08-31) — everything
// else became the getting-started page inside the app. This redirects straight
// to the letter rather than chaining through /onboarding's own redirect.
//
// Kept as a redirect rather than deleted: the box's own copy points here, the
// URL is linked from the app, and SPA delivery is OTA — a bundle baked before
// the rename can meet a box after it.
export const load: PageLoad = () => {
	throw redirect(308, '/founders-letter');
};
