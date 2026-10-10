<!--
	TableMenu: the table's actions while the caret is in one, as a small bar
	over the table (lib/document/tables.ts). Rows and columns, the column's
	alignment and the header row are a click away; moving a row or column
	and the deletions sit behind More, so a deletion is never one stray
	click. Alt-F10 in the table brings the keyboard here (`focus`), Tab steps
	through the bar, and Escape goes back to the cell.
-->
<script lang="ts">
	import type { Editor } from "@tiptap/core";
	import IconButton from "$lib/components/IconButton.svelte";
	import { FloatingContent, type VirtualAnchor } from "$lib/floating";
	import { contextMenu } from "$lib/stores/contextMenu.svelte";
	import { tableActions, tableMenuItems, type TableAction } from "$lib/document/tables";

	interface Props {
		editor: Editor;
		/** Where the bar sits above, at its right end: the table's top edge, or the view's top once that has scrolled away (`tableBarAnchor`). */
		anchor: { x: number; y: number; width: number };
		/** Bumped by the host on every update, so the alignment shown follows the caret. */
		revision: number;
		/** The keyboard came into the bar or left it: the host keeps the bar while it is here. */
		onFocusChange?: (inside: boolean) => void;
	}

	let { editor, anchor, revision, onFocusChange }: Props = $props();

	let bar: HTMLDivElement;

	/** Bring the keyboard to the bar's first action. */
	export function focus() {
		bar?.querySelector<HTMLElement>("button")?.focus();
	}

	function onkeydown(e: KeyboardEvent) {
		if (e.key !== "Escape") return;
		e.preventDefault();
		editor.commands.focus();
	}

	const virtualAnchor = $derived<VirtualAnchor>({ x: anchor.x, y: anchor.y, width: anchor.width, height: 0 });
	const actions = $derived.by(() => {
		void revision;
		return tableActions(editor);
	});
	/**
	 * On touch every action is a 44pt button (design-grammar §6): a 28px one's
	 * grown hit area would reach into its neighbour's, so the edge of Row above
	 * ran Row below. Fewer fit a phone's width at that size, so the
	 * alignments go behind More there.
	 */
	const touch = typeof window !== "undefined" && !!window.matchMedia?.("(pointer: coarse)").matches;
	const inMore = (a: TableAction) => a.destructive || a.more || (touch && a.id.startsWith("align-"));
	const shown = $derived(actions.filter((a) => !inMore(a)));
	const behind = $derived(actions.filter(inMore));

	function more(e: MouseEvent) {
		const box = (e.currentTarget as HTMLElement).getBoundingClientRect();
		contextMenu.show({ x: box.left, y: box.bottom }, tableMenuItems(behind));
	}

	function run(a: TableAction) {
		a.run();
	}

	/**
	 * More's menu took the keyboard (a button cannot say which row is
	 * reached, so the menu holds the focus): the bar stays while it does,
	 * and until the focus lands outside both.
	 */
	function keepWhileMenuHasKeys() {
		const landed = (e: FocusEvent) => {
			const at = e.target as Element | null;
			if (at?.closest?.('[role="menu"]')) return;
			document.removeEventListener("focusin", landed, true);
			if (!bar?.contains(at)) onFocusChange?.(false);
		};
		document.addEventListener("focusin", landed, true);
	}
</script>

<FloatingContent
	anchor={virtualAnchor}
	options={{ placement: "top-end", offset: 6, flip: false, shift: true, padding: 8 }}
	class="table-menu-container"
>
	<div
		bind:this={bar}
		class="table-menu"
		role="toolbar"
		aria-label="Table"
		tabindex="-1"
		data-print="hide"
		onmousedown={(e) => e.preventDefault()}
		onfocusin={() => onFocusChange?.(true)}
		onfocusout={(e) => {
			const to = e.relatedTarget as Element | null;
			if (bar.contains(to)) return;
			if (to?.closest?.('[role="menu"]')) keepWhileMenuHasKeys();
			else onFocusChange?.(false);
		}}
		{onkeydown}
	>
		{#each shown as a (a.id)}
			{#if a.groupStart}<span class="sep"></span>{/if}
			<IconButton icon={a.icon} label={a.label} size={touch ? "touch" : "md"} pressed={a.checked} onclick={() => run(a)} />
		{/each}
		<span class="sep"></span>
		<IconButton icon="ri:more-2-line" label="More table actions" size={touch ? "touch" : "md"} haspopup="menu" onclick={more} />
	</div>
</FloatingContent>

<style>
	:global(.table-menu-container) {
		--z-floating: 100;
		padding: 0;
		background: transparent;
		border: none;
		box-shadow: none;
	}

	.table-menu {
		display: flex;
		align-items: center;
		gap: 0;
		padding: 4px;
		border: 1px solid var(--color-border-subtle, var(--color-border));
		border-radius: 12px;
		background: var(--color-surface);
	}

	.sep {
		width: 1px;
		align-self: stretch;
		margin: 4px;
		background: var(--color-border-subtle, var(--color-border));
	}
</style>
