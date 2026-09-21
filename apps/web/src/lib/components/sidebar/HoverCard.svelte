<script lang="ts">
	/**
	 * A hover card — the panel row's expansion, floated beside it.
	 *
	 * A 208px column can carry a name and nothing else. Everything a row would
	 * like to say about itself (what it holds, when it moved, the two or three
	 * verbs that apply) does not fit on the row and should not be a page. So
	 * the row grows a card to its right while the pointer rests on it, the way
	 * a project's row does in the chat clients people already use: the card is
	 * the row, opened, not a second object with its own identity.
	 *
	 * Two rules keep it from becoming a tooltip that bites:
	 *
	 *   - It opens on a DELAY and closes with a GRACE. The parent owns both
	 *     timers (it knows which row is hot); this component only reports
	 *     whether the pointer is on the card so the parent can hold it open
	 *     while you cross the gap to reach a button on it.
	 *   - It is PORTALED to the body and positioned with floating-ui. The
	 *     panel clips its overflow and the sidebar animates its width, and a
	 *     card that lived inside either would be cut off or dragged along.
	 *
	 * Chrome matches the context menu's card exactly — same surface, border,
	 * radius, shadow — because to the eye it IS a menu that opened itself.
	 */
	import { onMount } from 'svelte';
	import { autoUpdate, computePosition, flip, offset, shift } from '@floating-ui/dom';

	interface Props {
		/** The row the card belongs to. */
		anchor: HTMLElement;
		onenter?: () => void;
		onleave?: () => void;
		children: import('svelte').Snippet;
	}

	let { anchor, onenter, onleave, children }: Props = $props();

	let card = $state<HTMLDivElement | null>(null);

	onMount(() => {
		const el = card;
		if (!el) return;
		// Out of the panel's clipping and the aside's width transition.
		document.body.appendChild(el);

		const update = async () => {
			if (!anchor.isConnected) return;
			const { x, y } = await computePosition(anchor, el, {
				placement: 'right-start',
				strategy: 'fixed',
				middleware: [offset(10), flip({ fallbackPlacements: ['left-start'] }), shift({ padding: 8 })],
			});
			el.style.left = `${x}px`;
			el.style.top = `${y}px`;
		};
		const stop = autoUpdate(anchor, el, () => void update());

		return () => {
			stop();
			el.remove();
		};
	});
</script>

<div
	bind:this={card}
	class="hover-card"
	role="dialog"
	tabindex="-1"
	onmouseenter={onenter}
	onmouseleave={onleave}
>
	{@render children()}
</div>

<style>
	.hover-card {
		position: fixed;
		top: 0;
		left: 0;
		z-index: 60;
		width: 264px;
		padding: 6px;
		background: var(--color-surface);
		border: 1px solid var(--color-border);
		border-radius: 12px;
		color: var(--color-foreground);
		animation: hover-card-in 120ms ease-out;
	}

	@keyframes hover-card-in {
		from {
			opacity: 0;
			transform: translateX(-4px);
		}
		to {
			opacity: 1;
			transform: none;
		}
	}

	@media (prefers-reduced-motion: reduce) {
		.hover-card {
			animation: none;
		}
	}
</style>
