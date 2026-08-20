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
	import Icon from "$lib/components/Icon.svelte";
	import { isEmoji } from "$lib/utils/iconHelpers";
	import { windowShellStore } from "$lib/stores/window-shell.svelte";
	import { pinsStore } from "$lib/stores/pins.svelte";
	import { contextMenu, type ContextMenuItem } from "$lib/stores/contextMenu.svelte";
	import { clothFor } from "$lib/sidebar/pin-colors";
	import { pinIconMenuItem } from "$lib/pins/pinAction";
	import { sidebarZones } from "$lib/stores/sidebarZones.svelte";
	import ZoneHeader from "./ZoneHeader.svelte";
	import RefPicker, { type EntityResult } from "$lib/components/RefPicker.svelte";
	import type { Pin } from "$lib/api/client";

	interface Props {
		collapsed?: boolean;
	}

	let { collapsed = false }: Props = $props();

	// Loaded once by the app layout; read the shared state, don't re-fetch.
	const pins = $derived(pinsStore.pins);
	const zoneCollapsed = $derived(sidebarZones.isCollapsed("desk"));

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

	// The `+` opens the ref picker — the same search used for @-mentions and
	// notebook members, which already resolves any entity to a url and offers
	// a synthetic "Link" result when what you typed looks like one. So the
	// Desk can be filled with anything the app can name, without a bespoke
	// search of its own.
	let pickerAt = $state<{ x: number; y: number } | null>(null);

	function openPicker(e: MouseEvent) {
		e.preventDefault();
		e.stopPropagation();
		const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
		pickerAt = { x: r.left, y: r.bottom + 6 };
	}

	async function addFromPicker(entity: EntityResult) {
		pickerAt = null;
		try {
			await pinsStore.add(entity.url, entity.name, entity.icon);
		} catch (err) {
			console.error("[DeskSection] Failed to pin:", err);
		}
	}

	function handleContextMenu(e: MouseEvent, pin: Pin) {
		e.preventDefault();
		e.stopPropagation();
		const items: ContextMenuItem[] = [];
		if (!isExternal(pin.url)) {
			items.push({
				id: "open-beside",
				label: "Open beside",
				icon: "ri:layout-column-line",
				// Braced: `openRouteBeside` returns the new tab, and an arrow
				// body would make this a `() => string`, which is not an action.
				action: () => {
					windowShellStore.openRouteBeside(pin.url, labelFor(pin));
				},
			});
		}
		// Hang the picker off the click, not the middle of the window.
		const at = { x: e.clientX, y: e.clientY, width: 0, height: 0 };
		// One entry, not two: the picker holds the icon AND the color, so
		// splitting them into "Change icon" and "Color" made one decision look
		// like two features.
		const iconItem = pinIconMenuItem(pin.url, at);
		if (iconItem) items.push(iconItem);
		items.push({
			id: "unpin",
			label: "Take off the desk",
			icon: "ri:unpin-line",
			dividerBefore: true,
			action: () => {
				void pinsStore.remove(pin.id);
			},
		});
		contextMenu.show({ x: e.clientX, y: e.clientY }, items);
	}
</script>

