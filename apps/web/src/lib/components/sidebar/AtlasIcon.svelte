<script lang="ts">
	/**
	 * Atlas — the shell's own icon set.
	 *
	 * Drawn objects, not interface symbols: a globe for the wiki, an archive
	 * box for the drive, a dial for settings. The library metaphor the sidebar
	 * is built on (desk, shelf, checkout) only holds if its hardware comes from
	 * the same world, which is why these are hand-drawn here rather than pulled
	 * from a general-purpose set. Fourteen glyphs is an ownable number.
	 *
	 * One optical grid: 16px box, ~12px of ink, 1.1px stroke, round caps.
	 * Anything that fills the box edge-to-edge reads oversized next to the
	 * others no matter how correct its geometry is.
	 */
	interface Props {
		name: string;
		size?: number;
		/**
		 * Skip the sidebar's dress (`.sidebar-icon`: muted color, half
		 * opacity, sidebar sizing). The glyphs also serve rooms that are not
		 * the sidebar — the phone drawer — and there the host styles them.
		 */
		bare?: boolean;
	}

	let { name, size = 16, bare = false }: Props = $props();

	const GLYPHS: Record<string, string> = {
		chats:
			'<circle cx="8" cy="7.5" r="5.2"/><path d="M4.6 11.9l-1.2 2"/><circle cx="5.9" cy="7.5" r="0.55" fill="currentColor" stroke="none"/><circle cx="8" cy="7.5" r="0.55" fill="currentColor" stroke="none"/><circle cx="10.1" cy="7.5" r="0.55" fill="currentColor" stroke="none"/>',
		pages:
			'<rect x="3.5" y="2.5" width="9" height="11" rx="1.2"/><path d="M6 6h5M6 8.5h5M6 11h3"/>',
		notebooks:
			'<path d="M5 2.5h7.5a1 1 0 0 1 1 1v9a1 1 0 0 1-1 1H5a1.5 1.5 0 0 1-1.5-1.5v-8A1.5 1.5 0 0 1 5 2.5z"/><path d="M3.5 5.5h2M3.5 8h2M3.5 10.5h2"/>',
		bookmarks: '<path d="M4.5 2.5h7v11L8 11l-3.5 2.5z"/>',
		calendar:
			'<rect x="2.5" y="3.5" width="11" height="10" rx="1.2"/><path d="M2.5 6.5h11M5.5 2v2.5M10.5 2v2.5"/><circle cx="8" cy="10" r="0.7" fill="currentColor" stroke="none"/>',
		wiki: '<circle cx="8" cy="8" r="5.5"/><ellipse cx="8" cy="8" rx="2.4" ry="5.5"/><path d="M2.5 8h11"/>',
		drive:
			'<rect x="2" y="2.5" width="12" height="3" rx="1"/><rect x="3" y="5.5" width="10" height="8" rx="1.2"/><path d="M6.5 9h3"/>',
		applets:
			'<path d="M8 2.2 9.6 6.4 13.8 8 9.6 9.6 8 13.8 6.4 9.6 2.2 8 6.4 6.4z"/>',
		search: '<circle cx="7.1" cy="7.1" r="4.3"/><path d="M10.4 10.4 13.5 13.5"/>',
		// The chats bubble, empty and waiting — a plus where the conversation
		// dots would be. Drawn for the phone drawer's "New chat" door.
		'new-chat':
			'<circle cx="8" cy="7.5" r="5.2"/><path d="M4.6 11.9l-1.2 2"/><path d="M8 5.6v3.8M6.1 7.5h3.8"/>',
		// The phone itself — the drawer's "This device" door. A drawn object
		// like the rest: the slab and its home bar, nothing else.
		device:
			'<rect x="4.6" y="2.2" width="6.8" height="11.6" rx="1.5"/><path d="M6.9 11.6h2.2"/>',
		// An inkwell, taking a drop. The well the record is written from — a desk
		// object, like the rest of this set, rather than the plug or stacked
		// database cylinder a general-purpose set would offer. It also avoids the
		// funnel, which at this size is read as Filter everywhere else.
		sources:
			'<ellipse cx="8" cy="6" rx="3.4" ry="1.2"/><path d="M4.6 6v4.3c0 1.2 1.5 2.2 3.4 2.2s3.4-1 3.4-2.2V6"/><path d="M8 2.3v1.7"/>',
		developer: '<path d="M3 5l3.2 3L3 11"/><path d="M9 11.5h4"/>',
		settings:
			'<circle cx="8" cy="8" r="5.2"/><path d="M8 2.8v2.7"/><circle cx="8" cy="8" r="1.1" fill="currentColor" stroke="none"/>',
	};

	const paths = $derived(GLYPHS[name] ?? GLYPHS.pages);
</script>

<svg
	class="atlas-icon {bare ? '' : 'sidebar-icon'}"
	width={size}
	height={size}
	viewBox="0 0 16 16"
	fill="none"
	stroke="currentColor"
	stroke-width="1.1"
	stroke-linecap="round"
	stroke-linejoin="round"
	aria-hidden="true"
>
	<!-- eslint-disable-next-line svelte/no-at-html-tags — static glyph table above, no user input -->
	{@html paths}
</svg>

<style>
	.atlas-icon {
		flex-shrink: 0;
		display: block;
	}
</style>
