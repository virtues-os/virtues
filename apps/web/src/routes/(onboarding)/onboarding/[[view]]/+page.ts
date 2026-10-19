import { redirect } from '@sveltejs/kit';
import type { PageLoad } from './$types';

// `/onboarding` left the vocabulary (2026-08-31). The flow it named is Setup
// now (2026-09-24), at /setup. Kept as a redirect rather
// than deleted — the box's own copy linked here, old step URLs
// (/onboarding/introductions, /sources, /you) are in browser histories, and
// SPA delivery is OTA: a bundle baked before the rename can meet a box after
// it. The [[view]] param exists solely so every old step URL lands here too.
export const load: PageLoad = () => {
	throw redirect(308, '/setup');
};
