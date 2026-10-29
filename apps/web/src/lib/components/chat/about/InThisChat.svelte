<!--
	The body of a chat's About page: what the conversation named, what it
	cited, what it made, and what left your server for it, read from the
	transcript alone (inThisChat.ts).

	The groups are the sidebar's (design.md, The shell): a small label and a
	hand-drawn rule, folding from the label, the fold remembered. No group
	shows a count, and a long one ends in "Show all", never a number, because
	someone in recovery reads the same page. "Left your server" is always
	drawn, so the owner can see what did and did not go out.

	The page usually sits beside its chat, and the two light each other:
	pointing at a row tints the places in the chat that name it, pointing at a
	link in the chat tints its row, and "Show in the chat" scrolls the chat to
	the message (ChatView listens for REVEAL_MESSAGE).
-->
<script lang="ts" module>
	/** Asks the chat view showing `chatId` to scroll to a message. A view that
	 *  did calls `preventDefault()`. */
	export const REVEAL_MESSAGE = "virtues:reveal-message";
	export type RevealMessageDetail = { chatId: string; messageId: string };
</script>

<script lang="ts">
	import { onMount, tick, type Snippet } from "svelte";
	import { slide } from "svelte/transition";
	import { flip } from "svelte/animate";
	import { prefersReducedMotion } from "svelte/motion";
	import Icon from "$lib/components/Icon.svelte";
	import PersonPicture from "$lib/components/wiki/PersonPicture.svelte";
	import { inThisChat, keyForHref, type AsideItem } from "./inThisChat";
	import { windowShellStore } from "$lib/stores/window-shell.svelte";
	import { getInitializationPromise, getModelById } from "$lib/stores/models.svelte";
	import { openWebLink } from "$lib/utils/webLinks";
	import { easePremium, motionMs } from "$lib/utils/motion";
	import { ruleStroke } from "$lib/sidebar/groupRule";

	let {
		chatId,
		messages,
		under,
	}: {
		chatId: string;
		messages: readonly unknown[];
		/** The last group, folded until opened: how the chat ran. */
		under?: Snippet;
	} = $props();

	// Rows a group shows before "Show all".
	const PREVIEW = 8;
	const FOLDS_KEY = "virtues.aboutChat.folds";
	// Groups that start folded until the owner opens them.
	const FOLDED_AT_FIRST = new Set(["also", "under"]);

	const panel = $derived(inThisChat(messages));

	// Who and what the chat is about, from either side of it: what you
	// attached or @-mentioned, then what the replies named.
	const about = $derived.by(() => {
		const seen = new Set<string>();
		return [...panel.broughtIn, ...panel.mentioned].filter((r) => !seen.has(r.key) && seen.add(r.key));
	});

	const nothingYet = $derived(!about.length && !panel.sources.length && !panel.alsoRead.length && !panel.made.length);

	// ── The models that wrote the answers ────────────────────────────────────

	let modelsReady = $state(false);
	onMount(() => void getInitializationPromise().then(() => (modelsReady = true)));

	function modelName(id: string): string {
		void modelsReady;
		return getModelById(id)?.displayName ?? id.split("/").pop() ?? id;
	}

	const writers = $derived(
		panel.writers.map((w) => {
			const id = w.label;
			const provider = w.detail ?? getModelById(id)?.provider;
			return { ...w, label: modelName(id), detail: provider ? providerName(provider) : undefined };
		}),
	);

	function providerName(p: string): string {
		return p.charAt(0).toUpperCase() + p.slice(1);
	}

	const providers = $derived([...new Set(writers.map((w) => w.detail).filter((p): p is string => !!p))]);

	function joinNames(names: string[]): string {
		if (names.length <= 1) return names[0] ?? "";
		return `${names.slice(0, -1).join(", ")} and ${names.at(-1)}`;
	}

	// ── Folds, remembered across chats like the sidebar's ──────────────────

	function readFolds(): Record<string, boolean> {
		try {
			const parsed = JSON.parse(localStorage.getItem(FOLDS_KEY) ?? "{}");
			return parsed && typeof parsed === "object" ? parsed : {};
		} catch {
			return {};
		}
	}

	let folds = $state<Record<string, boolean>>(readFolds());
	let expanded = $state<Record<string, boolean>>({});

	const folded = (id: string) => folds[id] ?? FOLDED_AT_FIRST.has(id);

	function toggleFold(id: string) {
		folds[id] = !folded(id);
		try {
			localStorage.setItem(FOLDS_KEY, JSON.stringify(folds));
		} catch {
			// Private windows refuse storage; the fold still holds for this visit.
		}
	}

	// The fold's duration scales with the group's height, clamped, as the
	// sidebar's does, so a long group does not snap and a short one does not drag.
	const foldStyle = (rows: number) => `--fold-ms: ${Math.min(280, Math.max(180, 140 + rows * 8))}ms`;

	const shown = (id: string, rows: AsideItem[]) => (expanded[id] ? rows : rows.slice(0, PREVIEW));

	// ── Rows arriving ────────────────────────────────────────────────────────

	// The rows already there when the page opens arrive with it, not one by
	// one. After that a new row slides in, and wears a tint that settles.
	let settled = $state(false);
	const seen = new Set<string>();
	let fresh = $state<Record<string, boolean>>({});

	onMount(() => {
		const t = setTimeout(() => (settled = true), 400);
		return () => clearTimeout(t);
	});

	$effect(() => {
		const all = [...about, ...panel.sources, ...panel.alsoRead, ...panel.made, ...panel.writers, ...panel.leftTheBox];
		for (const item of all) {
			if (seen.has(item.key)) continue;
			seen.add(item.key);
			if (settled) {
				fresh[item.key] = true;
				setTimeout(() => delete fresh[item.key], 1600);
			}
		}
	});

	const moveMs = () => (prefersReducedMotion.current ? 0 : motionMs("--motion-layout", 300));
	const rowIn = (node: Element) => (settled ? slide(node, { duration: moveMs(), easing: easePremium }) : { duration: 0 });
	const rowFlip = (node: Element, rects: { from: DOMRect; to: DOMRect }) =>
		flip(node, rects, { duration: settled ? moveMs() : 0, easing: easePremium });

	// ── Opening and showing where ────────────────────────────────────────────

	// Which appearance "Show in the chat" goes to next, per row.
	const step: Record<string, number> = {};

	/** Scroll the chat to a message. When no pane is showing the chat, open it
	 *  beside this page first. */
	async function reveal(messageId: string) {
		const ask = () => {
			const event = new CustomEvent<RevealMessageDetail>(REVEAL_MESSAGE, { cancelable: true, detail: { chatId, messageId } });
			window.dispatchEvent(event);
			return event.defaultPrevented;
		};
		if (ask()) return;
		windowShellStore.openRouteBeside(`/chat/${chatId}`);
		await tick();
		// The chat loads its transcript before it can scroll to a message.
		for (let i = 0; i < 10 && !ask(); i++) await new Promise((r) => setTimeout(r, 150));
	}

	function showWhere(item: AsideItem) {
		if (!item.messageIds.length) return;
		const i = (step[item.key] ?? 0) % item.messageIds.length;
		step[item.key] = i + 1;
		void reveal(item.messageIds[i]);
	}

	// A row opens what it names in a new tab beside this page, so the chat in
	// the other pane stays where it is; a web page opens outside the app. A row
	// with nothing to open (a chart, a search) shows where it is in the chat.
	function open(item: AsideItem) {
		if (item.route) windowShellStore.openInNewTab(item.route, item.label);
		else if (item.url && /^https?:\/\//.test(item.url)) void openWebLink(item.url);
		else if (item.url) window.open(item.url, "_blank", "noopener");
		else showWhere(item);
	}

	const isPerson = (item: AsideItem) => !!item.route?.startsWith("/person/");
	const isPage = (item: AsideItem) => !!item.route?.startsWith("/page/");
	const canShowWhere = (item: AsideItem) => item.messageIds.length > 0 && !!(item.route || item.url);

	// ── The chat and this page lighting each other ───────────────────────────

	const LINK = "[data-ref-url], a[href]";
	// The row whose link the pointer is on in the chat.
	let echoKey = $state<string | null>(null);
	let lit: Element[] = [];

	/** Links in the views on screen, other than this page. */
	function linksOnScreen(): Element[] {
		return [...document.querySelectorAll(`.tab-content.active ${LINK}`)].filter((el) => !el.closest(".about-chat"));
	}

	function hrefOf(el: Element): string | null {
		return el.getAttribute("data-ref-url") ?? el.getAttribute("href");
	}

	function lightLinks(item: AsideItem) {
		dimLinks();
		lit = linksOnScreen().filter((el) => {
			const href = hrefOf(el);
			return !!href && keyForHref(href) === item.key;
		});
		for (const el of lit) el.classList.add("ref-echo");
	}

	function dimLinks() {
		for (const el of lit) el.classList.remove("ref-echo");
		lit = [];
	}

	onMount(() => {
		const over = (e: Event) => {
			const el = (e.target as Element | null)?.closest?.(LINK);
			if (!el || el.closest(".about-chat") || !el.closest(".tab-content.active")) return;
			const href = hrefOf(el);
			echoKey = href ? keyForHref(href) : null;
		};
		const out = (e: Event) => {
			const el = (e.target as Element | null)?.closest?.(LINK);
			if (el && !el.contains((e as MouseEvent).relatedTarget as Node | null)) echoKey = null;
		};
		document.addEventListener("mouseover", over);
		document.addEventListener("mouseout", out);
		return () => {
			document.removeEventListener("mouseover", over);
			document.removeEventListener("mouseout", out);
			dimLinks();
		};
	});
</script>

{#snippet row(item: AsideItem)}
	<!-- The pointer lights the chat's links from anywhere on the row; the
	     keyboard does the same from the row's button. -->
	<!-- svelte-ignore a11y_no_static_element_interactions -->
	<div
		class="row"
		class:echo={echoKey === item.key}
		class:fresh={fresh[item.key]}
		onmouseenter={() => lightLinks(item)}
		onmouseleave={dimLinks}
	>
		<button type="button" class="open" onclick={() => open(item)} onfocus={() => lightLinks(item)} onblur={dimLinks} data-tip={item.label.length > 48 ? item.label : undefined}>
			<span class="lead">
				{#if isPerson(item)}
					<PersonPicture name={item.label} size={20} />
				{:else}
					<Icon icon={item.icon} width="16" />
				{/if}
			</span>
			<span class="label" class:serif={isPage(item)}>{item.label}</span>
			{#if item.detail}<span class="meta" class:yields={canShowWhere(item)}>{item.detail}</span>{/if}
		</button>
		{#if canShowWhere(item)}
			<button type="button" class="where" onclick={() => showWhere(item)} aria-label="Show where {item.label} appears in the chat" data-tip="Show in the chat">
				<Icon icon="mynaui:target" width="14" />
			</button>
		{/if}
	</div>
{/snippet}

{#snippet list(rows: AsideItem[])}
	<ul>
		{#each rows as item (item.key)}
			<li in:rowIn animate:rowFlip>
				{@render row(item)}
			</li>
		{/each}
	</ul>
{/snippet}

{#snippet groupHead(id: string, label: string, rows: number)}
	<div class="group-head" class:folded={folded(id)} style={foldStyle(rows)}>
		<button
			type="button"
			class="group-label"
			aria-expanded={!folded(id)}
			data-tip={folded(id) ? `Show ${label}` : `Hide ${label}`}
			onclick={() => toggleFold(id)}
		>
			<span>{label}</span>
			<svg class="chev" width="9" height="6" viewBox="0 0 10 6" fill="none" aria-hidden="true">
				<path d="M1 1l4 4 4-4" stroke="currentColor" stroke-width="1.3" stroke-linecap="round" stroke-linejoin="round" />
			</svg>
		</button>
		<svg class="group-rule" viewBox="0 0 100 4" preserveAspectRatio="none" aria-hidden="true">
			<path d={ruleStroke(id)} vector-effect="non-scaling-stroke" />
		</svg>
	</div>
{/snippet}

{#snippet group(id: string, label: string, rows: AsideItem[], extra?: Snippet)}
	{#if rows.length || extra}
		<section>
			{@render groupHead(id, label, Math.min(rows.length, PREVIEW))}
			<div class="sidebar-expandable fold" class:expanded={!folded(id)} style={foldStyle(Math.min(rows.length, PREVIEW))}>
				<div class="sidebar-expandable-inner">
					{#if rows.length}{@render list(shown(id, rows))}{/if}
					{#if rows.length > PREVIEW && !expanded[id]}
						<button type="button" class="more" onclick={() => (expanded[id] = true)}>Show all</button>
					{/if}
					{@render extra?.()}
				</div>
			</div>
		</section>
	{/if}
{/snippet}

<!-- What a reply's tools read but nothing in the chat names, folded under Sources. -->
{#snippet alsoRead()}
	<button type="button" class="more also" class:open={!folded("also")} aria-expanded={!folded("also")} onclick={() => toggleFold("also")}>
		<svg class="chev" width="9" height="6" viewBox="0 0 10 6" fill="none" aria-hidden="true">
			<path d="M1 1l4 4 4-4" stroke="currentColor" stroke-width="1.3" stroke-linecap="round" stroke-linejoin="round" />
		</svg>
		Also read, not cited
	</button>
	<div class="sidebar-expandable fold" class:expanded={!folded("also")} style={foldStyle(Math.min(panel.alsoRead.length, PREVIEW))}>
		<div class="sidebar-expandable-inner quiet">
			{@render list(shown("also", panel.alsoRead))}
			{#if panel.alsoRead.length > PREVIEW && !expanded.also}
				<button type="button" class="more" onclick={() => (expanded.also = true)}>Show all</button>
			{/if}
		</div>
	</div>
{/snippet}

<!-- The model rows first, since every answer went to one, then any tool
     that reached the web. The sentence says what went where. -->
{#snippet leftYourServer()}
	{#if writers.length}{@render list(writers)}{/if}
	{#if panel.leftTheBox.length}{@render list(shown("left", panel.leftTheBox))}{/if}
	{#if panel.leftTheBox.length > PREVIEW && !expanded.left}
		<button type="button" class="more" onclick={() => (expanded.left = true)}>Show all</button>
	{/if}
	<p class="note">
		{#if providers.length}
			Each answer sent your question, and the records it read, to {joinNames(providers)}.
		{/if}
		{#if !panel.leftTheBox.length}
			No web search, browsing or image model ran in this chat.
		{/if}
	</p>
{/snippet}

<div class="about-chat">
	{#if nothingYet}
		<p class="lede">The people, places and sources this chat names, and anything made here, gather here as the chat goes on.</p>
	{/if}

	{@render group("about", "Mentioned", about)}
	{@render group("sources", "Sources", panel.sources, panel.alsoRead.length ? alsoRead : undefined)}
	{@render group("made", "Made here", panel.made)}
	{@render group("left", "Left your server", [], leftYourServer)}
	{#if under}{@render group("under", "How it ran", [], under)}{/if}
</div>

<style>
	ul {
		margin: 0;
		padding: 0;
		list-style: none;
	}

	.lede {
		margin: 0 12px 8px;
		font-family: var(--font-serif);
		font-size: 17px;
		line-height: 1.5;
		color: var(--color-foreground-muted);
	}

	/* ── Group heads (the sidebar's, design.md) ─────────────────────────── */

	section:first-of-type .group-head {
		margin-top: 0;
	}

	.group-head {
		display: flex;
		align-items: center;
		height: 22px;
		margin-top: 24px;
		margin-bottom: 4px;
		padding-right: 4px;
		user-select: none;
	}

	.group-label {
		display: flex;
		align-items: center;
		gap: 6px;
		height: 100%;
		padding: 0 0 0 12px;
		border: none;
		border-radius: var(--radius-md);
		background: none;
		cursor: pointer;
		font-size: 12px;
		color: var(--color-foreground-subtle);
		transition: color var(--motion-hover-out) var(--ease-premium);
	}

	.group-label:hover {
		color: var(--color-foreground-muted);
		transition-duration: var(--motion-hover-in);
	}

	.group-head,
	.fold,
	.also {
		--fold-open: var(--fold-ms, 200ms);
		--fold-close: calc(var(--fold-ms, 200ms) * 0.85);
		--fold-ease: cubic-bezier(0.3, 0.1, 0.2, 1);
	}

	/* At rest the chevron takes no room; pointing at the head opens its slot
	   and the rule shortens to make way. Folded, it stays, pointing at the
	   group it would reopen. */
	.group-head .chev {
		width: 0;
		margin-left: -6px;
		opacity: 0;
		transition:
			width 180ms var(--fold-ease),
			margin-left 180ms var(--fold-ease),
			opacity 150ms ease,
			transform var(--fold-open) var(--fold-ease);
	}

	.group-head.folded .chev {
		transition:
			width 180ms var(--fold-ease),
			margin-left 180ms var(--fold-ease),
			opacity 150ms ease,
			transform var(--fold-close) var(--fold-ease);
	}

	.group-head:hover .chev,
	.group-label:focus-visible .chev,
	.group-head.folded .chev {
		width: 9px;
		margin-left: 0;
		opacity: 0.8;
	}

	.group-head.folded .chev {
		transform: rotate(-90deg);
	}

	.group-rule {
		flex: 1;
		min-width: 0;
		height: 4px;
		margin: 0 8px;
		fill: none;
		stroke: var(--color-foreground-subtle);
		stroke-width: 1;
		stroke-linecap: round;
		opacity: 0.45;
		pointer-events: none;
	}

	/* A fold, not a blind: the rows ride the clip's edge and tuck under their
	   head while fading, as the sidebar's groups do. */
	.fold {
		transition: grid-template-rows var(--fold-close) var(--fold-ease);
	}

	.fold.expanded {
		transition-duration: var(--fold-open);
	}

	.fold > :global(.sidebar-expandable-inner) {
		justify-content: flex-end;
		opacity: 0;
		transition: opacity calc(var(--fold-close) * 0.4) ease-out;
	}

	.fold.expanded > :global(.sidebar-expandable-inner) {
		opacity: 1;
		transition: opacity calc(var(--fold-open) * 0.6) ease-in calc(var(--fold-open) * 0.15);
	}

	.fold > :global(.sidebar-expandable-inner) > :global(*) {
		flex-shrink: 0;
	}

	/* ── Rows ───────────────────────────────────────────────────────────── */

	.row {
		position: relative;
		display: flex;
		align-items: center;
		border-radius: var(--radius-md);
		transition: background-color var(--motion-hover-out) var(--ease-premium);
	}

	.row:hover,
	.row.echo {
		background: var(--hover-bg);
		transition-duration: var(--motion-hover-in);
	}

	.row:has(.open:active) {
		background: var(--press-bg);
		transition-duration: var(--motion-press);
	}

	/* A row that came in while the chat was replying: a tint that settles. */
	.row.fresh {
		animation: arrive 1600ms var(--ease-premium);
	}

	@keyframes arrive {
		from {
			background-color: color-mix(in srgb, var(--color-primary) 12%, transparent);
		}
		to {
			background-color: transparent;
		}
	}

	.open {
		display: flex;
		align-items: center;
		gap: var(--sidebar-interactive-gap, 10px);
		flex: 1;
		min-width: 0;
		height: var(--sidebar-interactive-height, 32px);
		padding: 0 12px;
		border: none;
		border-radius: var(--radius-md);
		background: none;
		text-align: left;
		font-size: var(--chrome-label-font-size, 14px);
		color: var(--color-foreground);
		cursor: pointer;
	}

	.lead {
		display: flex;
		align-items: center;
		justify-content: center;
		flex: none;
		width: 20px;
		margin: 0 -2px;
		color: var(--color-foreground-muted);
		opacity: 0.8;
		transition:
			color var(--motion-hover-out) var(--ease-premium),
			opacity var(--motion-hover-out) var(--ease-premium);
	}

	.row:hover .lead,
	.row.echo .lead {
		color: var(--color-foreground);
		opacity: 1;
		transition-duration: var(--motion-hover-in);
	}

	.label {
		flex: 1;
		min-width: 0;
		overflow: hidden;
		white-space: nowrap;
		text-overflow: ellipsis;
	}

	/* A page is writing, so it is set in the serif, a step that matches the
	   sans by eye (design.md, Type). */
	.label.serif {
		font-family: var(--font-serif-ui);
		font-size: 15px;
	}

	.meta {
		flex: none;
		max-width: 40%;
		overflow: hidden;
		white-space: nowrap;
		text-overflow: ellipsis;
		font-size: 12px;
		color: var(--color-foreground-subtle);
		transition: opacity var(--motion-hover-out) var(--ease-premium);
	}

	/* Pointing at a row that can show where it is, the kind gives way to
	   the button that does it. */
	.row:hover .meta.yields,
	.row:focus-within .meta.yields {
		opacity: 0;
		transition-duration: var(--motion-hover-in);
	}

	.where {
		position: absolute;
		right: 2px;
		display: flex;
		align-items: center;
		justify-content: center;
		width: 28px;
		height: 28px;
		border: none;
		border-radius: var(--radius-md);
		background: none;
		color: var(--color-foreground-subtle);
		opacity: 0;
		cursor: pointer;
		transition:
			opacity var(--motion-hover-out) var(--ease-premium),
			color var(--motion-hover-out) var(--ease-premium),
			background-color var(--motion-hover-out) var(--ease-premium),
			transform var(--motion-release) var(--ease-premium);
	}

	.row:hover .where,
	.where:focus-visible {
		opacity: 1;
		transition-duration: var(--motion-hover-in);
	}

	.where:hover {
		color: var(--color-foreground);
		background: var(--hover-bg);
	}

	.where:active {
		transform: scale(0.94);
		transition-duration: var(--motion-press);
	}

	/* What was only read sits a step quieter than what was cited. */
	.quiet .label {
		color: var(--color-foreground-muted);
	}

	/* ── Show all, Also read, notes ─────────────────────────────────────── */

	.more {
		display: flex;
		align-items: center;
		gap: 6px;
		height: 28px;
		padding: 0 12px;
		border: none;
		border-radius: var(--radius-md);
		background: none;
		font-size: 12px;
		color: var(--color-foreground-subtle);
		cursor: pointer;
		transition: color var(--motion-hover-out) var(--ease-premium);
	}

	.more:hover {
		color: var(--color-foreground);
		transition-duration: var(--motion-hover-in);
	}

	.also {
		margin-top: 4px;
	}

	.also .chev {
		transform: rotate(-90deg);
		transition: transform var(--fold-close) var(--fold-ease);
	}

	.also.open .chev {
		transform: rotate(0deg);
		transition-duration: var(--fold-open);
	}

	.note {
		margin: 8px 12px 0;
		font-size: 13px;
		line-height: 1.5;
		color: var(--color-foreground-muted);
	}

	button:focus-visible {
		outline: 2px solid var(--color-primary);
		outline-offset: -2px;
	}

	@media (hover: none) {
		.where {
			opacity: 1;
		}

		.meta.yields {
			opacity: 0;
		}
	}
</style>
