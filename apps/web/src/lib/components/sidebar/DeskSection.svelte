<script lang="ts">
	/**
	 * The Desk — what you've taken off the shelf to work on.
	 *
	 * Pins, and pins are URL-keyed, so a Desk row can be ANY route: a notebook,
	 * an applet, a page, a PDF in Drive, a single day, a person, an external
	 * link. That generality is the point. The first version of this fetched
	 * notebooks and called them the Desk, which quietly redefined the zone as
	 * "your notebooks" and, worse, left Notebooks itself missing from the
	 * Library — a destination deleted by an implementation detail.
	 *
	 * Set in the serif with a bookcloth dot each: the names of the user's
	 * things get a bookface, and the type distinction (serif here, sans on the
	 * Library shelf below) encodes OWNERSHIP rather than decoration. No icons —
	 * a pinned thing has no natural glyph, which is what the dot is for. No
	 * selection state — the tabs and the path own "where am I".
	 *
	 * Order is the user's own `sort_order`, never recency: a shelf that
	 * reshuffles itself is how a stable list stops being a place.
	 */
	import { windowShellStore } from "$lib/stores/window-shell.svelte";
	import { pinsStore } from "$lib/stores/pins.svelte";
	import { contextMenu } from "$lib/stores/contextMenu.svelte";
	import { clothFor } from "$lib/sidebar/pin-colors";
	import type { Pin } from "$lib/api/client";

	interface Props {
		collapsed?: boolean;
		animationDelay?: number;
	}

	let { collapsed = false, animationDelay = 0 }: Props = $props();

	// Loaded once by the app layout; read the shared state, don't re-fetch.
	const pins = $derived(pinsStore.pins);

	function isExternal(url: string): boolean {
		return /^https?:\/\//i.test(url);
	}

	/** Falls back to the url when a pin has no label, per PinTarget's contract. */
	function labelFor(pin: Pin): string {
		return pin.label?.trim() || pin.url;
	}

	function open(pin: Pin) {
		if (isExternal(pin.url)) {
			window.open(pin.url, "_blank", "noopener,noreferrer");
			return;
		}
		windowShellStore.openTabFromRoute(pin.url, {
			label: labelFor(pin),
			focusExisting: true,
		});
	}

	function handleKeydown(e: KeyboardEvent, pin: Pin) {
		if (e.key === "Enter" || e.key === " ") {
			e.preventDefault();
			open(pin);
		}
	}

	function handleContextMenu(e: MouseEvent, pin: Pin) {
		e.preventDefault();
		e.stopPropagation();
		const items = [];
		if (!isExternal(pin.url)) {
			items.push({
				id: "open-beside",
				label: "Open beside",
				icon: "ri:layout-column-line",
				action: () => windowShellStore.openRouteBeside(pin.url, labelFor(pin)),
			});
		}
		items.push({
			id: "unpin",
			label: "Take off the desk",
			icon: "ri:unpin-line",
			action: () => {
				void pinsStore.remove(pin.id);
			},
		});
		contextMenu.show({ x: e.clientX, y: e.clientY }, items);
	}
</script>

{#if !collapsed}
	<div class="desk" style="--stagger-delay: {animationDelay}ms">
		<div class="desk-header">
			<span class="desk-title">Desk</span>
		</div>

		{#if pinsStore.loaded && pins.length === 0}
			<div class="desk-empty">Nothing checked out.</div>
		{:else}
			{#each pins as pin, i (pin.id)}
				<div
					class="sidebar-interactive desk-spine"
					role="link"
					tabindex="0"
					style="animation-delay: calc(var(--stagger-delay) + {i * 30}ms)"
					onclick={() => open(pin)}
					onkeydown={(e) => handleKeydown(e, pin)}
					oncontextmenu={(e) => handleContextMenu(e, pin)}
					title={labelFor(pin)}
				>
					<span class="desk-pin" aria-hidden="true">
						<i style="background: {clothFor(pin)}"></i>
					</span>
					<span class="sidebar-label desk-spine-label">{labelFor(pin)}</span>
				</div>
			{/each}
		{/if}
	</div>
{/if}

<style>
	@reference "../../../app.css";
	@reference "$lib/styles/sidebar.css";

	.desk {
		display: flex;
		flex-direction: column;
	}

	.desk-header {
		display: flex;
		align-items: center;
		height: 26px;
		padding: 0 8px 0 var(--sidebar-padding-left-base);
		margin-bottom: 2px;
		user-select: none;
		animation: deskRowIn 200ms cubic-bezier(0.2, 0, 0, 1) backwards;
		animation-delay: var(--stagger-delay, 0ms);
	}

	/* The zone subtitle: a whisper, not a headline. */
	.desk-title {
		font-size: 11px;
		font-weight: 500;
		letter-spacing: 0.015em;
		color: var(--color-foreground-subtle);
	}

	/* Spines: the serif appears in the chrome exactly where ownership does.
	   The display cut at text size wants its tracking back and a hair of
	   optical weight — the stroke reads as a medium, not a faux bold. */
	.desk-spine {
		font-family: var(--font-serif);
		font-size: 13.5px;
		font-weight: 400;
		letter-spacing: 0.02em;
		-webkit-text-stroke: 0.2px currentColor;
		animation: deskRowIn 200ms cubic-bezier(0.2, 0, 0, 1) backwards;
	}

	.desk-pin {
		width: 16px;
		height: 16px;
		flex-shrink: 0;
		display: flex;
		align-items: center;
		justify-content: center;
	}

	.desk-pin i {
		width: 6.5px;
		height: 6.5px;
		border-radius: 999px;
		display: block;
		box-shadow: inset 0 0 0 1px rgba(0, 0, 0, 0.12);
	}

	.desk-spine-label {
		line-height: 1.5;
	}

	.desk-empty {
		padding: 4px 8px 4px var(--sidebar-padding-left-base);
		font-family: var(--font-serif);
		font-style: italic;
		font-size: 12.5px;
		color: var(--color-foreground-subtle);
	}

	@keyframes deskRowIn {
		from {
			opacity: 0;
			transform: translateX(-8px);
		}
		to {
			opacity: 1;
			transform: translateX(0);
		}
	}

	@media (prefers-reduced-motion: reduce) {
		.desk-header,
		.desk-spine {
			animation: none;
		}
	}
</style>
