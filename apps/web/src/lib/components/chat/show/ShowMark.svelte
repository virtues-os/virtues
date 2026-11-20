<!--
	A mark drawn by a module from the chart kit (./marks/), mounted on its
	element with the whole show result and destroyed on teardown. Show.svelte
	already draws the title and the rows footer, so the module draws neither.

	Each module loads the first time a reply draws its mark. The kit is large,
	and the chat view is on the first paint of every launch, so importing the
	modules statically would make every launch download every mark.

	While the module loads, the placeholder holds the chart's place, so the
	reply doesn't jump when the chart lands. A module that fails to load says
	so in place of the chart.
-->
<script lang="ts">
	import { tick } from "svelte";
	import Drawing from "./Drawing.svelte";
	import type { KitMark, ShowOutput } from "./show";

	interface Props {
		out: ShowOutput & { mark: KitMark };
		onOpenRows?: (ref: string) => void;
	}
	let { out, onOpenRows }: Props = $props();

	type MarkModule = { mount: (el: HTMLElement, output: unknown, options: object) => { destroy(): void } };

	const MODULES: Record<KitMark, () => Promise<MarkModule>> = {
		baseline: () => import("./marks/usual.js"),
		cumulative: () => import("./marks/months.js"),
		interrupted: () => import("./marks/split.js"),
		states: () => import("./marks/ribbons.js"),
		multiples: () => import("./marks/multiples.js"),
		dumbbell: () => import("./marks/compare.js"),
		list: () => import("./marks/list.js"),
		stack: () => import("./marks/stack.js"),
		share: () => import("./marks/share.js"),
		waterfall: () => import("./marks/waterfall.js"),
		ridgeline: () => import("./marks/ridges.js"),
		diurnal: () => import("./marks/band.js"),
		actogram: () => import("./marks/loupe.js"),
		dotplot: () => import("./marks/spread.js"),
		scatter: () => import("./marks/cloud.js"),
		area: () => import("./marks/area.js"),
		aligned: () => import("./marks/aligned.js"),
		swimlanes: () => import("./marks/moments.js"),
		lollipop: () => import("./marks/renewals.js"),
		itinerary: () => import("./marks/tripline.js"),
		route: () => import("./marks/path.js"),
		density: () => import("./marks/stipple.js"),
	};

	let el: HTMLDivElement;
	let phase = $state<"loading" | "drawn" | "failed">("loading");
	$effect(() => {
		const output = out;
		const options = { foot: false, onOpenRows };
		let handle: { destroy(): void } | undefined;
		let gone = false;
		phase = "loading";
		Promise.all([MODULES[output.mark](), import("./marks/kit.css")])
			.then(async ([mod]) => {
				if (gone) return;
				// The placeholder goes first, so the mark measures its full
				// width. tick() resolves before the next paint, so no frame
				// shows the empty element.
				phase = "drawn";
				await tick();
				if (!gone) handle = mod.mount(el, output, options);
			})
			.catch((e) => {
				console.error(`The ${output.mark} chart could not load:`, e);
				if (!gone) phase = "failed";
			});
		return () => {
			gone = true;
			handle?.destroy();
		};
	});
</script>

{#if phase === "loading"}
	<Drawing />
{:else if phase === "failed"}
	<p class="failed">This chart didn't load. Reload the page to try again.</p>
{/if}
<div class="mark" bind:this={el}></div>

<style>
	.mark {
		width: 100%;
	}
	.failed {
		margin: 0;
		font-family: var(--font-sans);
		font-size: var(--text-small);
		color: var(--color-foreground-muted);
	}
</style>
