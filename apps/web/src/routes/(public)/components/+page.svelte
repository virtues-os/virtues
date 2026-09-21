<!--
  /components — the design gallery.

  WHAT THIS IS. The one place in this product where you can look at the
  product's own parts. Every primitive in `$lib/components`, in every state it
  actually has; every design token with its resolved value; the type scale; and
  a WCAG contrast check run across all sixteen themes at once.

  WHY IT EXISTS. There was nowhere to see a theme regression before a user did.
  Sixteen themes, 227 `.svelte` files, a primitive library only about thirty of
  them import, and no kitchen sink. A token that goes wrong in Canterbury goes
  wrong silently until somebody opens Canterbury. This page makes that visible
  in one screen.

  THE RULE. A new primitive must appear here. If you add a component to
  `apps/web/src/lib/components/` or export one from the `$lib` barrel, add it to
  this page in the same change. A primitive that is not in the gallery is a
  primitive nobody can review, and it will be re-invented by hand in the next
  view that needs it — which is exactly how twenty-two hand-rolled card blocks
  happened.

  THIS IS A TOOL, NOT A PRODUCT SURFACE. It is meant to be complete, dense and
  honest, not beautiful. It shows what is actually there, including the parts
  that are wrong. Where a component's API made a state impossible to reach from
  the outside, this page says so in a NOTE rather than faking the state.

  HOW THE READINGS ARE TAKEN. On mount the page walks all sixteen themes by
  setting `data-theme` on the document element, reads every token out of the
  cascade with `getComputedStyle`, resolves each to real sRGB through a hidden
  probe element (so `color-mix()` and `rgba()` resolve exactly as the browser
  resolves them), then puts the original theme back. Nothing here is a copy of
  themes.css — a value printed on this page is the value the browser computed.

  NO PERSISTENCE. Switching themes here only sets the attribute. It never calls
  `setTheme`, so browsing the gallery cannot overwrite the theme saved on the
  box. The route group's layout loads the real theme from the database a moment
  after mount; an observer re-asserts the gallery's pick when that lands, and
  the original attribute is restored on leaving.

  All sample content is fictional (see CLAUDE.md, "Never commit anything from a
  real life").
-->
<script lang="ts">
	import { onMount } from "svelte";

	import Badge from "$lib/components/Badge.svelte";
	import Button from "$lib/components/Button.svelte";
	import Card from "$lib/components/Card.svelte";
	import EmptyState from "$lib/components/EmptyState.svelte";
	import ErrorState from "$lib/components/ErrorState.svelte";
	import Field from "$lib/components/Field.svelte";
	import Icon from "$lib/components/Icon.svelte";
	import IconButton from "$lib/components/IconButton.svelte";
	import Input from "$lib/components/Input.svelte";
	import LoadingState from "$lib/components/LoadingState.svelte";
	import Modal from "$lib/components/Modal.svelte";
	import Page from "$lib/components/Page.svelte";
	import PageHeading from "$lib/components/PageHeading.svelte";
	import Section from "$lib/components/Section.svelte";
	import SubNav from "$lib/components/SubNav.svelte";
	import TextAction from "$lib/components/TextAction.svelte";
	import Textarea from "$lib/components/Textarea.svelte";

	// ========================================================================
	// THEMES
	// ========================================================================

	type ThemeRow = {
		id: string;
		label: string;
		mode: "light" | "dark";
		/** "Virtues Light" / "Virtues Dark" — the two the product stands behind. */
		canonical?: string;
		note?: string;
	};

	/**
	 * Order matches `getAvailableThemes()` in `$lib/utils/theme`: the two
	 * canonical themes lead, then light, then dark. Pemberley is `:root` rather
	 * than a `[data-theme]` block, so setting an unmatched attribute value is
	 * exactly how you select it — that is not a typo below.
	 */
	const THEMES: ThemeRow[] = [
		{ id: "oxford", label: "Oxford", mode: "light", canonical: "Virtues Light", note: "virtues-registry DEFAULT_THEME — what a new box lands on" },
		{ id: "asgard", label: "Asgard", mode: "dark", canonical: "Virtues Dark" },
		{ id: "pemberley", label: "Pemberley", mode: "light", note: "occupies :root — the base layer every other theme overrides" },
		{ id: "caladan", label: "Caladan", mode: "light" },
		{ id: "rivendell", label: "Rivendell", mode: "light" },
		{ id: "netherfield", label: "Netherfield", mode: "light" },
		{ id: "hogwarts", label: "Hogwarts", mode: "light" },
		{ id: "tatooine", label: "Tatooine", mode: "light" },
		{ id: "lothlorien", label: "Lothlorien", mode: "dark" },
		{ id: "baker-street", label: "Baker Street", mode: "dark" },
		{ id: "narnia", label: "Narnia", mode: "dark" },
		{ id: "canterbury", label: "Canterbury", mode: "dark" },
		{ id: "borghese", label: "Borghese", mode: "dark", note: "deliberately achromatic: --primary / --success / --error are all #FFFFFF, --warning / --info #CCCCCC. Not a bug — the one theme with no hue at all." },
		{ id: "lyceum", label: "The Lyceum", mode: "dark" },
		{ id: "agora", label: "Agora", mode: "dark" },
		{ id: "shire", label: "The Shire", mode: "dark" },
	];

	// ========================================================================
	// TOKENS
	// ========================================================================

	type TokenGroup = {
		name: string;
		blurb: string;
		tokens: string[];
		/** Non-color groups render as text rows, with no swatch. */
		kind?: "color" | "text";
	};

	/**
	 * Every token is named WITHOUT the leading `--`. The page reads each one
	 * twice: the raw `--name` that themes.css defines, and the `--color-name`
	 * bridge that `@theme inline` emits and that components actually write
	 * (`var(--color-foreground)`, about 1300 uses). Where a bridge does not
	 * exist the page prints "—", which is itself a finding: it means a component
	 * reaching for `--color-<that>` gets nothing.
	 */
	const TOKEN_GROUPS: TokenGroup[] = [
		{
			name: "Surfaces",
			blurb: "The grounds. `background` is the page, `surface` the pane and card, `surface-elevated` the raised/hover ground, `surface-overlay` a popover.",
			tokens: ["background", "surface", "surface-elevated", "surface-overlay", "background-inverse"],
		},
		{
			name: "Foreground",
			blurb: "The ink ramp, strongest first. `disabled` is not held to AA — a disabled control is exempt — but it is measured below anyway.",
			tokens: ["foreground", "foreground-muted", "foreground-subtle", "foreground-disabled"],
		},
		{
			name: "Border",
			blurb: "Hairlines. `border-focus` is the ring; `border-strong` is what an input wears at rest.",
			tokens: ["border", "border-subtle", "border-strong", "border-focus"],
		},
		{
			name: "Interaction (primary / secondary)",
			blurb: "Navy (`primary`) is the ink of anything pressable. Claret (`secondary`) means 'you are here; do this now' — and, confusingly, is what `.btn-primary` actually fills with.",
			tokens: [
				"primary", "primary-hover", "primary-active", "primary-subtle",
				"secondary", "secondary-hover", "secondary-active",
			],
		},
		{
			name: "Semantic",
			blurb: "State color. Each pairs a foreground with a `-subtle` fill; Badge uses exactly those pairs, so both halves are measured in the contrast table.",
			tokens: [
				"success", "success-subtle", "warning", "warning-subtle",
				"error", "error-subtle", "info", "info-subtle",
			],
		},
		{
			name: "Selection and diff",
			blurb: "`highlight` carries alpha on purpose — it sits over text. The contrast table composites it over `background` before measuring.",
			tokens: [
				"highlight", "highlight-foreground",
				"diff-add", "diff-add-bg", "diff-remove", "diff-remove-bg",
			],
		},
		{
			name: "Interaction ramp",
			blurb: "Hover and press, defined once as a mix against the FOREGROUND rather than against another surface — which is why they survive all sixteen themes. These carry alpha; the swatch is drawn over `surface` so you see what a user sees.",
			tokens: [
				"hover-bg", "active-bg", "press-bg",
				"sidebar-hover-bg", "sidebar-active-bg",
				"tab-active-bg", "tab-active-bg-focused",
			],
		},
		{
			name: "Category hues",
			blurb: "The fixed palette used to color user-chosen things (projects, sources). Not theme-semantic — they are the same in every theme.",
			tokens: [
				"cat-purple", "cat-purple-light", "cat-indigo", "cat-violet",
				"cat-pink", "cat-rose", "cat-orange", "cat-yellow",
				"cat-cyan", "cat-cyan-light", "cat-emerald", "cat-emerald-light",
			],
		},
		{
			name: "Metrics, motion, layers",
			blurb: "The non-color tokens. Shown because a regression here (a chrome row that stops matching the pane inset, a z-index that overtakes the modal) is just as invisible as a color one.",
			kind: "text",
			tokens: [
				"chrome-row-h", "chrome-tab-h", "pane-inset", "radius-full",
				"sidebar-interactive-height", "sidebar-interactive-radius",
				"sidebar-interactive-font-size", "sidebar-interactive-icon-size",
				"sidebar-padding-left-base", "sidebar-indent-width", "sidebar-item-gap",
				"sidebar-icon-opacity", "sidebar-child-icon-opacity",
				"duration-fast", "ease-in-out-quad", "ease-premium",
				"font-serif", "font-serif-ui", "font-sans", "font-mono",
				"z-raised", "z-sticky", "z-dropdown", "z-popover", "z-overlay",
				"z-modal", "z-lightbox", "z-toast", "z-context-menu",
				"z-context-submenu", "z-tooltip",
			],
		},
	];

	const COLOR_TOKENS = TOKEN_GROUPS.filter((g) => g.kind !== "text").flatMap((g) => g.tokens);
	const ALL_TOKENS = TOKEN_GROUPS.flatMap((g) => g.tokens);

	// ========================================================================
	// COLOR MATH (WCAG 2.1)
	// ========================================================================

	type RGBA = { r: number; g: number; b: number; a: number };

	/** Handles both `rgb()/rgba()` and the `color(srgb r g b / a)` form. */
	function parseColor(str: string): RGBA | null {
		const s = str.trim();
		let m = /^rgba?\(([^)]+)\)$/i.exec(s);
		if (m) {
			const p = m[1].split(/[,/\s]+/).filter(Boolean).map(Number);
			if (p.length >= 3 && p.slice(0, 3).every((n) => !Number.isNaN(n))) {
				return { r: p[0], g: p[1], b: p[2], a: p.length > 3 && !Number.isNaN(p[3]) ? p[3] : 1 };
			}
			return null;
		}
		m = /^color\(srgb\s+([^)]+)\)$/i.exec(s);
		if (m) {
			const p = m[1].split(/[/\s]+/).filter(Boolean).map(Number);
			if (p.length >= 3 && p.slice(0, 3).every((n) => !Number.isNaN(n))) {
				return {
					r: p[0] * 255,
					g: p[1] * 255,
					b: p[2] * 255,
					a: p.length > 3 && !Number.isNaN(p[3]) ? p[3] : 1,
				};
			}
		}
		return null;
	}

	function relativeLuminance(c: RGBA): number {
		const f = (v: number) => {
			const s = Math.min(255, Math.max(0, v)) / 255;
			return s <= 0.03928 ? s / 12.92 : Math.pow((s + 0.055) / 1.055, 2.4);
		};
		return 0.2126 * f(c.r) + 0.7152 * f(c.g) + 0.0722 * f(c.b);
	}

	/** Flatten a translucent color onto an opaque one. */
	function over(fg: RGBA, bg: RGBA): RGBA {
		const a = Math.min(1, Math.max(0, fg.a));
		return {
			r: fg.r * a + bg.r * (1 - a),
			g: fg.g * a + bg.g * (1 - a),
			b: fg.b * a + bg.b * (1 - a),
			a: 1,
		};
	}

	function contrast(fg: RGBA, bg: RGBA): number {
		const a = relativeLuminance(fg);
		const b = relativeLuminance(bg);
		const hi = Math.max(a, b);
		const lo = Math.min(a, b);
		return (hi + 0.05) / (lo + 0.05);
	}

	function hex(c: RGBA): string {
		const h = (v: number) => Math.round(Math.min(255, Math.max(0, v))).toString(16).padStart(2, "0");
		return `#${h(c.r)}${h(c.g)}${h(c.b)}`.toUpperCase();
	}

	// ========================================================================
	// CONTRAST PAIRS
	// ========================================================================

	type Pair = {
		id: string;
		fg: string;
		bg: string;
		/** Where a reader actually meets this pair. */
		where: string;
		/** AA threshold: 4.5 for body text, 3 for large text and UI shape. */
		need: number;
		/** WCAG exempts it, but it is still measured and shown. */
		exempt?: boolean;
	};

	const PAIRS: Pair[] = [
		{ id: "fg / background", fg: "foreground", bg: "background", where: "body text on the page", need: 4.5 },
		{ id: "fg-muted / background", fg: "foreground-muted", bg: "background", where: "secondary body text", need: 4.5 },
		{ id: "fg-subtle / background", fg: "foreground-subtle", bg: "background", where: "hints, dates, notes", need: 4.5 },
		{ id: "fg / surface", fg: "foreground", bg: "surface", where: "body text in a card or pane", need: 4.5 },
		{ id: "fg-muted / surface", fg: "foreground-muted", bg: "surface", where: "Field values, ghost buttons", need: 4.5 },
		{ id: "fg-subtle / surface", fg: "foreground-subtle", bg: "surface", where: "Field hints, placeholders", need: 4.5 },
		{ id: "fg-subtle / elevated", fg: "foreground-subtle", bg: "surface-elevated", where: "muted Badge label", need: 4.5 },
		{ id: "fg-disabled / surface", fg: "foreground-disabled", bg: "surface", where: "disabled control text", need: 4.5, exempt: true },
		{ id: "primary / background", fg: "primary", bg: "background", where: "links, the ink of pressable things", need: 4.5 },
		{ id: "primary / surface", fg: "primary", bg: "surface", where: "ErrorState retry, primary Badge", need: 4.5 },
		{ id: "secondary / background", fg: "secondary", bg: "background", where: "Button secondary label and border", need: 4.5 },
		{ id: "surface / secondary", fg: "surface", bg: "secondary", where: "Button PRIMARY label (.btn-primary fills with --secondary)", need: 4.5 },
		{ id: "surface / error", fg: "surface", bg: "error", where: "Button danger label", need: 4.5 },
		{ id: "success / surface", fg: "success", bg: "surface", where: "success text", need: 4.5 },
		{ id: "warning / surface", fg: "warning", bg: "surface", where: "warning text, Field warning note", need: 4.5 },
		{ id: "error / surface", fg: "error", bg: "surface", where: "ErrorState, error text", need: 4.5 },
		{ id: "info / surface", fg: "info", bg: "surface", where: "info text", need: 4.5 },
		{ id: "success / success-subtle", fg: "success", bg: "success-subtle", where: "Badge variant=success", need: 4.5 },
		{ id: "warning / warning-subtle", fg: "warning", bg: "warning-subtle", where: "Badge variant=warning", need: 4.5 },
		{ id: "error / error-subtle", fg: "error", bg: "error-subtle", where: "Badge variant=error", need: 4.5 },
		{ id: "info / info-subtle", fg: "info", bg: "info-subtle", where: "Badge variant=info", need: 4.5 },
		{ id: "diff-add / diff-add-bg", fg: "diff-add", bg: "diff-add-bg", where: "diff, added line", need: 4.5 },
		{ id: "diff-remove / diff-remove-bg", fg: "diff-remove", bg: "diff-remove-bg", where: "diff, removed line", need: 4.5 },
		{ id: "highlight-fg / highlight", fg: "highlight-foreground", bg: "highlight", where: "selected text (highlight composited over background)", need: 4.5 },
		{ id: "border / background", fg: "border", bg: "background", where: "hairline — UI shape, 3:1", need: 3 },
		{ id: "border-strong / surface", fg: "border-strong", bg: "surface", where: "input at rest — UI shape, 3:1", need: 3 },
		{ id: "border-focus / surface", fg: "border-focus", bg: "surface", where: "focus ring — UI shape, 3:1", need: 3 },
	];

	// ========================================================================
	// READING THE CASCADE
	// ========================================================================

	type ThemeRead = {
		raw: Record<string, string>;
		bridged: Record<string, string>;
		rgb: Record<string, RGBA | null>;
	};

	let readings = $state<Record<string, ThemeRead>>({});
	let ready = $state(false);

	/** Invalid color values leave this behind, which is how we detect them. */
	const SENTINEL = "rgb(1, 2, 3)";

	function resolveColor(probe: HTMLElement, value: string): RGBA | null {
		if (!value) return null;
		probe.style.color = SENTINEL;
		probe.style.color = value;
		const computed = getComputedStyle(probe).color;
		if (computed.replace(/\s+/g, "") === SENTINEL.replace(/\s+/g, "")) {
			// Either genuinely that color, or the assignment was rejected. Either
			// way it is not a value worth reporting, so say nothing.
			return null;
		}
		return parseColor(computed);
	}

	function readAllThemes(): Record<string, ThemeRead> {
		const html = document.documentElement;
		const previous = html.getAttribute("data-theme");
		const probe = document.createElement("span");
		probe.style.cssText = "position:fixed;left:-9999px;top:-9999px;width:0;height:0;";
		document.body.appendChild(probe);

		const out: Record<string, ThemeRead> = {};
		for (const theme of THEMES) {
			html.setAttribute("data-theme", theme.id);
			const cs = getComputedStyle(html);
			const raw: Record<string, string> = {};
			const bridged: Record<string, string> = {};
			for (const name of ALL_TOKENS) {
				raw[name] = cs.getPropertyValue(`--${name}`).trim();
				bridged[name] = cs.getPropertyValue(`--color-${name}`).trim();
			}
			const rgb: Record<string, RGBA | null> = {};
			for (const name of COLOR_TOKENS) {
				rgb[name] = resolveColor(probe, raw[name] || bridged[name]);
			}
			out[theme.id] = { raw, bridged, rgb };
		}

		probe.remove();
		if (previous) html.setAttribute("data-theme", previous);
		else html.removeAttribute("data-theme");
		return out;
	}

	// ========================================================================
	// THEME SWITCHING (visual only — never persisted)
	// ========================================================================

	let theme = $state<string>("oxford");
	let originalTheme: string | null = null;

	onMount(() => {
		originalTheme = document.documentElement.getAttribute("data-theme");
		if (originalTheme && THEMES.some((t) => t.id === originalTheme)) {
			theme = originalTheme;
		}

		readings = readAllThemes();
		ready = true;

		document.documentElement.setAttribute("data-theme", theme);

		// The route group's layout calls `initTheme()`, which loads the saved
		// theme from the database and writes the attribute a beat later. Without
		// this the gallery's pick would be silently overwritten mid-review.
		const observer = new MutationObserver(() => {
			if (document.documentElement.getAttribute("data-theme") !== theme) {
				document.documentElement.setAttribute("data-theme", theme);
			}
		});
		observer.observe(document.documentElement, { attributes: true, attributeFilter: ["data-theme"] });

		return () => {
			observer.disconnect();
			if (originalTheme) document.documentElement.setAttribute("data-theme", originalTheme);
			else document.documentElement.removeAttribute("data-theme");
		};
	});

	$effect(() => {
		if (!ready) return;
		document.documentElement.setAttribute("data-theme", theme);
	});

	const activeTheme = $derived(THEMES.find((t) => t.id === theme) ?? THEMES[0]);
	const currentRead = $derived(readings[theme]);

	// ========================================================================
	// DERIVED CONTRAST RESULTS
	// ========================================================================

	type Result = {
		pair: Pair;
		ratio: number | null;
		fgHex: string;
		bgHex: string;
		pass: boolean;
	};

	function resultsFor(themeId: string): Result[] {
		const read = readings[themeId];
		if (!read) return [];
		const page = read.rgb["background"] ?? { r: 255, g: 255, b: 255, a: 1 };
		return PAIRS.map((pair) => {
			const rawFg = read.rgb[pair.fg];
			const rawBg = read.rgb[pair.bg];
			if (!rawFg || !rawBg) {
				return { pair, ratio: null, fgHex: "—", bgHex: "—", pass: true };
			}
			const bg = over(rawBg, page);
			const fg = over(rawFg, bg);
			const ratio = contrast(fg, bg);
			return {
				pair,
				ratio,
				fgHex: hex(fg),
				bgHex: hex(bg),
				pass: ratio >= pair.need,
			};
		});
	}

	const currentResults = $derived(resultsFor(theme));

	const allResults = $derived(
		ready ? THEMES.map((t) => ({ theme: t, results: resultsFor(t.id) })) : [],
	);

	/** Every failure, in every theme, listed plainly. Exempt pairs excluded. */
	const failures = $derived(
		allResults
			.map(({ theme: t, results }) => ({
				theme: t,
				failed: results.filter((r) => r.ratio !== null && !r.pass && !r.pair.exempt),
			}))
			.filter((row) => row.failed.length > 0),
	);

	const failureCount = $derived(failures.reduce((n, row) => n + row.failed.length, 0));

	function fmt(r: number | null): string {
		if (r === null) return "—";
		return r >= 100 ? r.toFixed(0) : r.toFixed(2);
	}

	// ========================================================================
	// TYPE SCALE (agents/build/design-grammar.md §4)
	// ========================================================================

	type TypeRow = { size: number; face: "serif" | "sans" | "mono"; weight?: number; role: string; specimen: string };

	const TYPE_SCALE: TypeRow[] = [
		{ size: 36, face: "serif", role: "the page title (a dateline on Home)", specimen: "Tuesday, the sixteenth" },
		{ size: 22, face: "serif", role: "a step's heading; a section title (--md-h2-size)", specimen: "What should your server call you?" },
		{ size: 18, face: "serif", role: "the box's line — the novelty caption, the ask, a list row, a card's question", specimen: "Connect a calendar" },
		{ size: 17, face: "serif", role: "what you are typing", specimen: "It rained most of the afternoon." },
		{ size: 16, face: "serif", role: "what you kept", specimen: "The morning kept its own counsel." },
		{ size: 15, face: "sans", role: "the sentence under the title; a paragraph", specimen: "Your server is reading your life, and tomorrow morning it writes the first page." },
		{ size: 14, face: "sans", role: "buttons, links, list bodies", specimen: "Start the interview" },
		{ size: 13, face: "sans", role: "labels, quiet verbs, notes, dates", specimen: "Read again · measured here · 16 September" },
		{ size: 12, face: "sans", role: "captions in white on the painting", specimen: "since Thursday" },
		{ size: 12, face: "mono", role: "a clock time, and nothing else", specimen: "07:42" },
		{ size: 11, face: "sans", weight: 500, role: "the number inside a stepper dot", specimen: "4" },
	];

	const TYPE_PROHIBITIONS = [
		"The serif is never bold and never italic — JJannon ships one cut and has no italic.",
		"No uppercase serif with tracking.",
		"No mono outside a chart's own axis and a clock time. Mono kickers at 9.5–10.5px were the strongest tell of the dated register.",
		"Nothing under 11px.",
		"No half-pixel sizes (12.5, 13.5).",
	];

	// ========================================================================
	// PRIMITIVE DEMO STATE (all sample content fictional)
	// ========================================================================

	let inputPlain = $state("");
	let inputFilled = $state("Nick");
	let inputLong = $state("a value long enough to run past the right edge of the field and keep going");
	let inputNumber = $state("42");
	let inputEmail = $state("nick@example.com");
	let inputTel = $state("+15125550142");
	let inputPassword = $state("correct-horse");
	let inputDate = $state("2026-09-16");
	let inputClearable = $state("clear me");
	let inputSaveOk = $state("saves fine");
	let inputSaveFail = $state("fails to save");
	let textareaPlain = $state("");
	let textareaFilled = $state("Two notes were handed to David Okafor, and he put them in the file without reading either one.");
	let textareaGrow = $state("This one grows as you type, up to six rows, then scrolls.");

	async function saveOk(): Promise<void> {
		await new Promise((r) => setTimeout(r, 900));
	}

	async function saveFails(): Promise<void> {
		await new Promise((r) => setTimeout(r, 700));
		throw new Error("gallery: deliberate failure, to reach Input's error state");
	}

	const BADGE_VARIANTS = [
		"muted", "success", "error", "warning", "info", "primary",
	] as const;

	let openModal = $state<string | null>(null);

	let subNavRoute = $state("/gallery/second");
	const SUBNAV_ITEMS = [
		{ id: "first", label: "First" },
		{ id: "second", label: "Second", badge: 12 },
		{ id: "third", label: "Third" },
		{ id: "a-much-longer-label", label: "A much longer label" },
	];

	const LOREM_LONG =
		"Supercalifragilisticexpialidocious-antidisestablishmentarianism-pneumonoultramicroscopicsilicovolcanoconiosis";

	// ========================================================================
	// COVERAGE — what this page does NOT show, said out loud
	// ========================================================================

	const NOT_SHOWN = [
		["StateBlock", "landing in the working tree as this page was written — the shared shell under EmptyState / LoadingState / ErrorState. Those three are shown below and exercise it; give StateBlock its own row here the moment it is committed, per the rule at the top of this file"],
		["Markdown", "a render engine, not a primitive — it needs a document and the shiki theme; belongs in its own fixture"],
		["ThinkingBlock", "chat-coupled; needs a streaming turn to be meaningful"],
		["SudoModal", "asks for a real password; deliberately not instantiated on a public route"],
		["citations (InlineCitation, CitationTooltip, CitationPanel, SourcesFooter)", "need live refs and a resolver"],
		["ChatInput, RefPicker, UniversalPicker, DataGrid, ThemePicker, …", "composites, not primitives — they belong in a second gallery once the primitive set is settled"],
	];

	const API_NOTES: { component: string; note: string }[] = [
		{
			component: "Input",
			note: "FIXED 2026-09-16: `error?: boolean | string` now exists, mirroring `warning`, and carries `aria-invalid` + `aria-describedby` — wiring `helperText` and `warning` strings were also missing, so those are announced now too. Until today the error state could only be reached by an `onSave` that REJECTS, which meant a caller doing its own validation had no way to show one. `Textarea` still has the original bug.",
		},
		{
			component: "Button",
			note: "FIXED 2026-09-16: `loading` and `icon` added — the two gaps that made every pending button in the app hand-built. `children` stays REQUIRED, deliberately: an icon-only button is not expressible here, so its accessible name can never go missing. The 59 icon-only call sites need a separate `IconButton`, not a prop on this one.",
		},
		{
			component: "Button",
			note: "`variant=\"primary\"` fills with `--color-secondary`, not `--color-primary` — deliberate (claret is the page's one filled action) but it means the contrast table has to say `surface / secondary` for what a reader calls the primary button. The variant's name and the token's name will disagree for as long as both exist.",
		},
		{
			component: "IconButton + TextAction",
			note: "NEW 2026-09-16, the two shapes `Button` has no form for. A triage found 422 raw `<button>` elements of which only 121 are `Button`: 59 are icon-only chrome (drawn in ten box sizes and six radii, nine of the ten rows violating §6) and 40 are bare text actions (18 independent definitions, 7 color tokens, 7 font sizes). Both are registered here on the day they landed. ZERO call sites are converted yet — that is the next wave, and until it runs these two primitives have no importers outside this page.",
		},
		{
			component: "TextAction",
			note: "`--color-accent` DOES NOT RESOLVE. `wiki/EntityArticleSection.svelte` and `wiki/NotesRail.svelte` both color their `.linkish` with `var(--color-accent, currentColor)`, and `themes.css` defines no `--accent`, so the `@theme inline` bridge never emits one and both have silently rendered `currentColor` on all sixteen themes since they were written. This is the warning at the top of the token table, caught in the wild. TextAction uses `--color-primary`, which §6 names and which resolves.",
		},
		{
			component: "IconButton",
			note: "Four of the hand-rolled icon buttons hover with `--color-background-hover` or `--color-surface-elevated`. Both bridge to `--surface-elevated` — the 3-4% step design.md names as having shipped a dead hover twice. IconButton uses the `--hover-bg` / `--active-bg` foreground-mix ramp. Worth knowing before converting a call site: its hover will get MORE visible, not less, and that is the fix rather than a regression.",
		},
		{
			component: "Button + Act",
			note: "There are still TWO button primitives. `chat/getting-started/ui/Act.svelte` has 16 call sites and a header making the identical argument this one does. It is not rendered on this page because it is a room dialect rather than a library primitive — see the recommendation in the conversion notes. If it is ever merged into `Button`, this row is what should be deleted.",
		},
		{
			component: "Card + Field",
			note: "`<Card list>` used to put `divide-y` on its DIRECT children only, so the one real call site (DeviceView), which wraps its Fields in a `<ul>`, drew no hairlines at all — invisible in review because a lone wrapper has no sibling for `* + *` to match. Fixed 2026-09-16: the rule now matches rows one level down too. Both arrangements are rendered below and both draw.",
		},
		{
			component: "Field",
			note: "Renders a bare `<li>`. It is only valid inside a list element, but `Card` renders a `<div>`, so correct usage requires a wrapper the component does not provide — and that wrapper is what defeats `list`.",
		},
		{
			component: "SubNav",
			note: "Not renderable in isolation. It imports `windowShellStore` and switches tabs by calling `updateTab(tabId, …)`, so it only works inside a real pane. With a synthetic tabId the call no-ops and the underline never moves — the specimen below is therefore inert by construction.",
		},
		{
			component: "Page",
			note: "`h-full` plus `overflow-y-auto`, so it must be given a sized parent. It has no standalone measure of its own; the specimens below are boxed to 320px.",
		},
		{
			component: "EmptyState / ErrorState / LoadingState",
			note: "One block behind three doors, as of the uncommitted `StateBlock.svelte` in the tree. The block is fixed at one size and one padding, so a view that needs a small inline pending row still writes its own — that is the next thing to look at.",
		},
		{
			component: "Modal",
			note: "Its scoped stylesheet reaches for `--surface` and `--border` (raw), while nearly everything else in the app uses the `--color-*` bridge. Both resolve today; only one of them is the convention.",
		},
		{
			component: "Badge + Field",
			note: "Neither can be made to fit. Badge is `white-space: nowrap`, and Field's value column is `shrink-0`, so one long unbroken value widens its row, then its card, then the page. The two overflow specimens below are the only things on this page that had to be wrapped in a scroller.",
		},
		{
			component: "Input / Textarea",
			note: "Two components that should be the same control differ in their vocabulary: Input has `success`, `loading` and `clearable`; Textarea has none of the three. Neither has an `error`. A form built from both cannot give consistent feedback.",
		},
		{
			component: "the library at large",
			note: "The primitives are being rewritten from Tailwind utilities onto scoped CSS and theme tokens WHILE this page is being written — Button, Badge, Card, Field, Page, PageHeading, Section, the three states and SubNav are all modified in the working tree right now. Every note above was re-checked against the files as they stand; some of them may already be answered by the time you read this. The page reads the components, so it will always show what shipped.",
		},
	];

	const SECTIONS = [
		["themes", "Themes"],
		["tokens", "Token sheet"],
		["contrast", "Contrast"],
		["type", "Type scale"],
		["primitives", "Primitives"],
		["notes", "API notes"],
	];
