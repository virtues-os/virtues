<script lang="ts">
	import { onMount } from "svelte";
	import Icon from "$lib/components/Icon.svelte";
	import ChatInput from "$lib/components/ChatInput.svelte";
	import { askVirtues } from "$lib/stores/pendingPrompt.svelte";
	import { windowShellStore } from "$lib/stores/window-shell.svelte";
	import { chatSessions } from "$lib/stores/chatSessions.svelte";
	import { pagesStore } from "$lib/stores/pages.svelte";
	import {
		getReflectionsForDate,
		createReflection,
		type Page,
	} from "$lib/api/client";

	// HomeViewSpread — the "open notebook" prototype (B+C). Home is a two-page
	// spread: the VERSO (left) is the standing self — day number, your Likeness,
	// what changed, a note from a year ago; it barely moves. The RECTO (right) is
	// the living day — one salient hero the salience engine picked, with the rest
	// listed below as a ledger you can promote from. A composer is docked across
	// the foot: the one place you act.
	//
	// Everything below the Likeness is HARDCODED until the salience / novelty /
	// praxis endpoints exist. The `preview` toggle (top-right, dev-only) swaps
	// between a populated day, a quiet day, and a brand-new box so we can judge
	// the empty states without wiping data.

	let input = $state("");
	let narrative = $state<string | undefined>(undefined);

	// --- Preview harness (dev-only; remove with the prototype) --------------
	type Demo = "full" | "quiet" | "new";
	let demo = $state<Demo>("full");
	const demoOptions: { id: Demo; label: string }[] = [
		{ id: "full", label: "Populated" },
		{ id: "quiet", label: "Quiet day" },
		{ id: "new", label: "First day" },
	];

	const RECORD_START = new Date(2025, 11, 5); // 2025-12-05 (placeholder)
	const now = new Date();
	const realDay = Math.max(
		1,
		Math.floor((+now - +RECORD_START) / 86_400_000) + 1,
	);
	const longDate = now.toLocaleDateString(undefined, {
		weekday: "long",
		month: "long",
		day: "numeric",
	});

	// --- HARDCODED placeholders ---
	const stateLine = "Slept 6h 20m · Clear, 54°";
	const likenessChanged = "edited 3 days ago · you added a line about patience";
	const aYearAgo = "A year ago today, you first wrote about leaving the job.";
	const SAMPLE_LIKENESS =
		"I mistake motion for progress, and I'm learning to sit with a decision before I act on it.";
	// ------------------------------

	const dayNumber = $derived(demo === "new" ? 1 : realDay);
	const almanac = $derived(demo === "new" ? "No sources connected yet" : stateLine);

	const narrativeGlimpse = $derived.by(() => {
		const text = narrative?.trim();
		if (!text) return undefined;
		const first = text.match(/^.*?[.!?](?:\s|$)/)?.[0]?.trim();
		const g = first && first.length <= 220 ? first : text;
		return g.length > 200 ? g.slice(0, 198).trimEnd() + "…" : g;
	});
	// The verso's Likeness. A brand-new box has none; otherwise fall back to a
	// sample so the design reads even when the endpoint is empty in dev.
	const likenessText = $derived(
		demo === "new" ? undefined : (narrativeGlimpse ?? SAMPLE_LIKENESS),
	);
	const showDiff = $derived(demo !== "new" && !!likenessText);

	// Recto candidates — the living day only. Populated day has salience; a quiet
	// day surfaces nothing; a new box hasn't learned anything yet.
	type Card = {
		kicker: string;
		hero: string;
		label: string;
		value: string;
		route?: string;
		routeLabel?: string;
	};
	const cards = $derived.by<Card[]>(() => {
		if (demo !== "full") return [];
		const list: Card[] = [
			{
				kicker: "Most novel today",
				hero: "Lunch with Sarah — first time in three months.",
				label: "Most novel",
				value: "Lunch with Sarah",
				route: "/day",
				routeLabel: "Today",
			},
			{
				kicker: "Still open",
				hero: "Three things you said you'd do, and haven't yet.",
				label: "Still open",
				value: "3 unkept",
			},
			{
				kicker: "An undercurrent",
				hero: "You've written the word “tired” four times this week.",
				label: "Undercurrent",
				value: "“tired” · 4×",
			},
		];
		if (!hasEntryToday)
			list.push({
				kicker: "Unwritten",
				hero: "Today has no entry yet — two minutes is enough.",
				label: "Unwritten",
				value: "no entry",
			});
		return list;
	});
	let heroIndex = $state(0);
	const heroCard = $derived(
		cards.length ? cards[heroIndex % cards.length] : undefined,
	);
	const restCards = $derived(
		cards
			.map((c, i) => ({ c, i }))
			.filter(({ i }) => cards.length && i !== heroIndex % cards.length),
	);
	function promote(i: number) {
		heroIndex = i;
	}

	type Recent = { route: string; title: string; ts: number };
	const recents = $derived.by<Recent[]>(() => {
		if (demo === "new") return [];
		const chats: Recent[] = chatSessions.sessions.map((c) => ({
			route: `/chat/${c.conversation_id}`,
			title: c.title || "Untitled",
			ts: c.last_updated ? Date.parse(c.last_updated) : 0,
		}));
		const pages: Recent[] = pagesStore.pages.map((p) => ({
			route: `/page/${p.id}`,
			title: p.title || "Untitled",
			ts: p.updated_at ? Date.parse(p.updated_at) : 0,
		}));
		return [...chats, ...pages].sort((a, b) => b.ts - a.ts).slice(0, 3);
	});

	function open(route: string, title: string) {
		windowShellStore.openTabFromRoute(route, { label: title });
	}

	const todayDate = (() => {
		const y = now.getFullYear();
		const m = String(now.getMonth() + 1).padStart(2, "0");
		const day = String(now.getDate()).padStart(2, "0");
		return `${y}-${m}-${day}`;
	})();

	let journal = $state<Page[]>([]);
	const hasEntryToday = $derived(demo !== "new" && journal.length > 0);

	async function examineToday() {
		if (hasEntryToday) {
			open(`/page/${journal[0].id}`, journal[0].title || "Today");
			return;
		}
		try {
			const page = await createReflection(todayDate);
			journal = [...journal, page];
			open(`/page/${page.id}`, page.title || "Untitled");
		} catch (e) {
			console.error("Failed to create journal entry:", e);
		}
	}

	onMount(() => {
		fetch("/api/wiki/narrative-identity")
			.then((r) => (r.ok ? r.json() : null))
			.then((d) => {
				if (d?.content) narrative = d.content;
			})
			.catch(() => {});
		if (chatSessions.sessions.length === 0 && !chatSessions.isLoading)
			chatSessions.load();
		if (pagesStore.pages.length === 0 && !pagesStore.pagesLoading)
			pagesStore.loadPages();
		getReflectionsForDate(todayDate)
			.then((pages) => (journal = pages))
			.catch(() => {});
	});
