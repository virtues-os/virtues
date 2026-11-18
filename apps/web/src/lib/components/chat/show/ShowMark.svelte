<!--
	A chart mark drawn by a module from the chart kit (./marks/), mounted on its
	element with the whole show result and destroyed on teardown. Show.svelte
	already draws the title and the rows footer, so the module draws neither.
-->
<script lang="ts">
	import "./marks/kit.css";
	import * as usual from "./marks/usual.js";
	import * as months from "./marks/months.js";
	import * as split from "./marks/split.js";
	import * as ribbons from "./marks/ribbons.js";
	import type { KitMark, ShowOutput } from "./show";

	interface Props {
		out: ShowOutput & { mark: KitMark };
		onOpenRows?: (ref: string) => void;
	}
	let { out, onOpenRows }: Props = $props();

	const MODULES: Record<KitMark, { mount: (el: HTMLElement, output: unknown, options: object) => { destroy(): void } }> = {
		baseline: usual,
		cumulative: months,
		interrupted: split,
		states: ribbons,
	};

	let el: HTMLDivElement;
	$effect(() => {
		const handle = MODULES[out.mark].mount(el, out, { foot: false, onOpenRows });
		return () => handle.destroy();
	});
</script>

<div class="mark" bind:this={el}></div>

<style>
	.mark {
		width: 100%;
	}
</style>
