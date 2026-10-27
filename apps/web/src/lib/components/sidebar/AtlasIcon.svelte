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
		projects: 'mynaui:folder',
		tasks: 'mynaui:check-square',
		applets: 'mynaui:sparkles',
		wiki: 'mynaui:globe',
		bookmarks: 'mynaui:bookmark',
		calendar: 'mynaui:calendar',
		drive: 'mynaui:archive',
		dashboard: 'mynaui:layout-dashboard',
		search: 'mynaui:search',
		device: 'mynaui:mobile',
		sources: 'mynaui:database',
		timeline: 'mynaui:path',
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
		display: 'mynaui:monitor',
		keys: 'mynaui:key',
		shared: 'mynaui:share',
		connections: 'mynaui:link',

		// Developer's, Sources' and Drive's own rows.
		sql: 'mynaui:table',
		terminal: 'mynaui:terminal',
		overview: 'mynaui:grid',
		catalog: 'mynaui:layers-three',
		files: 'mynaui:folder',
		streams: 'mynaui:waves',
		media: 'mynaui:image',
		trash: 'mynaui:trash',

		// The wiki's own rows.
		day: 'mynaui:sunrise',
		years: 'mynaui:tree',
		chapters: 'mynaui:list',
		identity: 'mynaui:user',
		people: 'mynaui:users',
		places: 'mynaui:map-pin',
		organizations: 'mynaui:building',
		stories: 'mynaui:book-open',
		history: 'mynaui:clock-circle',
	};

	/**
	 * The size of a glyph on the sidebar's rows and in the window header.
	 */
	export const SHELL_GLYPH_PX = 16;
</script>

<script lang="ts">
	/**
	 * A shell glyph by its name in the table above, drawn from the Myna set.
	 *
	 * The window header tints the sidebar glyph's column to say the sidebar is
	 * shown (`.atlas-fill`). Myna draws no column, so it is added here, under
	 * the outline, and the header fades its fill.
	 */
	import myna from '$lib/myna-icons.json';

	interface Props {
		name: string;
		size?: number;
		/**
		 * Stroke weight, in the 24-unit box's units. Leave it unset: it is
		 * derived from `size` so every glyph renders at the scale's one line,
		 * 1.25px of ink, at 14, 16 and 20 alike (design.md, "The scale").
		 */
		stroke?: number;
		/**
		 * Skip the sidebar's dress (`.sidebar-icon`: muted color, half
		 * opacity, sidebar sizing). The glyphs also serve rooms that are not
		 * the sidebar — the phone drawer — and there the host styles them.
		 */
		bare?: boolean;
	}

	let { name, size = 16, bare = false, stroke }: Props = $props();

	/** 1.25px of ink at any size: the box is 24 units wide. */
	const LINE_PX = 1.25;
	const strokeWidth = $derived(stroke ?? (LINE_PX * 24) / size);

	const SIDEBAR_COLUMN =
		'<path class="atlas-fill" d="M9 3.5H9.4C7.16 3.5 6.04 3.5 5.184 3.936a4 4 0 0 0-1.748 1.748C3 6.54 3 7.66 3 9.9v4.7c0 2.24 0 3.36.436 4.216a4 4 0 0 0 1.748 1.748C6.04 21 7.16 21 9.4 21H9z" fill="currentColor" fill-opacity="0" stroke="none"/>';

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
		const drawn = (myna.icons[icon]?.body ?? '').replace(/stroke-width="[\d.]+"/g, `stroke-width="${strokeWidth}"`);
		return name === 'sidebar' ? SIDEBAR_COLUMN + drawn : drawn;
	});
</script>

<svg
	class="atlas-icon {bare ? '' : 'sidebar-icon'}"
	width={size}
	height={size}
	viewBox="0 0 24 24"
	aria-hidden="true"
>
	<!-- eslint-disable-next-line svelte/no-at-html-tags — static glyph table above, no user input -->
	{@html body}
</svg>

<style>
	.atlas-icon {
		flex-shrink: 0;
		display: block;
	}
</style>