{#if !collapsed}
	<div class="desk">
		<ZoneHeader id="desk" label="Desk">
			<button
				class="sidebar-item-action"
				title="Add to desk"
				aria-label="Add to desk"
				onclick={openPicker}
			>
				<svg width="14" height="14" viewBox="0 0 16 16" fill="none">
					<path
						d="M8 3.5v9M3.5 8h9"
						stroke="currentColor"
						stroke-width="1.5"
						stroke-linecap="round"
					/>
				</svg>
			</button>
		</ZoneHeader>

		<!-- Folds on the shared grid-rows accordion (0fr → 1fr), the same one
		     the collections use, so a zone closing looks like every other
		     thing that closes in this panel. -->
		<div class="sidebar-expandable" class:expanded={!zoneCollapsed}>
			<div class="sidebar-expandable-inner">
				{#if pinsStore.loaded && pins.length === 0}
					<div class="desk-empty">Nothing pinned yet</div>
				{:else}
					{#each pins as pin (pin.id)}
						<div
							class="sidebar-interactive desk-spine"
							role="link"
							tabindex="0"
							onclick={() => open(pin)}
							onkeydown={(e) => handleKeydown(e, pin)}
							oncontextmenu={(e) => handleContextMenu(e, pin)}
							title={labelFor(pin)}
						>
							<!-- The glyph if the user picked one, the cloth dot if
							     not. The dot is still the default — most pins have
							     no natural icon, which is why it exists — but a pin
							     that HAS been given one should wear it, in its own
							     color, the same as its tab does. -->
							<span class="desk-pin" aria-hidden="true">
								{#if pin.icon && isEmoji(pin.icon)}
									<span class="desk-emoji">{pin.icon}</span>
								{:else if pin.icon}
									<Icon
										icon={pin.icon}
										width="14"
										style="color: {clothFor(pin)}"
									/>
								{:else}
									<i style="background: {clothFor(pin)}"></i>
								{/if}
							</span>
							<span class="sidebar-label desk-spine-label">{labelFor(pin)}</span>
						</div>
					{/each}
				{/if}
				<!-- The seam as content, not as padding. Padding on the clipping
				     box would sit OUTSIDE the collapsed content box — and
				     `overflow: hidden` clips at the padding box — so 28px of the
				     list stayed visible with the Desk shut, which is exactly one
				     row: the first pin appeared to survive the fold. A spacer is
				     inside the thing being folded, so it folds. -->
				<div class="desk-tail" aria-hidden="true"></div>
			</div>
		</div>
	</div>
{/if}

{#if pickerAt}
	<RefPicker
		mode="single"
		position={pickerAt}
		placeholder="Search, or paste a link…"
		excludeIds={pins.map((p) => p.url)}
		onSelect={addFromPicker}
		onClose={() => (pickerAt = null)}
	/>
{/if}

<style>
	@reference "../../../app.css";
	@reference "$lib/styles/sidebar.css";

	.desk {
		display: flex;
		flex-direction: column;
	}

	/* The seam between Desk and Library, as a row of air the fold can carry
	   away. Same token as every other interval in the column. */
	.desk-tail {
		height: var(--sidebar-interactive-height);
		flex: none;
	}

	/* Spines: the serif appears in the chrome exactly where ownership does.
	   The display cut at text size wants its tracking back and a hair of
	   optical weight — the stroke reads as a medium, not a faux bold. */
	.desk-spine {
		/* The chrome cut: the spine sits beside an icon, and JJannon's own
		   metrics would leave the letters 1.3px above it. */
		font-family: var(--font-serif-ui);
		font-size: 13.5px;
		font-weight: 400;
		letter-spacing: 0.02em;
		-webkit-text-stroke: 0.2px currentColor;
	}

	.desk-pin {
		width: 16px;
		height: 16px;
		flex-shrink: 0;
		display: flex;
		align-items: center;
		justify-content: center;
	}

	.desk-emoji {
		font-size: 12px;
		line-height: 1;
	}

	.desk-pin i {
		width: 6.5px;
		height: 6.5px;
		border-radius: 999px;
		display: block;
		box-shadow: inset 0 0 0 1px rgba(0, 0, 0, 0.12);
	}

	/* An integer box: 13.5 × 1.5 = 20.25px, which centers on a half pixel and
	   rounds differently by DPR and scroll position. 20px is the same leading
	   without the wobble. */
	.desk-spine-label {
		line-height: 20px;
	}

	/* Plain, quiet, sans. It was set in italic serif — which made the one line
	   in the panel that says "there is nothing here" the most decorated thing
	   in it. An empty state is a statement of fact, not a flourish; the serif
	   is reserved for the names of real things. */
	.desk-empty {
		padding: 3px 8px 3px
			calc(
				var(--sidebar-padding-left-base) + var(--sidebar-interactive-icon-size) +
					var(--sidebar-interactive-gap)
			);
		font-size: 12px;
		color: var(--color-foreground-subtle);
	}

</style>
