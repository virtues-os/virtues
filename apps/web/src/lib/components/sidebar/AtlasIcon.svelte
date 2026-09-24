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
		 * Stroke weight. The set is drawn at 1.1 for the 16px sidebar rows;
		 * at rail scale, icon-only, a 1.1 line washes out, so the rail asks
		 * for ~1.5. Kept a prop rather than baked so the two registers share
		 * one glyph table.
		 */
		stroke?: number;
		/**
		 * Skip the sidebar's dress (`.sidebar-icon`: muted color, half
		 * opacity, sidebar sizing). The glyphs also serve rooms that are not
		 * the sidebar — the phone drawer — and there the host styles them.
		 */
		bare?: boolean;
	}

	let { name, size = 16, bare = false, stroke = 1.1 }: Props = $props();

	const GLYPHS: Record<string, string> = {
		chats:
			'<circle cx="8" cy="7.5" r="5.2"/><path d="M4.6 11.9l-1.2 2"/><circle cx="5.9" cy="7.5" r="0.55" fill="currentColor" stroke="none"/><circle cx="8" cy="7.5" r="0.55" fill="currentColor" stroke="none"/><circle cx="10.1" cy="7.5" r="0.55" fill="currentColor" stroke="none"/>',
		pages:
			'<rect x="3.5" y="2.5" width="9" height="11" rx="1.2"/><path d="M6 6h5M6 8.5h5M6 11h3"/>',
		projects:
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
		// The room you land in. A drawn object like the rest — a roof and a
		// door, not the outline-house-in-a-circle every icon set ships. It is
		// the one glyph whose room is a PLACE rather than a kind of thing,
		// which is why it gets the most literal drawing in the set.
		home:
			'<path d="M2.8 7.4 8 3.1l5.2 4.3"/><path d="M4.2 8.5v4.4h7.6V8.5"/><path d="M6.7 12.9V9.8h2.6v3.1"/>',
		settings:
			'<circle cx="8" cy="8" r="5.2"/><path d="M8 2.8v2.7"/><circle cx="8" cy="8" r="1.1" fill="currentColor" stroke="none"/>',

		// ── The wiki's own rows ────────────────────────────────────────────
		// Drawn here rather than pulled from Remix for the reason the set
		// exists: the Wiki tile on the rail is an Atlas globe, and a panel of
		// Remix glyphs hanging off it is two icon languages in one column.
		// Same grid as the rest — 16px box, ~12px of ink, nothing touching an
		// edge — so they sit level with `calendar` and `pages` above them.

		// TODAY. A sun on the horizon, not a calendar leaf: Days already has
		// the calendar, and the two rows are adjacent.
		day:
			'<path d="M2.7 11.7h10.6"/><path d="M4.9 11.7a3.1 3.1 0 0 1 6.2 0"/><path d="M8 4.2v1.5M4.5 5.7l1 1M11.5 5.7l-1 1"/>',
		// YEARS. Growth rings — a section through something that grew. The
		// eccentricity is the whole drawing: concentric rings are a bullseye,
		// which is what the first attempt looked like at any size. Each ring
		// steps down and left, so the wide side reads as the good years.
		years:
			'<circle cx="8" cy="8" r="5.3"/><circle cx="7.1" cy="8.7" r="3.3"/><circle cx="6.5" cy="9.3" r="1.4"/>',
		// CHAPTERS. A line cut into stretches — the life's own partition, which
		// is what a chapter is. Drawn as a rule with two crossbars rather than
		// as a segmented capsule: the capsule version read as a battery at
		// every size. It is line-based on purpose, like Lifeline two rows
		// down — both are the life seen as one line, one divided and one not.
		//
		// Not a book. Stories is the open book and `projects` is the closed
		// one; a third would be the set's third book and nobody's second guess.
		// Three stretches of a life, staggered and of different lengths. Two
		// earlier drawings failed the only test that matters here, which is
		// 15px: a segmented capsule read as a battery, and a rule with two
		// crossbars read as two plus signs at full size and as a smudged dash
		// at row size. Bars survive it — and the STAGGER is what keeps them
		// from being the align-left glyph every icon set ships.
		chapters: '<path d="M2.7 4.7h6.6M6.2 8h7.1M3.6 11.3h5.2"/>',
		// LIFELINE. One continuous stroke that rises and falls. The only glyph
		// in the set with no enclosure, because the thing it names has no edge.
		lifeline:
			'<path d="M2.5 10.4c1.7 0 2.1-4.3 3.7-4.3s1.9 5.2 3.4 5.2 1.8-3.2 3.5-3.2"/>',
		// YOU. One figure, facing out. People is the same figure twice; the
		// difference has to survive at 16px, so this one is centred and larger
		// and that one is a pair.
		identity:
			'<circle cx="8" cy="5.9" r="2.5"/><path d="M3.5 13.3a4.6 4.6 0 0 1 9 0"/>',
		people:
			'<circle cx="6.2" cy="6" r="2.2"/><path d="M2.5 12.9a3.8 3.8 0 0 1 7.4 0"/><path d="M10.5 4.2a2.2 2.2 0 0 1 0 3.6"/><path d="M11.3 9.4a3.8 3.8 0 0 1 2.2 3.5"/>',
		// PLACES. The pin, which is the one interface symbol in the set. A
		// drawn object was tried — a waystone, a folded map — and both are mush
		// at this size; the pin is the only shape that survives it.
		places:
			'<path d="M8 13.5c0-.1 4.2-4 4.2-6.8a4.2 4.2 0 1 0-8.4 0c0 2.8 4.2 6.7 4.2 6.8z"/><circle cx="8" cy="6.6" r="1.4"/>',
		// ORGANIZATIONS. A facade with windows and a door. Deliberately NOT a
		// portico: the columns-and-pediment drawing is a roof over a box, which
		// is what `home` already is, and the two are four rows apart.
		organizations:
			'<rect x="3.4" y="2.9" width="9.2" height="10.5" rx="1.1"/><path d="M5.8 5.6h1.3M8.9 5.6h1.3M5.8 8.2h1.3M8.9 8.2h1.3"/><path d="M6.8 13.4v-2.5h2.4v2.5"/>',
		// STORIES. An open book. Chapters is the divided bar; this is the thing
		// you sit down and read.
		stories:
			'<path d="M8 4.9v8.2"/><path d="M8 4.9C6.7 3.9 5 3.6 3.1 3.7v8c1.9-.1 3.6.2 4.9 1.2 1.3-1 3-1.3 4.9-1.2v-8c-1.9-.1-3.6.2-4.9 1.2z"/>',
		// HISTORY. Drawn but currently UNATTACHED — the wiki panel's History row
		// was removed the same day, and the glyph is kept for when it returns.
		// A clock with hands: `settings` is also a circle, so the two are kept
		// apart by what is inside — a dial has one tick and a filled hub, a
		// clock has two hands and no hub.
		history: '<circle cx="8" cy="8" r="5.3"/><path d="M8 4.8V8l2.5 1.7"/>',
	};

	// Unknown names fall back to `pages` rather than drawing nothing, which is
	// forgiving in production and silent in development — a typo'd name looks
	// like a deliberate sheet of paper. The warning is the only thing that
	// tells you which row is lying.
	const paths = $derived.by(() => {
		const hit = GLYPHS[name];
		if (!hit && import.meta.env.DEV) {
			console.warn(`AtlasIcon: no glyph named "${name}" — drawing "pages" instead`);
		}
		return hit ?? GLYPHS.pages;
	});
</script>

<svg
	class="atlas-icon {bare ? '' : 'sidebar-icon'}"
	width={size}
	height={size}
	viewBox="0 0 16 16"
	fill="none"
	stroke="currentColor"
	stroke-width={stroke}
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
