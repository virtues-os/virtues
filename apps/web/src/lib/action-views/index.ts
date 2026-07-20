/**
 * Action view registry — Vite-glob loader for view-runtime action UIs.
 *
 * View-runtime actions are self-contained under `applets/<name>/`. Their UI
 * lives next to the manifest at `applets/<name>/ui/`:
 *
 *   applets/<name>/manifest.toml          (declares config.view.name)
 *   applets/<name>/ui/Card.svelte         (optional — overrides ActionCard)
 *   applets/<name>/ui/Detail.svelte       (optional — overrides ActionDetailView)
 *   applets/<name>/ui/Output.svelte       (future — for run-output rendering)
 *
 * The host web app discovers them at build time via `import.meta.glob` and
 * exposes `loadCard(name)` / `loadDetail(name)`. Eager loading is fine for
 * v1 — view actions are small and few. Switch to lazy when the count grows.
 *
 * The folder name under `applets/` is the lookup key. It does NOT have to
 * match the action's id_prefix; manifests can declare any view name they want,
 * and multiple actions can share a single view bundle.
 */

import type { Component } from 'svelte';

// Glob is relative to this file. Path walks up to the repo root and into
// `applets/<name>/ui/`:
//   action-views/ → lib/ → src/ → web/ → apps/ → <repo root>
const cardModules = import.meta.glob<{ default: Component }>(
	'../../../../../applets/*/ui/Card.svelte',
	{ eager: true }
);
const detailModules = import.meta.glob<{ default: Component }>(
	'../../../../../applets/*/ui/Detail.svelte',
	{ eager: true }
);

/** Extract the action folder name (`hello_world`) from a glob key
 * (`../../../../../applets/hello_world/ui/Card.svelte`).
 */
function nameFromPath(path: string): string | null {
	const m = path.match(/\/applets\/([^/]+)\/ui\//);
	return m ? m[1] : null;
}

function buildIndex(
	registry: Record<string, { default: Component }>
): Map<string, Component> {
	const out = new Map<string, Component>();
	for (const [path, mod] of Object.entries(registry)) {
		const name = nameFromPath(path);
		if (name) out.set(name, mod.default);
	}
	return out;
}

const cardIndex = buildIndex(cardModules);
const detailIndex = buildIndex(detailModules);

/** Load the Card component for a view action. Returns null if no override exists. */
export function loadCard(name: string | null | undefined): Component | null {
	if (!name) return null;
	return cardIndex.get(name) ?? null;
}

/** Load the Detail component for a view action. Returns null if no override exists. */
export function loadDetail(name: string | null | undefined): Component | null {
	if (!name) return null;
	return detailIndex.get(name) ?? null;
}

/** All registered view-action names. Useful for registry diagnostics / dev UI. */
export function registeredViewNames(): string[] {
	const names = new Set<string>([...cardIndex.keys(), ...detailIndex.keys()]);
	return [...names].sort();
}