</script>

<svelte:head>
	<title>Components — Virtues design gallery</title>
</svelte:head>

<div class="gallery">
	<!-- ================================================================== -->
	<!-- TOOLBAR                                                            -->
	<!-- ================================================================== -->
	<header class="bar">
		<div class="bar-row">
			<div class="bar-title">
				<strong>Components</strong>
				<span class="bar-sub">design gallery · a tool, not a product surface</span>
			</div>
			<nav class="bar-nav">
				{#each SECTIONS as [id, label] (id)}
					<a href="#{id}">{label}</a>
				{/each}
			</nav>
		</div>
		<div class="bar-row themes-row">
			<span class="bar-label">Theme</span>
			{#each THEMES as t (t.id)}
				<button
					type="button"
					class="theme-chip"
					class:is-active={theme === t.id}
					class:is-canonical={!!t.canonical}
					title={t.note ?? t.canonical ?? t.label}
					onclick={() => (theme = t.id)}
				>
					<span
						class="chip-dot"
						style="background: {readings[t.id]?.raw.background ?? 'transparent'}; border-color: {readings[t.id]?.raw.border ?? 'currentColor'};"
					></span>
					{t.label}
					{#if t.canonical}<span class="chip-tag">{t.canonical}</span>{/if}
				</button>
			{/each}
		</div>
	</header>

	<main class="body">
		<!-- ============================================================== -->
		<!-- WHAT THIS IS                                                   -->
		<!-- ============================================================== -->
		<section class="intro">
			<p>
				Every primitive in <code>$lib/components</code>, in every state it actually has;
				every design token with the value the browser computed for it; the type scale;
				and a WCAG contrast check run across all sixteen themes.
			</p>
			<p class="rule">
				<strong>The rule:</strong> a new primitive must appear here, in the same change
				that adds it. A primitive that is not in the gallery is a primitive nobody can
				review — and it will be re-invented by hand in the next view that needs it.
			</p>
			<p class="meta">
				Switching themes here is visual only. It never calls <code>setTheme</code>, so
				nothing you do on this page changes the theme saved on the box.
				All sample content is fictional.
			</p>
		</section>

		<!-- ============================================================== -->
		<!-- THEMES                                                         -->
		<!-- ============================================================== -->
		<section id="themes">
			<h2>Themes <span class="count">16</span></h2>
			<p class="lede">
				Two are canonical. The other fourteen are costumes — shipped, supported, and
				not what the product is designed in.
			</p>

			<div class="theme-grid">
				{#each THEMES as t (t.id)}
					{@const read = readings[t.id]}
					<button
						type="button"
						class="theme-card"
						class:is-active={theme === t.id}
						onclick={() => (theme = t.id)}
					>
						<div
							class="theme-swatches"
							style="background: {read?.raw.background ?? 'transparent'}"
						>
							{#each ["surface", "foreground", "primary", "secondary", "success", "warning", "error", "info"] as tok (tok)}
								<span
									class="theme-sq"
									style="background: {read?.raw[tok] ?? 'transparent'}; border-color: {read?.raw.border ?? 'currentColor'}"
									title="--{tok}: {read?.raw[tok] ?? '(unset)'}"
								></span>
							{/each}
						</div>
						<div class="theme-meta">
							<span class="theme-name">{t.label}</span>
							<span class="theme-mode">{t.mode}</span>
						</div>
						<code class="theme-attr">data-theme="{t.id}"</code>
						{#if t.canonical}
							<span class="canonical">{t.canonical}</span>
						{/if}
						{#if t.note}
							<span class="theme-note">{t.note}</span>
						{/if}
					</button>
				{/each}
			</div>
		</section>

		<!-- ============================================================== -->
		<!-- TOKEN SHEET                                                    -->
		<!-- ============================================================== -->
		<section id="tokens">
			<h2>Token sheet <span class="count">{activeTheme.label}</span></h2>
			<p class="lede">
				Read at runtime out of the cascade, not copied from <code>themes.css</code>.
				Each row shows the raw token themes.css defines, the <code>--color-*</code>
				bridge components actually write, and the sRGB the browser resolved.
			</p>
			<p class="lede">
				<strong>A bridge reading <em>—</em> is not a typo.</strong> Tailwind emits only
				the theme variables something in the source actually references, so an unread
				<code>--color-*</code> simply does not exist at runtime — today that is
				<code>--color-primary-active</code>, <code>--color-background-inverse</code>,
				all four <code>--color-diff-*</code>, the whole interaction ramp and every
				<code>--color-cat-*</code>. Write one of those names in a new component and it
				will start resolving; read one through a composed string
				(<code>var(--color-{'{'}name{'}'})</code>) and it will not.
			</p>

			{#if !ready}
				<p class="pending">Reading the cascade…</p>
			{:else}
				{#each TOKEN_GROUPS as group (group.name)}
					<h3>{group.name}</h3>
					<p class="group-blurb">{group.blurb}</p>

					{#if group.kind === "text"}
						<table class="sheet">
							<thead>
								<tr><th>token</th><th>--name</th><th>--color-name</th></tr>
							</thead>
							<tbody>
								{#each group.tokens as tok (tok)}
									<tr>
										<td class="mono">--{tok}</td>
										<td class="mono val">{currentRead?.raw[tok] || "—"}</td>
										<td class="mono val dim">{currentRead?.bridged[tok] || "—"}</td>
									</tr>
								{/each}
							</tbody>
						</table>
					{:else}
						<div class="swatches">
							{#each group.tokens as tok (tok)}
								{@const raw = currentRead?.raw[tok] ?? ""}
								{@const bridged = currentRead?.bridged[tok] ?? ""}
								{@const rgb = currentRead?.rgb[tok] ?? null}
								<div class="swatch">
									<div class="chip-wrap">
										<span class="chip" style="background: {raw || bridged || 'transparent'}"></span>
									</div>
									<div class="swatch-text">
										<code class="tok">--{tok}</code>
										<code class="raw">{raw || "—"}</code>
										<code class="bridge">
											--color-{tok}: {bridged || "—"}
										</code>
										<code class="resolved">{rgb ? hex(rgb) : "—"}{rgb && rgb.a < 1 ? ` · alpha ${rgb.a.toFixed(2)}` : ""}</code>
									</div>
								</div>
							{/each}
						</div>
					{/if}
				{/each}
			{/if}
		</section>

		<!-- ============================================================== -->
		<!-- CONTRAST                                                       -->
		<!-- ============================================================== -->
		<section id="contrast">
			<h2>Contrast <span class="count">WCAG 2.1 AA</span></h2>
			<p class="lede">
				{PAIRS.length} pairs × {THEMES.length} themes, computed from the resolved sRGB
				above. Translucent tokens are composited over <code>--background</code> before
				measuring, which is what a reader actually sees. Thresholds: 4.5:1 for body
				text, 3:1 for a hairline or a ring (UI shape). A disabled control is exempt
				from AA; it is measured anyway and marked.
			</p>

			{#if !ready}
				<p class="pending">Computing…</p>
			{:else}
				<div class="verdict" class:bad={failureCount > 0}>
					{#if failureCount === 0}
						No failures.
					{:else}
						<strong>{failureCount}</strong> failing pairs across
						<strong>{failures.length}</strong> of {THEMES.length} themes.
					{/if}
				</div>

				<h3>Every failure, by theme</h3>
				<p class="group-blurb">
					Listed plainly rather than hidden behind a filter. These are real, and most
					of them are shipping.
				</p>
				{#if failures.length === 0}
					<p class="pending">None.</p>
				{:else}
					<div class="fail-list">
						{#each failures as row (row.theme.id)}
							<div class="fail-theme">
								<button type="button" class="fail-head" onclick={() => (theme = row.theme.id)}>
									{row.theme.label}
									<span class="fail-count">{row.failed.length}</span>
									{#if row.theme.canonical}<span class="chip-tag">{row.theme.canonical}</span>{/if}
								</button>
								<table class="sheet">
									<thead>
										<tr><th>pair</th><th>ratio</th><th>needs</th><th>resolved</th><th>where</th></tr>
									</thead>
									<tbody>
										{#each row.failed as r (r.pair.id)}
											<tr>
												<td class="mono">{r.pair.id}</td>
												<td class="mono bad-cell">{fmt(r.ratio)}</td>
												<td class="mono dim">{r.pair.need}</td>
												<td class="mono dim">
													<span class="inline-chip" style="background:{r.fgHex}"></span>{r.fgHex}
													on
													<span class="inline-chip" style="background:{r.bgHex}"></span>{r.bgHex}
												</td>
												<td class="where">{r.pair.where}</td>
											</tr>
										{/each}
									</tbody>
								</table>
							</div>
						{/each}
					</div>
				{/if}

				<h3>{activeTheme.label} — all pairs</h3>
				<table class="sheet">
					<thead>
						<tr><th>pair</th><th>ratio</th><th>needs</th><th>verdict</th><th>resolved</th><th>where</th></tr>
					</thead>
					<tbody>
						{#each currentResults as r (r.pair.id)}
							<tr class:row-fail={r.ratio !== null && !r.pass && !r.pair.exempt}>
								<td class="mono">{r.pair.id}</td>
								<td class="mono">{fmt(r.ratio)}</td>
								<td class="mono dim">{r.pair.need}</td>
								<td class="mono">
									{#if r.ratio === null}
										<span class="dim">unset</span>
									{:else if r.pair.exempt}
										<span class="dim">{r.pass ? "pass" : "fail"} · exempt</span>
									{:else if r.pass}
										<span class="ok">pass</span>
									{:else}
										<span class="bad-cell">FAIL</span>
									{/if}
								</td>
								<td class="mono dim">
									<span class="inline-chip" style="background:{r.fgHex}"></span>{r.fgHex}
									on
									<span class="inline-chip" style="background:{r.bgHex}"></span>{r.bgHex}
								</td>
								<td class="where">{r.pair.where}</td>
							</tr>
						{/each}
					</tbody>
				</table>

				<h3>All themes, all pairs</h3>
				<p class="group-blurb">
					The whole surface at once. Scroll sideways. A red cell is below its
					threshold; a hollow cell is a token the theme does not define.
				</p>
				<div class="matrix-wrap">
					<table class="sheet matrix">
						<thead>
							<tr>
								<th class="sticky-col">theme</th>
								{#each PAIRS as p (p.id)}
									<th title="{p.where} · needs {p.need}:1">{p.id}</th>
								{/each}
							</tr>
						</thead>
						<tbody>
							{#each allResults as row (row.theme.id)}
								<tr>
									<th class="sticky-col" class:is-active={theme === row.theme.id}>
										<button type="button" onclick={() => (theme = row.theme.id)}>
											{row.theme.label}
										</button>
									</th>
									{#each row.results as r (r.pair.id)}
										<td
											class="mono cell"
											class:cell-fail={r.ratio !== null && !r.pass && !r.pair.exempt}
											class:cell-exempt={r.pair.exempt}
											class:cell-unset={r.ratio === null}
											title="{row.theme.label} · {r.pair.id} · {fmt(r.ratio)}:1 (needs {r.pair.need})"
										>{fmt(r.ratio)}</td>
									{/each}
								</tr>
							{/each}
						</tbody>
					</table>
				</div>
			{/if}
		</section>

		<!-- ============================================================== -->
		<!-- TYPE                                                           -->
		<!-- ============================================================== -->
		<section id="type">
			<h2>Type scale</h2>
			<p class="lede">
				Three faces, the ones <code>themes.css</code> declares, at the sizes
				<code>agents/build/design-grammar.md</code> §4 specifies. JJannon ships
				<strong>one cut</strong> — the weights and the italic below are shown as
				prohibitions, not as options.
			</p>

			<div class="faces">
				{#each [["--font-serif", "JJannon", "prose"], ["--font-serif-ui", "JJannon UI", "chrome — metrics normalized so a centered line box centers the letters"], ["--font-sans", "Avenir", "everything that is not prose"], ["--font-mono", "IBM Plex Mono", "a chart axis and a clock time, nothing else"]] as [tok, name, use] (tok)}
					<div class="face">
						<code class="tok">{tok}</code>
						<span class="face-name">{name}</span>
						<span class="face-use">{use}</span>
						<code class="raw">{currentRead?.raw[tok.slice(2)] || currentRead?.bridged[tok.slice(2)] || "—"}</code>
					</div>
				{/each}
			</div>

			<table class="sheet type-table">
				<thead>
					<tr><th>px</th><th>face</th><th>role</th><th>specimen</th></tr>
				</thead>
				<tbody>
					{#each TYPE_SCALE as row (`${row.size}-${row.face}`)}
						<tr>
							<td class="mono">{row.size}</td>
							<td class="mono dim">{row.face}{row.weight ? ` ${row.weight}` : ""}</td>
							<td class="where">{row.role}</td>
							<td>
								<span
									class="specimen"
									style="font-family: var(--font-{row.face}); font-size: {row.size}px; font-weight: {row.weight ?? 400};"
								>{row.specimen}</span>
							</td>
						</tr>
					{/each}
				</tbody>
			</table>

			<h3>Prohibitions, each of which has shipped</h3>
			<ul class="prohibitions">
				{#each TYPE_PROHIBITIONS as p (p)}
					<li>{p}</li>
				{/each}
			</ul>

			<h3>What the serif does when you ask it for a weight it does not have</h3>
			<p class="group-blurb">
				Shown so the synthetic result is recognizable on sight. None of the three on
				the right are legal in this product.
			</p>
			<div class="serif-demo">
				<span style="font-family: var(--font-serif); font-size: 26px;">The only cut there is</span>
				<span style="font-family: var(--font-serif); font-size: 26px; font-weight: 700;">Faked bold</span>
				<span style="font-family: var(--font-serif); font-size: 26px; font-style: italic;">Faked italic</span>
				<span style="font-family: var(--font-serif); font-size: 26px; text-transform: uppercase; letter-spacing: 0.12em;">Tracked caps</span>
			</div>
		</section>

		<!-- ============================================================== -->
		<!-- PRIMITIVES                                                     -->
		<!-- ============================================================== -->
		<section id="primitives">
			<h2>Primitives</h2>
			<p class="lede">
				Real components, real props. Nothing below is a mock-up, and no variant is
				invented — where a state cannot be reached from the outside, the specimen says
				so instead of faking it.
			</p>

			<!-- ---------------------------------------------------------- -->
			<h3>Button</h3>
			<p class="group-blurb">
				<code>variant</code>: primary · secondary · ghost · danger.
				<code>size</code>: sm · md · lg. <code>disabled</code>.
				No loading state and no icon slot — see API notes.
			</p>
			<div class="spec">
				{#each ["primary", "secondary", "ghost", "danger"] as v (v)}
					<div class="spec-row">
						<span class="spec-label">{v}</span>
						<div class="spec-items">
							{#each ["sm", "md", "lg"] as s (s)}
								<Button variant={v as "primary" | "secondary" | "ghost" | "danger"} size={s as "sm" | "md" | "lg"}>
									{v} {s}
								</Button>
							{/each}
							<Button variant={v as "primary" | "secondary" | "ghost" | "danger"} disabled>disabled</Button>
							<Button variant={v as "primary" | "secondary" | "ghost" | "danger"}>
								A verb that names the place and runs long
							</Button>
						</div>
					</div>
				{/each}
				<div class="spec-row">
					<span class="spec-label">type</span>
					<div class="spec-items">
						<Button type="submit">submit</Button>
						<Button type="reset" variant="secondary">reset</Button>
					</div>
				</div>
			</div>

			<!-- ---------------------------------------------------------- -->
			<h3>IconButton</h3>
			<p class="group-blurb">
				The icon-only button <code>Button</code> deliberately refuses.
				<code>size</code>: xs · sm · md · touch (20 / 24 / 28 / 44px).
				<code>variant</code>: ghost · secondary · danger. <code>label</code> is
				REQUIRED and feeds both <code>aria-label</code> and <code>title</code>.
				<code>pressed</code>, <code>expanded</code>, <code>haspopup</code>,
				<code>disabled</code>. One radius (6px) at every size — it replaces ten
				box sizes and six radii found across 59 hand-rolled call sites.
			</p>
			<div class="spec">
				{#each ["ghost", "secondary", "danger"] as v (v)}
					<div class="spec-row">
						<span class="spec-label">{v}</span>
						<div class="spec-items">
							{#each ["xs", "sm", "md", "touch"] as s (s)}
								<IconButton
									icon="ri:close-line"
									label="Close ({v} {s})"
									variant={v as "ghost" | "secondary" | "danger"}
									size={s as "xs" | "sm" | "md" | "touch"}
								/>
							{/each}
							<IconButton
								icon="ri:delete-bin-line"
								label="Disabled"
								variant={v as "ghost" | "secondary" | "danger"}
								disabled
							/>
						</div>
					</div>
				{/each}
				<div class="spec-row">
					<span class="spec-label">pressed (aria-pressed)</span>
					<div class="spec-items">
						{#each ["xs", "sm", "md", "touch"] as s (s)}
							<IconButton
								icon="ri:bold"
								label="Bold ({s})"
								size={s as "xs" | "sm" | "md" | "touch"}
								pressed
							/>
						{/each}
						<IconButton icon="ri:bold" label="Bold, off" pressed={false} />
						<IconButton icon="ri:bold" label="Bold, pressed and disabled" pressed disabled />
					</div>
				</div>
				<div class="spec-row">
					<span class="spec-label">popover trigger</span>
					<div class="spec-items">
						<IconButton icon="ri:filter-3-line" label="Add filter" haspopup="menu" expanded={false} />
						<IconButton icon="ri:more-line" label="More actions" haspopup="menu" expanded={true} />
						<IconButton icon="ri:calendar-line" label="Pick a day" variant="secondary" haspopup="dialog" expanded={false} />
					</div>
				</div>
				<div class="spec-row">
					<span class="spec-label">in a dense row (the real case)</span>
					<div class="spec-items">
						<span class="ib-row">
							<IconButton icon="ri:arrow-left-s-line" label="Back" size="xs" />
							<IconButton icon="ri:arrow-right-s-line" label="Forward" size="xs" disabled />
							<span class="ib-rowlabel">September 2026</span>
							<IconButton icon="ri:close-line" label="Close this tab" size="xs" />
						</span>
					</div>
				</div>
				<p class="group-blurb">
					NOTE: the 44pt touch floor is a transparent pseudo-element under
					<code>(pointer: coarse)</code>, so it is invisible here on a mouse and
					changes no layout. To see it, emulate a touch device — the painted
					boxes stay 20 / 24 / 28px and the hit areas become 44px.
				</p>
			</div>

			<!-- ---------------------------------------------------------- -->
			<h3>TextAction</h3>
			<p class="group-blurb">
				A verb you can press with no box around it — grammar §6's "quiet link in
				<code>--color-primary</code> at 14px/500 sans". Two independent booleans
				rather than a <code>variant</code>: <code>quiet</code> (the way back) and
				<code>inline</code> (sits mid-sentence, inherits the paragraph's face).
				<code>loading</code> + <code>loadingLabel</code>, <code>disabled</code>,
				<code>href</code>. It replaces 18 independent hand-rolled definitions
				using 7 color tokens and 7 font sizes.
			</p>
			<div class="spec">
				<div class="spec-row">
					<span class="spec-label">standalone</span>
					<div class="spec-items">
						<TextAction>Write the article</TextAction>
						<TextAction quiet>Not now</TextAction>
						<TextAction disabled>Disabled</TextAction>
						<TextAction quiet disabled>Disabled, quiet</TextAction>
					</div>
				</div>
				<div class="spec-row">
					<span class="spec-label">loading</span>
					<div class="spec-items">
						<TextAction loading loadingLabel="Writing…">Write the article</TextAction>
						<TextAction loading>Keeps its own label</TextAction>
						<TextAction quiet loading loadingLabel="Saving…">Save the endpoint</TextAction>
					</div>
				</div>
				<div class="spec-row">
					<span class="spec-label">href (renders an &lt;a&gt;)</span>
					<div class="spec-items">
						<TextAction href="/components">Reconnect</TextAction>
						<TextAction href="/components" quiet>Billing</TextAction>
						<TextAction href="/components" disabled>href + disabled → a button</TextAction>
					</div>
				</div>
				<div class="spec-row">
					<span class="spec-label">inline, in the sans</span>
					<div class="spec-items">
						<p class="ta-prose-sans">
							Nothing has been recorded here yet.
							<TextAction inline>Write the article</TextAction>, or
							<TextAction inline quiet>keep this updated</TextAction>. It takes the
							size and face of the paragraph rather than 14px sans, which is what
							stops a word jumping out of its own line.
						</p>
					</div>
				</div>
				<div class="spec-row">
					<span class="spec-label">inline, in the serif</span>
					<div class="spec-items">
						<p class="ta-prose-serif">
							The wiki's prose is JJannon, and eleven of the call sites this
							replaces sit inside a paragraph exactly like this one —
							<TextAction inline>Rename</TextAction> ·
							<TextAction inline quiet>Unname</TextAction> — so an inline action
							inherits the serif and contributes only its color and its rule.
						</p>
					</div>
				</div>
				<div class="spec-row">
					<span class="spec-label">long label, wrapping inline</span>
					<div class="spec-items clip" style="max-width: 260px">
						<p class="ta-prose-sans">
							<TextAction inline>{LOREM_LONG}</TextAction>
						</p>
					</div>
				</div>
			</div>

			<!-- ---------------------------------------------------------- -->
			<h3>Badge</h3>
			<p class="group-blurb">
				<code>variant</code>: muted · success · error · warning · info · primary.
				<code>outline</code> and <code>uppercase</code> are independent booleans.
				The face and size are fixed by the component, not by the caller.
			</p>
			<div class="spec">
				<div class="spec-row">
					<span class="spec-label">fill</span>
					<div class="spec-items">
						{#each BADGE_VARIANTS as v (v)}
							<Badge variant={v}>{v}</Badge>
						{/each}
					</div>
				</div>
				<div class="spec-row">
					<span class="spec-label">outline</span>
					<div class="spec-items">
						{#each BADGE_VARIANTS as v (v)}
							<Badge variant={v} outline>{v}</Badge>
						{/each}
					</div>
				</div>
				<div class="spec-row">
					<span class="spec-label">uppercase</span>
					<div class="spec-items">
						{#each BADGE_VARIANTS.slice(0, 3) as v (v)}
							<Badge variant={v} uppercase>{v}</Badge>
							<Badge variant={v} outline uppercase>{v}</Badge>
						{/each}
					</div>
				</div>
				<div class="spec-row">
					<span class="spec-label">overflow (clipped by the gallery)</span>
					<div class="spec-items clip" style="max-width: 220px">
						<Badge variant="info">{LOREM_LONG}</Badge>
					</div>
				</div>
			</div>

			<!-- ---------------------------------------------------------- -->
			<h3>Card</h3>
			<p class="group-blurb">
				Two arrangements only: one padded surface, or <code>list</code> (rows divided
				by hairlines, each row padding itself). <code>padding</code>: none · sm · md,
				ignored when <code>list</code>.
			</p>
			<div class="spec cards">
				<div>
					<span class="spec-label">default (md)</span>
					<Card>
						<p class="card-body">One padded surface. A hairline on <code>--surface</code>, no shadow.</p>
					</Card>
				</div>
				<div>
					<span class="spec-label">padding="sm"</span>
					<Card padding="sm"><p class="card-body">Tighter.</p></Card>
				</div>
				<div>
					<span class="spec-label">padding="none"</span>
					<Card padding="none"><p class="card-body" style="padding: 4px">The card supplies nothing.</p></Card>
				</div>
				<div>
					<span class="spec-label">overflow</span>
					<Card><p class="card-body">{LOREM_LONG}</p></Card>
				</div>
			</div>

			<!-- ---------------------------------------------------------- -->
			<h3>Card list + Field</h3>
			<p class="group-blurb">
				Field renders a bare <code>&lt;li&gt;</code>; Card renders a
				<code>&lt;div&gt;</code>, so valid markup needs the wrapper. The two
				arrangements below are the same component pair, and both draw their
				hairlines. Until 2026-09-16 the left one — how the only real call site
				writes it — drew none: the divider rule matched direct children, and a
				lone <code>&lt;ul&gt;</code> has no sibling to match against. This page is
				what caught it.
			</p>
			<div class="spec two-up">
				<div>
					<span class="spec-label">as DeviceView writes it — &lt;ul&gt; inside</span>
					<Card list>
						<ul>
							<Field label="App" hint="the shell, and what it can do" value="1.0.27" note="current" tone="success" mono />
							<Field label="Interface" hint="the screens you are looking at" value="1.0.26" note="ten days behind" tone="warning" mono />
							<Field label="Collector" hint="never reported its build" unknown />
						</ul>
					</Card>
				</div>
				<div>
					<span class="spec-label">rows as direct children — hairlines draw</span>
					<Card list>
						<Field label="App" hint="the shell, and what it can do" value="1.0.27" note="current" tone="success" mono />
						<Field label="Interface" hint="the screens you are looking at" value="1.0.26" note="ten days behind" tone="warning" mono />
						<Field label="Collector" hint="never reported its build" unknown />
					</Card>
				</div>
			</div>

			<p class="group-blurb">
				Field's three renderings of a missing value, side by side — the distinction
				the component exists for.
			</p>
			<div class="spec clip">
				<Card list>
					<Field label="value" hint="we asked, this is the answer" value="1.0.27" mono />
					<Field label="none" hint="we asked; there is genuinely nothing" value={null} />
					<Field label="unknown" hint="we could not ask — hover the rule" unknown />
					<Field label="tone: muted" value="—" note="a quiet note" tone="muted" />
					<Field label="tone: info" value="—" note="an informational note" tone="info" />
					<Field label="tone: success" value="—" note="a good note" tone="success" />
					<Field label="tone: warning" value="—" note="a note that wants attention" tone="warning" />
					<Field label="mono" value="sha256:4f3c2b1a" mono />
					<Field label="with an action" hint="trailing control" value="staged">
						{#snippet action()}
							<Button variant="secondary" size="sm">Relaunch</Button>
						{/snippet}
					</Field>
					<Field
						label="A label long enough to test what happens when the left column will not fit on one line"
						hint="and a hint that also runs past the measure, because real settings rows do"
						value={LOREM_LONG}
						note="and a note under it"
						tone="warning"
						mono
					/>
				</Card>
			</div>

			<!-- ---------------------------------------------------------- -->
			<h3>Section</h3>
			<p class="group-blurb">
				One labeled band. <code>note</code> is the right-margin aside — provenance,
				scope, units. <code>first</code> drops the top margin.
			</p>
			<div class="spec boxed">
				<Section title="Vitals" first>
					<Card><p class="card-body">first = true, so no top margin.</p></Card>
				</Section>
				<Section title="Software" note="measured here">
					<Card><p class="card-body">With a note.</p></Card>
				</Section>
				<Section title="Processes" note="as your server heard it">
					<Card><p class="card-body">Sysadmin voice — the words an operator would use.</p></Card>
				</Section>
			</div>

			<!-- ---------------------------------------------------------- -->
			<h3>PageHeading</h3>
			<p class="group-blurb">
				<code>level</code> 1 · 2 · 3, each serif. <code>description</code> and an
				<code>actions</code> snippet.
			</p>
			<div class="spec boxed">
				<PageHeading title="Level 1" description="The sentence under the title, in the sans." />
				<PageHeading level={2} title="Level 2" description="Smaller." />
				<PageHeading level={3} title="Level 3" />
				<PageHeading title="With actions" description="The actions slot is right-aligned and does not shrink.">
					{#snippet actions()}
						<Button variant="secondary" size="sm">Change</Button>
						<Button size="sm">Start the interview</Button>
					{/snippet}
				</PageHeading>
				<PageHeading
					title="A title long enough that it has to wrap, which is worth seeing next to an action that must not shrink"
					description="And a description that runs on as well, so the two-column behavior of the heading row is legible."
				>
					{#snippet actions()}
						<Button size="sm">Act</Button>
					{/snippet}
				</PageHeading>
			</div>

			<!-- ---------------------------------------------------------- -->
			<h3>Page</h3>
			<p class="group-blurb">
				<code>maxWidth</code>: none · prose · wide. <code>padding</code>: default ·
				compact · none. Each specimen is boxed to 320px because Page is
				<code>h-full</code> and has no measure of its own.
			</p>
			<div class="spec two-up">
				{#each [["prose", "default"], ["wide", "compact"]] as [w, p] (w)}
					<div>
						<span class="spec-label">maxWidth="{w}" padding="{p}"</span>
						<div class="page-box">
							<Page
								maxWidth={w as "none" | "prose" | "wide"}
								padding={p as "default" | "compact" | "none"}
								title="Sources"
								description="Everything your server is reading."
							>
								{#snippet actions()}
									<Button size="sm">Add a source</Button>
								{/snippet}
								<Card><p class="card-body">Page content sits under the heading.</p></Card>
							</Page>
						</div>
					</div>
				{/each}
			</div>

			<!-- ---------------------------------------------------------- -->
			<h3>Input</h3>
			<p class="group-blurb">
				Types, labels, helper text, and the four feedback states.
				<strong>error</strong> became a real prop on 2026-09-16; until then the
				only way in was an <code>onSave</code> that rejects, which the last
				specimen still demonstrates — click into it, change it, blur.
			</p>
			<div class="spec inputs">
				<div><span class="spec-label">plain</span><Input bind:value={inputPlain} placeholder="Type here" /></div>
				<div><span class="spec-label">label + helper</span><Input bind:value={inputFilled} label="What should your server call you?" helperText="A first name is plenty." /></div>
				<div><span class="spec-label">required</span><Input bind:value={inputPlain} label="Required" required placeholder="Needed" /></div>
				<div><span class="spec-label">disabled</span><Input value="Cannot be edited" label="Disabled" disabled /></div>
				<div><span class="spec-label">success</span><Input bind:value={inputFilled} label="Success" success helperText="Accepted." /></div>
				<div><span class="spec-label">loading</span><Input bind:value={inputFilled} label="Loading" loading helperText="Checking…" /></div>
				<div><span class="spec-label">warning (boolean)</span><Input bind:value={inputFilled} label="Warning" warning helperText="Plain helper text." /></div>
				<div><span class="spec-label">warning (string)</span><Input bind:value={inputFilled} label="Warning with message" warning="That name is already taken." /></div>
				<div><span class="spec-label">clearable</span><Input bind:value={inputClearable} label="Clearable" clearable /></div>
				<div><span class="spec-label">delight = false</span><Input bind:value={inputFilled} label="No typing affordance" delight={false} /></div>
				<div><span class="spec-label">type=number</span><Input bind:value={inputNumber} type="number" label="Number" min={0} max={100} step={1} /></div>
				<div><span class="spec-label">type=email</span><Input bind:value={inputEmail} type="email" label="Email" /></div>
				<div><span class="spec-label">type=tel</span><Input bind:value={inputTel} type="tel" label="Phone" /></div>
				<div><span class="spec-label">type=password</span><Input bind:value={inputPassword} type="password" label="Password" /></div>
				<div><span class="spec-label">type=date</span><Input bind:value={inputDate} type="date" label="Date" /></div>
				<div><span class="spec-label">overflow</span><Input bind:value={inputLong} label="A value past the edge" /></div>
				<div><span class="spec-label">autoSave — resolves</span><Input bind:value={inputSaveOk} label="Saves" autoSave onSave={saveOk} helperText="Edit and blur to see the check." /></div>
				<div><span class="spec-label">autoSave — rejects (the error state)</span><Input bind:value={inputSaveFail} label="Fails to save" autoSave onSave={saveFails} helperText="Edit and blur: this is the only route to .error." /></div>
			</div>

			<!-- ---------------------------------------------------------- -->
			<h3>Textarea</h3>
			<p class="group-blurb">
				Same feedback vocabulary as Input, minus <code>success</code>,
				<code>loading</code> and <code>clearable</code> — a divergence worth noticing.
				<code>autoResize</code> with <code>maxRows</code> grows then scrolls.
			</p>
			<div class="spec inputs">
				<div><span class="spec-label">plain</span><Textarea bind:value={textareaPlain} placeholder="Say something" /></div>
				<div><span class="spec-label">label + helper</span><Textarea bind:value={textareaFilled} label="Notes" helperText="Markdown is not rendered here." /></div>
				<div><span class="spec-label">rows = 2</span><Textarea bind:value={textareaFilled} rows={2} label="Two rows" /></div>
				<div><span class="spec-label">autoResize, maxRows = 6</span><Textarea bind:value={textareaGrow} autoResize rows={2} maxRows={6} label="Grows" /></div>
				<div><span class="spec-label">disabled</span><Textarea value="Cannot be edited" label="Disabled" disabled /></div>
				<div><span class="spec-label">warning (string)</span><Textarea bind:value={textareaFilled} label="Warning" warning="Too long for one page." /></div>
				<div><span class="spec-label">required</span><Textarea bind:value={textareaPlain} label="Required" required /></div>
			</div>

			<!-- ---------------------------------------------------------- -->
			<h3>EmptyState · LoadingState · ErrorState</h3>
			<p class="group-blurb">
				Three doors onto one block (<code>StateBlock</code>). They are shown together
				because the remaining question is a shared one: the block has a single size
				and a single padding, so a view that wants a small inline pending row still
				writes its own.
			</p>
			<div class="spec three-up">
				<div>
					<span class="spec-label">EmptyState — default icon</span>
					<Card padding="none"><EmptyState title="Nothing here yet" message="When a source reports, it will show up in this list." /></Card>
				</div>
				<div>
					<span class="spec-label">EmptyState — icon + actions</span>
					<Card padding="none">
						<EmptyState icon="ri:calendar-line" title="No calendar connected" message="Connect one and your server can read your days.">
							{#snippet actions()}
								<Button size="sm">Open sources</Button>
							{/snippet}
						</EmptyState>
					</Card>
				</div>
				<div>
					<span class="spec-label">EmptyState — icon only</span>
					<Card padding="none"><EmptyState icon="ri:inbox-archive-line" /></Card>
				</div>
				<div>
					<span class="spec-label">LoadingState — with message</span>
					<Card padding="none"><LoadingState message="Reading your record…" /></Card>
				</div>
				<div>
					<span class="spec-label">LoadingState — bare</span>
					<Card padding="none"><LoadingState /></Card>
				</div>
				<div>
					<span class="spec-label">ErrorState — default title</span>
					<Card padding="none"><ErrorState /></Card>
				</div>
				<div>
					<span class="spec-label">ErrorState — message</span>
					<Card padding="none"><ErrorState title="Could not reach your server" message="It answered once, eight minutes ago, and has not since." /></Card>
				</div>
				<div>
					<span class="spec-label">ErrorState — with retry</span>
					<Card padding="none">
						<ErrorState title="Could not reach your server" message="Retry is the library's own Button, so it moves when Button does." onRetry={() => {}} />
					</Card>
				</div>
				<div>
					<span class="spec-label">ErrorState — overflow</span>
					<Card padding="none"><ErrorState title="A failure with a long name" message={LOREM_LONG} /></Card>
				</div>
			</div>

			<!-- ---------------------------------------------------------- -->
			<h3>Icon</h3>
			<p class="group-blurb">
				A thin wrapper over <code>@iconify/svelte</code> with the app's registry
				pre-loaded. Sizes are passed as <code>width</code>; there are no variants.
			</p>
			<div class="spec">
				<div class="spec-row">
					<span class="spec-label">sizes</span>
					<div class="spec-items icons">
						{#each [14, 16, 18, 20, 24, 32, 40] as w (w)}
							<span class="icon-cell"><Icon icon="ri:book-2-line" width={w} /><em>{w}</em></span>
						{/each}
					</div>
				</div>
				<div class="spec-row">
					<span class="spec-label">the ones the shell uses</span>
					<div class="spec-items icons">
						{#each ["ri:inbox-line", "ri:error-warning-line", "ri:loader-4-line", "ri:close-line", "ri:refresh-line", "ri:calendar-line", "ri:search-line", "ri:settings-3-line", "ri:check-line", "ri:more-2-line"] as name (name)}
							<span class="icon-cell"><Icon icon={name} width={20} /><em>{name.replace("ri:", "")}</em></span>
						{/each}
					</div>
				</div>
			</div>

			<!-- ---------------------------------------------------------- -->
			<h3>Modal</h3>
			<p class="group-blurb">
				Portals to <code>&lt;body&gt;</code>, closes on backdrop click and Escape.
				<code>width</code>: sm · md · lg. Its <code>&lt;style&gt;</code> reaches for
				the raw <code>--surface</code> / <code>--border</code> rather than the
				<code>--color-*</code> bridge the rest of the app writes.
			</p>
			<div class="spec">
				<div class="spec-row">
					<span class="spec-label">open one</span>
					<div class="spec-items">
						<Button variant="secondary" onclick={() => (openModal = "sm")}>width=sm</Button>
						<Button variant="secondary" onclick={() => (openModal = "md")}>width=md</Button>
						<Button variant="secondary" onclick={() => (openModal = "lg")}>width=lg</Button>
						<Button variant="secondary" onclick={() => (openModal = "footer")}>with footer</Button>
						<Button variant="secondary" onclick={() => (openModal = "untitled")}>no title</Button>
						<Button variant="secondary" onclick={() => (openModal = "long")}>long content</Button>
					</div>
				</div>
			</div>

			<Modal open={openModal === "sm"} onClose={() => (openModal = null)} title="Small" width="sm">
				<p class="card-body">360px.</p>
			</Modal>
			<Modal open={openModal === "md"} onClose={() => (openModal = null)} title="Medium" width="md">
				<p class="card-body">480px — the default.</p>
			</Modal>
			<Modal open={openModal === "lg"} onClose={() => (openModal = null)} title="Large" width="lg">
				<p class="card-body">640px.</p>
			</Modal>
			<Modal open={openModal === "footer"} onClose={() => (openModal = null)} title="Unpair this device?" width="md">
				<p class="card-body">It will stop reporting, and its key is discarded. You can pair it again later.</p>
				{#snippet footer()}
					<Button variant="ghost" onclick={() => (openModal = null)}>Stop for now</Button>
					<Button variant="danger" onclick={() => (openModal = null)}>Unpair</Button>
				{/snippet}
			</Modal>
			<Modal open={openModal === "untitled"} onClose={() => (openModal = null)}>
				<p class="card-body">No title, so no header row and no close button — Escape or the backdrop are the only ways out.</p>
			</Modal>
			<Modal open={openModal === "long"} onClose={() => (openModal = null)} title="Long content" width="md">
				{#each Array.from({ length: 24 }, (_, i) => i + 1) as n (n)}
					<p class="card-body">Paragraph {n}. The body scrolls; the backdrop keeps a safe-area inset at the bottom.</p>
				{/each}
			</Modal>

			<!-- ---------------------------------------------------------- -->
			<h3>SubNav</h3>
			<p class="group-blurb">
				<strong>Inert here.</strong> SubNav derives its active segment from a route it
				does not own, and switches by calling <code>windowShellStore.updateTab()</code>,
				which returns early for a tab that does not exist. With a synthetic
				<code>tabId</code> nothing moves. What the specimen still shows honestly: the
				resting chrome, the badge, the inset, and the divider seam.
			</p>
			<div class="spec boxed">
				<span class="spec-label">insetX="0"</span>
				<SubNav tabId="gallery-not-a-real-tab" route={subNavRoute} base="/gallery" default="first" items={SUBNAV_ITEMS} insetX="0" />
				<span class="spec-label">divider</span>
				<SubNav tabId="gallery-not-a-real-tab" route={subNavRoute} base="/gallery" default="first" items={SUBNAV_ITEMS} insetX="0" divider />
				<span class="spec-label">custom item snippet</span>
				<SubNav tabId="gallery-not-a-real-tab" route={subNavRoute} base="/gallery" default="first" items={SUBNAV_ITEMS} insetX="0">
					{#snippet item(it, isActive)}
						<Icon icon={isActive ? "ri:checkbox-blank-circle-fill" : "ri:checkbox-blank-circle-line"} width={8} />
						{it.label}
					{/snippet}
				</SubNav>
				<div class="spec-items" style="margin-top: 12px">
					<span class="spec-label">route (the only thing that drives it):</span>
					{#each SUBNAV_ITEMS as it (it.id)}
						<Button size="sm" variant="ghost" onclick={() => (subNavRoute = `/gallery/${it.id}`)}>
							/gallery/{it.id}
						</Button>
					{/each}
					<code class="raw">{subNavRoute}</code>
				</div>
			</div>
		</section>

		<!-- ============================================================== -->
		<!-- NOTES                                                          -->
		<!-- ============================================================== -->
		<section id="notes">
			<h2>API notes</h2>
			<p class="lede">
				Everything that made a component awkward to render honestly. Each of these is
				a place the library disagrees with itself, and therefore direct evidence for a
				consolidation pass.
			</p>
			<table class="sheet">
				<thead><tr><th>component</th><th>what got in the way</th></tr></thead>
				<tbody>
					{#each API_NOTES as n, i (i)}
						<tr><td class="mono">{n.component}</td><td class="where">{n.note}</td></tr>
					{/each}
				</tbody>
			</table>

			<h3>Not shown here, and why</h3>
			<table class="sheet">
				<thead><tr><th>component</th><th>reason</th></tr></thead>
				<tbody>
					{#each NOT_SHOWN as [name, why] (name)}
						<tr><td class="mono">{name}</td><td class="where">{why}</td></tr>
					{/each}
				</tbody>
			</table>
		</section>

		<footer class="foot">
			<p>
				<code>apps/web/src/routes/(public)/components/+page.svelte</code> — one file, no
				dependencies outside <code>$lib/components</code>. Add a primitive, add it here.
			</p>
		</footer>
	</main>
</div>

<style>
	/* Plain CSS throughout, on theme tokens only. The gallery's own chrome must
	   survive all sixteen themes as well as the specimens do, or a theme
	   regression could hide inside the tool meant to catch it. */

	.gallery {
		min-height: 100vh;
		background: var(--color-background);
		color: var(--color-foreground);
		font-family: var(--font-sans);
		font-size: 14px;
		line-height: 1.5;
	}

	/* ---- toolbar ---- */
	.bar {
		position: sticky;
		top: 0;
		z-index: 20;
		background: var(--color-surface);
		border-bottom: 1px solid var(--color-border);
		padding: 10px 20px;
	}
	.bar-row {
		display: flex;
		align-items: center;
		gap: 12px;
		flex-wrap: wrap;
	}
	.themes-row {
		margin-top: 8px;
		gap: 6px;
	}
	.bar-title strong {
		font-family: var(--font-serif-ui);
		font-size: 18px;
		font-weight: 400;
	}
	.bar-sub,
	.bar-label {
		font-size: 12px;
		color: var(--color-foreground-subtle);
		margin-left: 8px;
	}
	.bar-label {
		margin-left: 0;
		text-transform: lowercase;
	}
	.bar-nav {
		margin-left: auto;
		display: flex;
		gap: 14px;
	}
	.bar-nav a {
		font-size: 13px;
		color: var(--color-primary);
		text-decoration: none;
	}
	.bar-nav a:hover {
		text-decoration: underline;
	}

	.theme-chip {
		display: inline-flex;
		align-items: center;
		gap: 6px;
		padding: 3px 8px;
		font-size: 12px;
		font-family: var(--font-sans);
		color: var(--color-foreground-muted);
		background: transparent;
		border: 1px solid var(--color-border);
		border-radius: 999px;
		cursor: pointer;
	}
	.theme-chip:hover {
		background: var(--hover-bg);
	}
	.theme-chip.is-active {
		color: var(--color-foreground);
		border-color: var(--color-border-strong);
		background: var(--active-bg);
	}
	.theme-chip.is-canonical {
		border-color: var(--color-primary);
	}
	.chip-dot {
		width: 10px;
		height: 10px;
		border-radius: 999px;
		border: 1px solid var(--color-border);
		display: inline-block;
	}
	.chip-tag {
		font-size: 10px;
		font-family: var(--font-mono);
		color: var(--color-primary);
	}

	/* ---- body ---- */
	.body {
		max-width: 1400px;
		margin: 0 auto;
		padding: 24px 20px 96px;
	}
	section {
		margin-top: 56px;
		scroll-margin-top: 110px;
	}
	.intro {
		margin-top: 8px;
		max-width: 62ch;
	}
	.intro p {
		margin: 0 0 8px;
		color: var(--color-foreground-muted);
	}
	.intro .rule {
		color: var(--color-foreground);
	}
	.intro .meta {
		font-size: 12px;
		color: var(--color-foreground-subtle);
	}
	h2 {
		font-family: var(--font-serif-ui);
		font-size: 26px;
		font-weight: 400;
		margin: 0 0 4px;
		color: var(--color-foreground);
	}
	h3 {
		font-family: var(--font-serif-ui);
		font-size: 18px;
		font-weight: 400;
		margin: 32px 0 4px;
		color: var(--color-foreground);
		border-top: 1px solid var(--color-border-subtle);
		padding-top: 14px;
	}
	.count {
		font-family: var(--font-mono);
		font-size: 11px;
		color: var(--color-foreground-subtle);
		margin-left: 8px;
	}
	.lede,
	.group-blurb {
		margin: 0 0 14px;
		max-width: 76ch;
		font-size: 13px;
		color: var(--color-foreground-muted);
	}
	.group-blurb {
		font-size: 12px;
		color: var(--color-foreground-subtle);
	}
	.pending {
		font-size: 13px;
		color: var(--color-foreground-subtle);
	}
	code {
		font-family: var(--font-mono);
		font-size: 0.92em;
	}

	/* ---- theme grid ---- */
	.theme-grid {
		display: grid;
		grid-template-columns: repeat(auto-fill, minmax(230px, 1fr));
		gap: 10px;
	}
	.theme-card {
		display: flex;
		flex-direction: column;
		gap: 5px;
		text-align: left;
		padding: 10px;
		background: var(--color-surface);
		border: 1px solid var(--color-border);
		border-radius: 8px;
		cursor: pointer;
		color: inherit;
		font: inherit;
	}
	.theme-card.is-active {
		border-color: var(--color-primary);
	}
	.theme-swatches {
		display: flex;
		gap: 3px;
		padding: 8px;
		border-radius: 6px;
		border: 1px solid var(--color-border-subtle);
	}
	.theme-sq {
		width: 18px;
		height: 18px;
		border-radius: 3px;
		border: 1px solid;
	}
	.theme-meta {
		display: flex;
		align-items: baseline;
		gap: 8px;
	}
	.theme-name {
		font-family: var(--font-serif-ui);
		font-size: 15px;
	}
	.theme-mode {
		font-family: var(--font-mono);
		font-size: 10px;
		color: var(--color-foreground-subtle);
	}
	.theme-attr,
	.theme-note,
	.canonical {
		font-size: 11px;
	}
	.theme-attr {
		color: var(--color-foreground-subtle);
	}
	.canonical {
		font-family: var(--font-mono);
		color: var(--color-primary);
	}
	.theme-note {
		color: var(--color-foreground-subtle);
		line-height: 1.4;
	}

	/* ---- token swatches ---- */
	.swatches {
		display: grid;
		grid-template-columns: repeat(auto-fill, minmax(320px, 1fr));
		gap: 8px;
	}
	.swatch {
		display: flex;
		gap: 10px;
		align-items: flex-start;
		padding: 8px;
		border: 1px solid var(--color-border-subtle);
		border-radius: 6px;
		background: var(--color-surface);
	}
	.chip-wrap {
		/* A checkerboard behind the chip so alpha is visible rather than guessed. */
		background-image:
			linear-gradient(45deg, rgba(128, 128, 128, 0.35) 25%, transparent 25%),
			linear-gradient(-45deg, rgba(128, 128, 128, 0.35) 25%, transparent 25%),
			linear-gradient(45deg, transparent 75%, rgba(128, 128, 128, 0.35) 75%),
			linear-gradient(-45deg, transparent 75%, rgba(128, 128, 128, 0.35) 75%);
		background-size: 8px 8px;
		background-position: 0 0, 0 4px, 4px -4px, -4px 0;
		border: 1px solid var(--color-border);
		border-radius: 4px;
		flex-shrink: 0;
	}
	.chip {
		display: block;
		width: 44px;
		height: 44px;
		border-radius: 3px;
	}
	.swatch-text {
		display: flex;
		flex-direction: column;
		gap: 1px;
		min-width: 0;
	}
	.swatch-text code {
		font-size: 11px;
		word-break: break-all;
	}
	.tok {
		color: var(--color-foreground);
	}
	.raw {
		color: var(--color-foreground-muted);
	}
	.bridge,
	.resolved {
		color: var(--color-foreground-subtle);
	}

	/* ---- tables ---- */
	.sheet {
		width: 100%;
		border-collapse: collapse;
		font-size: 12px;
		margin-bottom: 8px;
	}
	.sheet th,
	.sheet td {
		text-align: left;
		padding: 5px 8px;
		border-bottom: 1px solid var(--color-border-subtle);
		vertical-align: top;
	}
	.sheet thead th {
		font-family: var(--font-mono);
		font-size: 11px;
		font-weight: 400;
		color: var(--color-foreground-subtle);
		border-bottom: 1px solid var(--color-border);
		white-space: nowrap;
	}
	.mono {
		font-family: var(--font-mono);
		font-size: 11px;
	}
	.dim {
		color: var(--color-foreground-subtle);
	}
	.val {
		word-break: break-all;
	}
	.where {
		color: var(--color-foreground-muted);
		max-width: 62ch;
	}
	.ok {
		color: var(--color-success);
	}
	.bad-cell {
		color: var(--color-error);
		font-weight: 500;
	}
	.row-fail td {
		background: var(--color-error-subtle);
	}
	.inline-chip {
		display: inline-block;
		width: 9px;
		height: 9px;
		border-radius: 2px;
		border: 1px solid var(--color-border);
		margin-right: 3px;
		vertical-align: -1px;
	}

	.verdict {
		display: inline-block;
		padding: 6px 12px;
		border-radius: 6px;
		font-size: 13px;
		background: var(--color-success-subtle);
		color: var(--color-success);
		margin-bottom: 8px;
	}
	.verdict.bad {
		background: var(--color-error-subtle);
		color: var(--color-error);
	}

	.fail-list {
		display: flex;
		flex-direction: column;
		gap: 18px;
	}
	.fail-head {
		display: inline-flex;
		align-items: center;
		gap: 8px;
		background: none;
		border: none;
		padding: 0 0 4px;
		cursor: pointer;
		font-family: var(--font-serif-ui);
		font-size: 15px;
		color: var(--color-foreground);
	}
	.fail-count {
		font-family: var(--font-mono);
		font-size: 10px;
		padding: 1px 6px;
		border-radius: 999px;
		background: var(--color-error-subtle);
		color: var(--color-error);
	}

	.matrix-wrap {
		overflow-x: auto;
		border: 1px solid var(--color-border);
		border-radius: 6px;
	}
	.matrix {
		margin: 0;
		min-width: max-content;
	}
	.matrix thead th {
		writing-mode: vertical-rl;
		transform: rotate(180deg);
		height: 150px;
		padding: 6px 2px;
		font-size: 10px;
	}
	.matrix .sticky-col {
		position: sticky;
		left: 0;
		background: var(--color-surface);
		writing-mode: horizontal-tb;
		transform: none;
		height: auto;
		z-index: 1;
		border-right: 1px solid var(--color-border);
	}
	.matrix .sticky-col button {
		background: none;
		border: none;
		padding: 0;
		font: inherit;
		font-size: 11px;
		color: var(--color-foreground);
		cursor: pointer;
		white-space: nowrap;
	}
	.matrix .sticky-col.is-active button {
		color: var(--color-primary);
	}
	.cell {
		text-align: right;
		white-space: nowrap;
		color: var(--color-foreground-muted);
	}
	.cell-fail {
		background: var(--color-error-subtle);
		color: var(--color-error);
		font-weight: 500;
	}
	.cell-exempt {
		font-style: normal;
		color: var(--color-foreground-subtle);
	}
	.cell-unset {
		color: var(--color-foreground-disabled);
	}

	/* ---- type ---- */
	.faces {
		display: grid;
		grid-template-columns: repeat(auto-fill, minmax(280px, 1fr));
		gap: 8px;
		margin-bottom: 18px;
	}
	.face {
		display: flex;
		flex-direction: column;
		gap: 2px;
		padding: 8px 10px;
		border: 1px solid var(--color-border-subtle);
		border-radius: 6px;
		background: var(--color-surface);
	}
	.face-name {
		font-family: var(--font-serif-ui);
		font-size: 16px;
	}
	.face-use {
		font-size: 11px;
		color: var(--color-foreground-subtle);
	}
	.type-table td {
		vertical-align: middle;
	}
	.specimen {
		display: block;
		line-height: 1.25;
		color: var(--color-foreground);
	}
	.prohibitions {
		margin: 0 0 8px;
		padding-left: 18px;
		max-width: 76ch;
		font-size: 13px;
		color: var(--color-foreground-muted);
	}
	.prohibitions li {
		margin-bottom: 4px;
	}
	.serif-demo {
		display: flex;
		gap: 28px;
		flex-wrap: wrap;
		align-items: baseline;
		padding: 14px;
		border: 1px solid var(--color-border);
		border-radius: 6px;
		background: var(--color-surface);
	}

	/* ---- specimens ---- */
	.spec {
		display: flex;
		flex-direction: column;
		gap: 10px;
		margin-bottom: 8px;
	}
	.spec.boxed {
		padding: 16px;
		border: 1px solid var(--color-border);
		border-radius: 6px;
		background: var(--color-surface);
	}
	.spec-row {
		display: flex;
		align-items: flex-start;
		gap: 12px;
	}
	.spec-label {
		display: block;
		/* `flex: none` by default: in a COLUMN flex container a flex-basis is a
		   HEIGHT, and a 200px basis turned every label in a `.spec.boxed` into a
		   200px-tall blank. The 200px gutter is wanted only inside `.spec-row`,
		   which is a row. */
		flex: none;
		font-family: var(--font-mono);
		font-size: 10px;
		color: var(--color-foreground-subtle);
		margin-bottom: 4px;
	}
	.spec-row > .spec-label {
		flex: 0 0 200px;
		padding-top: 6px;
	}
	/* Two primitives below cannot be made to fit: Badge is `white-space: nowrap`
	   and Field's value column is `shrink-0`, so a long unbroken value pushes its
	   container — and, unclipped, the whole page — wider. The specimens keep the
	   overflow (that is the point) but scroll it here instead of widening the
	   gallery. */
	.clip {
		overflow-x: auto;
	}
	.spec-items {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: 8px;
		min-width: 0;
	}
	.spec.cards,
	.spec.inputs {
		display: grid;
		grid-template-columns: repeat(auto-fill, minmax(300px, 1fr));
		gap: 14px;
	}
	.spec.two-up {
		display: grid;
		grid-template-columns: repeat(auto-fit, minmax(340px, 1fr));
		gap: 16px;
	}
	.spec.three-up {
		display: grid;
		grid-template-columns: repeat(auto-fill, minmax(280px, 1fr));
		gap: 14px;
	}
	.card-body {
		margin: 0;
		font-size: 13px;
		color: var(--color-foreground-muted);
	}

	/* IconButton: a dense chrome row, to show the sizes at the spacing they
	   actually ship at rather than on the gallery's 8px flex gap. */
	.ib-row {
		display: inline-flex;
		align-items: center;
		gap: 2px;
		padding: 4px 6px;
		border: 1px solid var(--color-border);
		border-radius: 6px;
	}
	.ib-rowlabel {
		padding: 0 6px;
		font-size: 13px;
		color: var(--color-foreground-muted);
	}

	/* TextAction: two real paragraphs, because `inline` is defined by what it
	   inherits — a specimen on its own gives no evidence about the thing the
	   prop exists for. */
	.ta-prose-sans,
	.ta-prose-serif {
		max-width: 46ch;
		margin: 0;
	}
	.ta-prose-sans {
		font-family: var(--font-sans);
		font-size: 15px;
		line-height: 1.6;
		color: var(--color-foreground);
	}
	.ta-prose-serif {
		font-family: var(--font-serif);
		font-size: 18px;
		line-height: 1.55;
		color: var(--color-foreground);
	}
	.page-box {
		height: 320px;
		border: 1px solid var(--color-border);
		border-radius: 6px;
		overflow: hidden;
		background: var(--color-surface);
	}
	.icons {
		gap: 16px;
	}
	.icon-cell {
		display: inline-flex;
		flex-direction: column;
		align-items: center;
		gap: 3px;
		color: var(--color-foreground-muted);
	}
	.icon-cell em {
		font-style: normal;
		font-family: var(--font-mono);
		font-size: 10px;
		color: var(--color-foreground-subtle);
	}

	.foot {
		margin-top: 64px;
		padding-top: 16px;
		border-top: 1px solid var(--color-border);
		font-size: 12px;
		color: var(--color-foreground-subtle);
	}
</style>
