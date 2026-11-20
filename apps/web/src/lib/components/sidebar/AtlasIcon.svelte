<script module lang="ts">
	/**
	 * The glyph table: the shell's name for each of its things, and the Myna
	 * icon that draws it. Exported so `icons.ts` can register each one as
	 * `atlas:<name>` for surfaces that take an icon by string (tabs, menus,
	 * ⌘K). One drawing per thing, whichever way it is asked for.
	 */
	export const GLYPHS: Record<string, string> = {
		// The doors and rooms.
		chats: 'mynaui:chat',
		'new-chat': 'mynaui:chat-plus',
		pages: 'mynaui:file-text',
		inbox: 'mynaui:inbox',
		// A thread is more than a container, so it is not a folder.
		projects: 'mynaui:layers-three',
		tasks: 'mynaui:check-square',
		applets: 'mynaui:grid',
		wiki: 'mynaui:globe',
		bookmarks: 'mynaui:bookmark',
		calendar: 'mynaui:calendar',
		drive: 'mynaui:archive',
		dashboard: 'mynaui:layout-dashboard',
		search: 'mynaui:search',
		device: 'mynaui:mobile',
		sources: 'mynaui:database',
		setup: 'mynaui:flag',
		home: 'mynaui:home',
		settings: 'mynaui:cog',

		// The window header's controls.
		back: 'mynaui:arrow-left',
		forward: 'mynaui:arrow-right',
		sidebar: 'mynaui:sidebar',
		plus: 'mynaui:plus',
		close: 'mynaui:x',

		// The sidebar rows' hover controls.
		more: 'mynaui:dots',
		filter: 'mynaui:filter',
		pin: 'mynaui:pin',
		pinned: 'mynaui:pin-solid',

		// Settings' own rows.
		profile: 'mynaui:user-square',
		assistant: 'mynaui:pen',
		billing: 'mynaui:credit-card',
		system: 'mynaui:servers',
		devices: 'mynaui:desktop',
		keys: 'mynaui:key',
		shared: 'mynaui:share',

		// Developer's, Sources' and Drive's own rows.
		sql: 'mynaui:table',
		terminal: 'mynaui:terminal',
		overview: 'mynaui:grid',
		catalog: 'mynaui:layers-three',
		files: 'mynaui:folder',
		streams: 'mynaui:waves',
		media: 'mynaui:image',
		trash: 'mynaui:trash',
		passwords: 'mynaui:lock-password',

		// The wiki's own rows.
		day: 'mynaui:sunrise',
		years: 'mynaui:tree',
		chapters: 'mynaui:list',
		identity: 'mynaui:user',
		people: 'mynaui:users',
		places: 'mynaui:map-pin',
		organizations: 'mynaui:building',
		things: 'mynaui:box',
		history: 'mynaui:clock-circle',
	};

	/**
	 * The filled drawing of a glyph, for the sidebar's rows: a row's icon
	 * fills in under the pointer and stays filled on the row you are on, the
	 * way a selected symbol fills on a Mac. Only the glyphs the sidebar
	 * shows have one.
	 */
	export const SOLID_GLYPHS: Record<string, string> = {
		chats: 'mynaui:chat-solid',
		'new-chat': 'mynaui:chat-plus-solid',
		pages: 'mynaui:file-text-solid',
		projects: 'mynaui:layers-three-solid',
		tasks: 'mynaui:check-square-solid',
		applets: 'mynaui:grid-solid',
		wiki: 'mynaui:globe-solid',
		bookmarks: 'mynaui:bookmark-solid',
		calendar: 'mynaui:calendar-solid',
		drive: 'mynaui:archive-solid',
		setup: 'mynaui:flag-solid',
		day: 'mynaui:sunrise-solid',
		settings: 'mynaui:cog-solid',
	};

	/**
	 * The size of a glyph in the window header.
	 */
	export const SHELL_GLYPH_PX = 16;

	/**
	 * The size of a door's glyph on the sidebar's rows. Pins draw an 18px
	 * tile to match (PinGlyph). At 16px in the 44px rail the glyphs looked
	 * small for their rows.
	 */
	export const ROW_GLYPH_PX = 18;
</script>

<script lang="ts">
	/**
	 * A shell glyph by its name in the table above, drawn from the Myna set.
	 */
	import myna from '$lib/myna-icons.json';

	interface Props {
		name: string;
		size?: number;
		/**
		 * Stroke weight, in the 24-unit box's units. Leave it unset: it is
		 * derived from `size` so every glyph renders at the scale's one line,
		 * 1.25px of ink, at 14, 16 and 20 alike (design.md, "Glyphs").
		 */
		stroke?: number;
		/**
		 * Skip the sidebar's dress (`.sidebar-icon`: muted color, half
		 * opacity, sidebar sizing). The glyphs also serve rooms that are not
		 * the sidebar — the phone drawer — and there the host styles them.
		 */
		bare?: boolean;
		/**
		 * Also draw the glyph's filled version (`SOLID_GLYPHS`), hidden. The
		 * row it sits in shows it, by revealing `.glyph-solid` and hiding
		 * `.glyph-outline` (the sidebar's rows, HomePanel).
		 */
		fillable?: boolean;
	}

	let { name, size = 16, bare = false, stroke, fillable = false }: Props = $props();

	/** 1.25px of ink at any size: the box is 24 units wide. */
	const LINE_PX = 1.25;
	const strokeWidth = $derived(stroke ?? (LINE_PX * 24) / size);

	// Unknown names fall back to `pages` rather than drawing nothing, which is
	// forgiving in production and silent in development — a typo'd name looks
	// like a deliberate sheet of paper. The warning is the only thing that
	// tells you which row is lying.
	const body = $derived.by(() => {
		const hit = GLYPHS[name];
		if (!hit && import.meta.env.DEV) {
			console.warn(`AtlasIcon: no glyph named "${name}" — drawing "pages" instead`);
		}
		const icon = (hit ?? GLYPHS.pages).slice('mynaui:'.length) as keyof typeof myna.icons;
		return (myna.icons[icon]?.body ?? '').replace(/stroke-width="[\d.]+"/g, `stroke-width="${strokeWidth}"`);
	});

	const solidBody = $derived.by(() => {
		const solid = fillable ? SOLID_GLYPHS[name] : undefined;
		if (!solid) return null;
		return myna.icons[solid.slice('mynaui:'.length) as keyof typeof myna.icons]?.body ?? null;
	});
</script>

<svg
	class="atlas-icon {bare ? '' : 'sidebar-icon'}"
	width={size}
	height={size}
	viewBox="0 0 24 24"
	aria-hidden="true"
>
	{#if solidBody}
		<!-- eslint-disable svelte/no-at-html-tags — static glyph tables above, no user input -->
		<g class="glyph-outline">{@html body}</g>
		<g class="glyph-solid">{@html solidBody}</g>
		<!-- eslint-enable svelte/no-at-html-tags -->
	{:else}
		<!-- eslint-disable-next-line svelte/no-at-html-tags — static glyph table above, no user input -->
		{@html body}
	{/if}
</svg>

<style>
	.atlas-icon {
		flex-shrink: 0;
		display: block;
	}

	/* The two drawings cross-fade, so the swap reads as the glyph filling. */
	.glyph-outline,
	.glyph-solid {
		transition: opacity 120ms var(--ease-premium);
	}

	.glyph-solid {
		opacity: 0;
	}

	@media (prefers-reduced-motion: reduce) {
		.glyph-outline,
		.glyph-solid {
			transition: none;
		}
	}
</style>
