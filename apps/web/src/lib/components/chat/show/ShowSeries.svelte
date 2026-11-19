<!--
	A `show` of numbers whose figures carry the rows behind them (`series`).
	Nothing is drawn where the call sits: the figures stay in the sentence
	that quotes them. Once the turn ends, the drill-down figure (number.js)
	makes each one open its series under that paragraph, or the sparkline
	(spark.js, mark spark) sets a small trend after each.

	The figures are the links the model wrote to each series' ref, which
	Markdown.svelte renders as `[data-vk-figure]` spans; without them, the
	module finds each figure in the next paragraph by its value. When it finds
	none, `onUnplaced` asks Show to draw the plain row of figures instead.

	The two modules load the first time a reply uses one, so the chart kit
	stays off the first paint of every launch (see ShowMark.svelte).
-->
<script lang="ts">
	import { parseToolRef } from "$lib/utils/refRoutes";
	import type { ShowOutput } from "./show";

	interface Props {
		out: ShowOutput;
		/** The turn is still arriving, so the sentence may not be there yet. */
		working: boolean;
		onOpenRows?: (ref: string) => void;
		onUnplaced: () => void;
	}
	let { out, working, onOpenRows, onUnplaced }: Props = $props();

	let anchor: HTMLSpanElement;

	/** The first paragraph or list item after `from` in the message. */
	function nextBlock(root: Element, from: Element): HTMLElement | null {
		for (const el of root.querySelectorAll<HTMLElement>("p, li")) {
			if (from.compareDocumentPosition(el) & Node.DOCUMENT_POSITION_FOLLOWING) return el;
		}
		return null;
	}

	$effect(() => {
		if (working || !anchor) return;
		const root = anchor.closest("[data-message-id]") ?? anchor.parentElement;
		const callId = out.ref ? parseToolRef(out.ref)?.callId : undefined;
		if (!root) return;
		const marked = callId ? [...root.querySelectorAll<HTMLElement>(`[data-vk-call="${CSS.escape(callId)}"]`)] : [];
		const reply = marked[0]?.closest<HTMLElement>("p, li") ?? nextBlock(root, anchor);
		if (!reply) {
			onUnplaced();
			return;
		}
		const output = out;
		const options = { reply, figures: marked.length ? marked : undefined, onOpenRows };
		let el: HTMLDivElement | undefined;
		let handle: { destroy(): void } | undefined;
		let gone = false;
		const loading = output.mark === "spark" ? import("./marks/spark.js") : import("./marks/number.js");
		Promise.all([loading, import("./marks/kit.css")])
			.then(([mod]) => {
				if (gone) return;
				el = document.createElement("div");
				reply.after(el);
				let placed: number;
				if (output.mark === "spark") {
					const sparks = (mod as typeof import("./marks/spark.js")).mount(el, output, options);
					handle = sparks;
					placed = sparks.sparks.length;
				} else {
					handle = (mod as typeof import("./marks/number.js")).mount(el, output, options);
					placed = root.querySelectorAll(".vk-figure").length;
				}
				if (!placed) {
					handle.destroy();
					handle = undefined;
					el.remove();
					onUnplaced();
				}
			})
			.catch((e) => {
				console.error("The figures' module could not load:", e);
				if (!gone) onUnplaced();
			});
		return () => {
			gone = true;
			handle?.destroy();
			el?.remove();
		};
	});
</script>

<span class="anchor" bind:this={anchor} aria-hidden="true"></span>

<style>
	.anchor {
		display: none;
	}
</style>
