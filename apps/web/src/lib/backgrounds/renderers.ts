/**
 * The renderers that draw a background style themselves, by the manifest's
 * `renderer` (agents/build/backgrounds.md, "Renderers"). Background.svelte
 * asks here for a renderer's component and loads it only when a setting
 * first needs it, so a renderer nobody has chosen costs nothing at launch.
 *
 * The `image` renderer is not here: it is drawn by Background.svelte itself,
 * because most people see it on the first frame. A renderer's thumbnail for
 * the pickers is in lib/backgrounds/controls.ts, which loads with them.
 *
 * Every component takes the picture's `params` and the `appearance`, fills
 * its parent, and keeps the motion rules in that doc.
 */
import type { Component } from 'svelte';
import type { Appearance, DrawnRenderer } from './resolve';

export interface DrawnProps {
	params: unknown;
	appearance: Appearance;
}

type Loader = () => Promise<Component<DrawnProps>>;

const LOADERS: Record<DrawnRenderer, Loader> = {
	dots: () => import('$lib/components/backgrounds/DotsField.svelte').then((m) => m.default as Component<DrawnProps>),
};

const loaded = new Map<DrawnRenderer, Promise<Component<DrawnProps>>>();

/** The component that draws a renderer's pictures, loaded once. */
export function loadRenderer(renderer: DrawnRenderer): Promise<Component<DrawnProps>> {
	let promise = loaded.get(renderer);
	if (!promise) {
		promise = LOADERS[renderer]();
		loaded.set(renderer, promise);
	}
	return promise;
}
