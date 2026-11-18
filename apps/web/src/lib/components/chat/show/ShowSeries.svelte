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
-->
<script lang="ts">
	import "./marks/kit.css";
	import * as number from "./marks/number.js";
	import * as spark from "./marks/spark.js";
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
		const el = document.createElement("div");
		reply.after(el);
		const options = { reply, figures: marked.length ? marked : undefined, onOpenRows };
		let handle: { destroy(): void };
		let placed: number;
		if (out.mark === "spark") {
			const sparks = spark.mount(el, out, options);
			handle = sparks;
			placed = sparks.sparks.length;
		} else {
			handle = number.mount(el, out, options);
			placed = root.querySelectorAll(".vk-figure").length;
		}
		if (!placed) {
			handle.destroy();
			el.remove();
			onUnplaced();
			return;
		}
		return () => {
			handle.destroy();
			el.remove();
		};
	});
</script>

<span class="anchor" bind:this={anchor} aria-hidden="true"></span>

<style>
	.anchor {
		display: none;
	}
</style>