</script>

<div class="spread-scroll">
	<!-- Preview harness (dev-only) -->
	<div class="preview-toggle" role="radiogroup" aria-label="Preview state">
		<span class="pt-label">preview</span>
		{#each demoOptions as o (o.id)}
			<button
				type="button"
				class="pt-seg"
				class:on={demo === o.id}
				role="radio"
				aria-checked={demo === o.id}
				onclick={() => {
					demo = o.id;
					heroIndex = 0;
				}}
			>
				{o.label}
			</button>
		{/each}
	</div>

	<!-- Folio line spans the whole spread -->
	<div class="top">
		<header class="folio-head">
			<span class="folio-day">Day {dayNumber}</span>
			<span class="folio-date">{longDate}</span>
			<span class="folio-almanac">{almanac}</span>
		</header>
	</div>

	<div class="spread-body">
		<div class="spread">
			<!-- VERSO — the standing self -->
			<section class="verso">
				<span class="page-eyebrow">You</span>
				{#if likenessText}
					<button
						type="button"
						class="likeness"
						onclick={() => open("/narrative-identity/present", "You")}
					>
						“{likenessText}”
					</button>
					{#if showDiff}
						<div class="likeness-foot">
							<span class="diff-dot"></span>
							<span>{likenessChanged}</span>
						</div>
					{/if}
				{:else}
					<!-- First-day empty state: an invitation, not an error -->
					<div class="likeness-empty">
						<p class="le-lead">This page becomes you.</p>
						<p class="le-body">
							Your Likeness is a few honest lines about who you
							are. Virtues drafts it from what it learns — you keep
							it true.
						</p>
						<button
							type="button"
							class="le-cta"
							onclick={() =>
								open("/narrative-identity/present", "You")}
						>
							Begin your Likeness
							<Icon icon="ri:arrow-right-line" width="15" />
						</button>
					</div>
				{/if}

				<div class="verso-foot">
					<span class="a-year-ago">
						{demo === "new" ? "The record starts here." : aYearAgo}
					</span>
				</div>
			</section>

			<div class="gutter"></div>

			<!-- RECTO — the living day -->
			<section class="recto">
				<span class="page-eyebrow">Today</span>

				{#if heroCard}
					<div class="hero">
						<span class="kicker">{heroCard.kicker}</span>
						<button
							type="button"
							class="hero-body"
							onclick={() =>
								heroCard.route
									? open(heroCard.route, heroCard.routeLabel || "")
									: undefined}
						>
							{heroCard.hero}
						</button>
					</div>

					{#if restCards.length > 0}
						<div class="ledger">
							{#each restCards as { c, i } (c.label)}
								<button class="ledger-row" onclick={() => promote(i)}>
									<span class="lbl">{c.label}</span>
									<span class="leader"></span>
									<span class="val">{c.value}</span>
								</button>
							{/each}
						</div>
					{/if}
				{:else}
					<!-- Empty recto: a new box vs a genuinely quiet day -->
					<div class="hero empty">
						{#if demo === "new"}
							<p class="hero-body-static">
								Nothing's surfaced yet.
							</p>
							<p class="hero-sub">
								Connect a source — mail, calendar, your phone —
								and your days begin to fill this page.
							</p>
						{:else}
							<p class="hero-body-static">
								Nothing stands out today.
							</p>
							<p class="hero-sub">
								A calm day. A good one to sit with.
							</p>
						{/if}
					</div>
				{/if}

				<div class="recto-actions">
					<button type="button" class="examine" onclick={examineToday}>
						<Icon icon="ri:quill-pen-line" width="15" />
						{#if hasEntryToday}
							Continue today's entry
						{:else if demo === "new"}
							Write your first entry
						{:else}
							Examine today
						{/if}
					</button>
					{#if recents.length > 0}
						<div class="continue">
							<span class="continue-head">Where you left off</span>
							{#each recents as r (r.route)}
								<button
									type="button"
									class="continue-item"
									onclick={() => open(r.route, r.title)}
								>
									{r.title}
								</button>
							{/each}
						</div>
					{/if}
				</div>
			</section>
		</div>
	</div>

	<!-- Composer docked across the foot — the one place you act -->
	<div class="composer-dock">
		<div class="composer-inner">
			<ChatInput
				bind:value={input}
				placeholder={demo === "new"
					? "Ask anything to begin…"
					: "Ask about today, or begin writing…"}
				maxWidth="max-w-none"
				on:submit={(e) => askVirtues(e.detail)}
			/>
		</div>
	</div>
</div>

<style>
	.spread-scroll {
		position: relative;
		height: 100%;
		width: 100%;
		display: flex;
		flex-direction: column;
		min-height: 0;
	}

	/* Preview harness ------------------------------------------------------- */
	.preview-toggle {
		position: absolute;
		top: 0.625rem;
		right: 0.75rem;
		z-index: 40;
		display: flex;
		align-items: center;
		gap: 0.125rem;
		padding: 0.1875rem 0.1875rem 0.1875rem 0.5rem;
		border-radius: 999px;
		border: 1px solid var(--color-border);
		background: var(--color-surface-overlay, var(--color-surface-elevated));
	}
	.pt-label {
		font-family: var(--font-mono);
		font-size: 0.625rem;
		text-transform: uppercase;
		letter-spacing: 0.08em;
		color: var(--color-foreground-disabled);
		margin-right: 0.25rem;
	}
	.pt-seg {
		padding: 0.25rem 0.5rem;
		border-radius: 999px;
		border: none;
		background: transparent;
		color: var(--color-foreground-subtle);
		font-family: var(--font-mono);
		font-size: 0.625rem;
		letter-spacing: 0.02em;
		cursor: pointer;
		transition:
			background-color var(--duration-fast) ease,
			color var(--duration-fast) ease;
	}
	.pt-seg:hover {
		color: var(--color-foreground);
	}
	.pt-seg.on {
		background: var(--color-foreground);
		color: var(--color-background);
	}

	/* Folio line ------------------------------------------------------------ */
	.top {
		flex-shrink: 0;
		display: flex;
		justify-content: center;
		padding: clamp(1.75rem, 5vh, 3rem) 2rem 0;
	}
	.folio-head {
		width: 100%;
		max-width: 62rem;
		display: flex;
		align-items: baseline;
		gap: 1rem;
		padding-bottom: 1rem;
		border-bottom: 1px solid var(--color-border);
		font-family: var(--font-mono);
		font-size: 0.75rem;
		letter-spacing: 0.02em;
		color: var(--color-foreground-subtle);
	}
	.folio-day {
		color: var(--color-foreground-muted);
		font-weight: 500;
	}
	.folio-date {
		flex: 1;
	}
	.folio-almanac {
		text-align: right;
	}

	/* Spread ---------------------------------------------------------------- */
	.spread-body {
		flex: 1;
		min-height: 0;
		overflow-y: auto;
		display: flex;
		align-items: center;
		justify-content: center;
		padding: 2rem;
	}
	.spread {
		width: 100%;
		max-width: 62rem;
		display: grid;
		grid-template-columns: 1fr 1px 1.1fr;
		gap: 3rem;
		align-items: start;
	}
	.gutter {
		align-self: stretch;
		background: var(--color-border);
	}
	.page-eyebrow {
		font-family: var(--font-mono);
		font-size: 0.6875rem;
		text-transform: uppercase;
		letter-spacing: 0.18em;
		color: var(--color-foreground-subtle);
		display: block;
		margin-bottom: 1.5rem;
	}

	/* Verso — the standing self -------------------------------------------- */
	.verso {
		display: flex;
		flex-direction: column;
		min-height: 15rem;
	}
	.likeness {
		text-align: left;
		background: transparent;
		border: none;
		padding: 0;
		cursor: pointer;
		font-family: var(--font-serif);
		font-style: italic;
		font-weight: 300;
		font-size: clamp(1.25rem, 2.3vw, 1.6rem);
		line-height: 1.42;
		letter-spacing: -0.005em;
		color: var(--color-foreground);
		transition: color var(--duration-fast) ease;
	}
	.likeness:hover {
		color: var(--color-foreground-muted);
	}
	.likeness-foot {
		display: flex;
		align-items: center;
		gap: 0.4375rem;
		margin-top: 1rem;
		font-family: var(--font-mono);
		font-size: 0.6875rem;
		color: var(--color-foreground-subtle);
	}
	.diff-dot {
		width: 6px;
		height: 6px;
		border-radius: 50%;
		background: var(--color-primary);
		flex-shrink: 0;
	}

	/* First-day Likeness empty state */
	.likeness-empty {
		display: flex;
		flex-direction: column;
		align-items: flex-start;
	}
	.le-lead {
		font-family: var(--font-serif);
		font-style: italic;
		font-weight: 300;
		font-size: clamp(1.25rem, 2.3vw, 1.6rem);
		line-height: 1.35;
		color: var(--color-foreground);
		margin: 0;
	}
	.le-body {
		font-family: var(--font-sans);
		font-size: 0.9375rem;
		line-height: 1.55;
		color: var(--color-foreground-muted);
		margin: 0.875rem 0 1.25rem;
		max-width: 22rem;
	}
	.le-cta {
		display: inline-flex;
		align-items: center;
		gap: 0.375rem;
		padding: 0.5rem 0.875rem;
		border-radius: 0.625rem;
		border: none;
		background: var(--color-primary);
		color: var(--color-highlight-foreground, #fff);
		font-family: var(--font-sans);
		font-size: 0.9375rem;
		cursor: pointer;
		transition: background-color var(--duration-fast) ease;
	}
	.le-cta:hover {
		background: var(--color-primary-hover);
	}

	.verso-foot {
		margin-top: auto;
		padding-top: 2rem;
	}
	.a-year-ago {
		font-family: var(--font-serif);
		font-style: italic;
		font-size: 0.875rem;
		line-height: 1.5;
		color: var(--color-foreground-subtle);
	}

	/* Recto — the living day ------------------------------------------------ */
	.recto {
		display: flex;
		flex-direction: column;
	}
	.hero {
		display: flex;
		flex-direction: column;
		gap: 0.625rem;
	}
	.kicker {
		font-family: var(--font-mono);
		font-size: 0.6875rem;
		text-transform: uppercase;
		letter-spacing: 0.14em;
		color: var(--color-foreground-subtle);
	}
	.hero-body {
		text-align: left;
		background: transparent;
		border: none;
		padding: 0;
		cursor: pointer;
		font-family: var(--font-serif);
		font-weight: 300;
		font-size: clamp(1.5rem, 2.7vw, 1.95rem);
		line-height: 1.3;
		letter-spacing: -0.01em;
		color: var(--color-foreground);
		transition: color var(--duration-fast) ease;
	}
	.hero-body:hover {
		color: var(--color-foreground-muted);
	}
	.hero.empty {
		gap: 0.75rem;
	}
	.hero-body-static {
		font-family: var(--font-serif);
		font-weight: 300;
		font-size: clamp(1.5rem, 2.7vw, 1.95rem);
		line-height: 1.3;
		letter-spacing: -0.01em;
		color: var(--color-foreground);
		margin: 0;
	}
	.hero-sub {
		font-family: var(--font-sans);
		font-size: 0.9375rem;
		line-height: 1.55;
		color: var(--color-foreground-muted);
		margin: 0;
		max-width: 24rem;
	}

	.ledger {
		display: flex;
		flex-direction: column;
		gap: 0.5rem;
		margin-top: 1.75rem;
	}
	.ledger-row {
		display: flex;
		align-items: baseline;
		gap: 0.5rem;
		width: 100%;
		background: transparent;
		border: none;
		padding: 0;
		cursor: pointer;
		font-family: var(--font-mono);
		font-size: 0.8125rem;
		text-align: left;
	}
	.ledger-row .lbl {
		color: var(--color-foreground-subtle);
		flex-shrink: 0;
		transition: color var(--duration-fast) ease;
	}
	.ledger-row .leader {
		flex: 1;
		border-bottom: 1px dotted var(--color-border-strong);
		transform: translateY(-0.2em);
		min-width: 1.5rem;
	}
	.ledger-row .val {
		flex-shrink: 0;
		color: var(--color-foreground-muted);
		transition: color var(--duration-fast) ease;
	}
	.ledger-row:hover .lbl,
	.ledger-row:hover .val {
		color: var(--color-foreground);
	}

	.recto-actions {
		display: flex;
		flex-direction: column;
		gap: 1.75rem;
		margin-top: 2.25rem;
	}
	.examine {
		display: inline-flex;
		align-items: center;
		gap: 0.4375rem;
		align-self: flex-start;
		padding: 0.5625rem 0.875rem;
		border: 1px solid var(--color-border-strong);
		border-radius: 0.625rem;
		background: transparent;
		color: var(--color-foreground);
		font-family: var(--font-sans);
		font-size: 0.9375rem;
		cursor: pointer;
		transition:
			border-color var(--duration-fast) ease,
			background-color var(--duration-fast) ease;
	}
	.examine:hover {
		border-color: var(--color-foreground-muted);
		background: var(--color-surface-elevated);
	}
	.continue {
		display: flex;
		flex-direction: column;
		gap: 0.375rem;
	}
	.continue-head {
		font-family: var(--font-mono);
		font-size: 0.6875rem;
		text-transform: uppercase;
		letter-spacing: 0.14em;
		color: var(--color-foreground-subtle);
		margin-bottom: 0.375rem;
	}
	.continue-item {
		text-align: left;
		background: transparent;
		border: none;
		padding: 0;
		cursor: pointer;
		font-family: var(--font-serif);
		font-size: 0.9375rem;
		color: var(--color-foreground-muted);
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
		transition: color var(--duration-fast) ease;
	}
	.continue-item:hover {
		color: var(--color-foreground);
	}

	/* Composer dock --------------------------------------------------------- */
	.composer-dock {
		flex-shrink: 0;
		display: flex;
		justify-content: center;
		padding: 1rem 2rem clamp(1.25rem, 3vh, 2rem);
		border-top: 1px solid var(--color-border);
		background: var(--color-background);
	}
	.composer-inner {
		width: 100%;
		max-width: 62rem;
		opacity: 0.85;
		transition: opacity var(--duration-normal) ease;
	}
	.composer-inner:focus-within {
		opacity: 1;
	}

	/* Narrow — fold the spread into a single column, composer stays docked --- */
	@media (max-width: 52rem) {
		.spread {
			grid-template-columns: 1fr;
			gap: 2rem;
		}
		.gutter {
			display: none;
		}
		.verso {
			min-height: 0;
			padding-bottom: 2rem;
			border-bottom: 1px solid var(--color-border);
		}
		.verso-foot {
			margin-top: 1.5rem;
		}
		.spread-body {
			align-items: start;
		}
	}
</style>
