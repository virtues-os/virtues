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

	// HomeViewFolio — the "examined-life almanac" prototype. The home surface is
	// one living page in an ongoing book about you: a folio line numbers the day,
	// the hero is a *salient* slot (the salience engine's winner — Likeness by
	// default), and the day is read as a mono ledger. Type triad in service of the
	// motif: JJannon/Lora serif for the voice, IBM Plex Mono for the ledger &
	// folio, Avenir for labels. Non-Likeness content below is HARDCODED until the
	// praxis / salience / novelty endpoints exist.

	let input = $state("");
	let preferredName = $state<string | undefined>(undefined);
	let narrative = $state<string | undefined>(undefined);

	// Day number since the record began — the almanac's spine. Placeholder start
	// date until an account-created timestamp is wired in.
	const RECORD_START = new Date(2025, 11, 5); // 2025-12-05
	const now = new Date();
	const dayNumber = Math.max(
		1,
		Math.floor((+now - +RECORD_START) / 86_400_000) + 1,
	);
	const longDate = now.toLocaleDateString(undefined, {
		weekday: "long",
		month: "long",
		day: "numeric",
	});

	// --- HARDCODED placeholders (wire to real endpoints later) ---
	const stateLine = "Slept 6h 20m · Clear, 54°";
	const onThisDay = "A year ago today: you first wrote about leaving the job.";
	// -------------------------------------------------------------

	const narrativeGlimpse = $derived.by(() => {
		const text = narrative?.trim();
		if (!text) return undefined;
		const first = text.match(/^.*?[.!?](?:\s|$)/)?.[0]?.trim();
		const g = first && first.length <= 200 ? first : text;
		return g.length > 180 ? g.slice(0, 178).trimEnd() + "…" : g;
	});

	// The salient hero: a ranked set of candidates the salience engine would
	// choose between. One slot, many archetypes. Click to cycle — this is how the
	// prototype lets you *feel* "salience picks the winner". Likeness leads on a
	// quiet day; an eventful day would promote novelty / an open loop / an
	// undercurrent above it.
	type Hero = {
		kicker: string;
		body: string;
		meta: string;
		italic?: boolean;
		route?: string;
		routeLabel?: string;
		dot?: boolean;
	};
	const heroes = $derived.by<Hero[]>(() => {
		const list: Hero[] = [];
		if (narrativeGlimpse)
			list.push({
				kicker: "Who you're becoming",
				body: narrativeGlimpse,
				meta: "your Likeness · edited 3 days ago",
				italic: true,
				route: "/narrative-identity/present",
				routeLabel: "You",
				dot: true,
			});
		list.push({
			kicker: "Most novel today",
			body: "Lunch with Sarah — first time in three months.",
			meta: "surprise × significance",
			route: "/day",
			routeLabel: "Today",
		});
		list.push({
			kicker: "Still open",
			body: "Three things you said you'd do, and haven't yet.",
			meta: "two from Monday · one from last week",
		});
		list.push({
			kicker: "An undercurrent",
			body: "You've written the word “tired” four times this week.",
			meta: "noticed across your journal and chats",
		});
		return list;
	});
	let heroIndex = $state(0);
	const hero = $derived(heroes[heroIndex % Math.max(1, heroes.length)]);
	function nextHero() {
		heroIndex = (heroIndex + 1) % Math.max(1, heroes.length);
	}

	type Recent = { route: string; title: string; icon: string; ts: number };
	const recents = $derived.by<Recent[]>(() => {
		const chats: Recent[] = chatSessions.sessions.map((c) => ({
			route: `/chat/${c.conversation_id}`,
			title: c.title || "Untitled",
			icon: c.icon || "ri:chat-3-line",
			ts: c.last_updated ? Date.parse(c.last_updated) : 0,
		}));
		const pages: Recent[] = pagesStore.pages.map((p) => ({
			route: `/page/${p.id}`,
			title: p.title || "Untitled",
			icon: p.icon || "ri:file-text-line",
			ts: p.updated_at ? Date.parse(p.updated_at) : 0,
		}));
		return [...chats, ...pages].sort((a, b) => b.ts - a.ts).slice(0, 4);
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
	const hasEntryToday = $derived(journal.length > 0);

	async function newJournalEntry() {
		try {
			const page = await createReflection(todayDate);
			journal = [...journal, page];
			open(`/page/${page.id}`, page.title || "Untitled");
		} catch (e) {
			console.error("Failed to create journal entry:", e);
		}
	}

	onMount(() => {
		fetch("/api/profile")
			.then((r) => (r.ok ? r.json() : null))
			.then((p) => {
				if (p?.preferred_name) preferredName = p.preferred_name;
			})
			.catch(() => {});
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

<div class="folio-scroll">
	<div class="folio">
		<!-- Folio line: the almanac spine -->
		<header class="folio-head">
			<span class="folio-day">Day {dayNumber}</span>
			<span class="folio-date">{longDate}</span>
			<span class="folio-almanac">{stateLine}</span>
		</header>
		<div class="rule"></div>

		<!-- Hero: the salient slot. Click to cycle the candidates. -->
		<section class="hero">
			<span class="kicker">{hero.kicker}</span>
			<button
				type="button"
				class="hero-body"
				class:italic={hero.italic}
				onclick={() =>
					hero.route
						? open(hero.route, hero.routeLabel || "")
						: nextHero()}
			>
				{#if hero.italic}“{hero.body}”{:else}{hero.body}{/if}
			</button>
			<div class="hero-foot">
				{#if hero.dot}<span class="diff-dot" title="recently changed"></span>{/if}
				<span class="hero-meta">{hero.meta}</span>
				<button
					type="button"
					class="hero-next"
					onclick={nextHero}
					title="See what else surfaced"
				>
					next <Icon icon="ri:arrow-right-line" width="13" />
				</button>
			</div>
		</section>

		<!-- Marginalia — the flourish only a whole-timeline product earns -->
		<aside class="marginalia">{onThisDay}</aside>

		<div class="rule"></div>

		<!-- Today, examined — the ledger -->
		<section class="ledger">
			<span class="eyebrow">Today, examined</span>
			<button class="ledger-row" onclick={() => open("/day", "Today")}>
				<span class="lbl">Most novel</span>
				<span class="leader"></span>
				<span class="val">Lunch with Sarah — first in 3 months</span>
			</button>
			<div class="ledger-row static">
				<span class="lbl">Still open</span>
				<span class="leader"></span>
				<span class="val">3 promises, unkept</span>
			</div>
			<div class="ledger-row static">
				<span class="lbl">Undercurrent</span>
				<span class="leader"></span>
				<span class="val">“tired” · 4× this week</span>
			</div>
		</section>

		<div class="rule"></div>

		<!-- Ritual + continue -->
		<section class="foot-cols">
			<div class="foot-col">
				<span class="eyebrow">The examen</span>
				<button
					type="button"
					class="examine"
					onclick={hasEntryToday
						? () =>
								open(
									`/page/${journal[0].id}`,
									journal[0].title || "Today",
								)
						: newJournalEntry}
				>
					<Icon icon="ri:quill-pen-line" width="15" />
					{hasEntryToday ? "Continue today's entry" : "Examine today — two minutes"}
				</button>
			</div>
			<div class="foot-col">
				<span class="eyebrow">Where you left off</span>
				{#if recents.length > 0}
					<div class="continue-list">
						{#each recents as r (r.route)}
							<button
								type="button"
								class="continue-item"
								onclick={() => open(r.route, r.title)}
							>
								<span class="continue-title">{r.title}</span>
							</button>
						{/each}
					</div>
				{:else}
					<span class="continue-empty">Nothing yet.</span>
				{/if}
			</div>
		</section>

		<!-- Omnibar: quiet, at the foot. The verb, not the place. -->
		<div class="omnibar">
			<ChatInput
				bind:value={input}
				placeholder="Ask, or begin writing…"
				maxWidth="max-w-none"
				on:submit={(e) => askVirtues(e.detail)}
			/>
		</div>
	</div>
</div>

<style>
	.folio-scroll {
		height: 100%;
		width: 100%;
		overflow-y: auto;
		display: flex;
		justify-content: center;
	}

	.folio {
		position: relative;
		width: 100%;
		max-width: 40rem;
		padding: clamp(2.5rem, 8vh, 5.5rem) 2rem 5rem;
		display: flex;
		flex-direction: column;
	}

	/* Folio line ------------------------------------------------------------ */
	.folio-head {
		display: flex;
		align-items: baseline;
		gap: 1rem;
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

	.rule {
		height: 1px;
		background: var(--color-border);
		margin: 1.25rem 0;
	}

	/* Hero ------------------------------------------------------------------ */
	.hero {
		display: flex;
		flex-direction: column;
		gap: 0.75rem;
		padding: 0.5rem 0 0.25rem;
	}
	.kicker {
		font-family: var(--font-mono);
		font-size: 0.6875rem;
		text-transform: uppercase;
		letter-spacing: 0.16em;
		color: var(--color-foreground-subtle);
	}
	.hero-body {
		display: block;
		text-align: left;
		border: none;
		background: transparent;
		padding: 0;
		cursor: pointer;
		font-family: var(--font-serif);
		font-weight: 300;
		font-size: clamp(1.6rem, 3.4vw, 2.15rem);
		line-height: 1.28;
		letter-spacing: -0.01em;
		color: var(--color-foreground);
		transition: color var(--duration-fast) ease;
	}
	.hero-body.italic {
		font-style: italic;
	}
	.hero-body:hover {
		color: var(--color-foreground-muted);
	}
	.hero-foot {
		display: flex;
		align-items: center;
		gap: 0.5rem;
		font-family: var(--font-mono);
		font-size: 0.75rem;
		color: var(--color-foreground-subtle);
	}
	.diff-dot {
		width: 6px;
		height: 6px;
		border-radius: 50%;
		background: var(--color-primary);
		flex-shrink: 0;
	}
	.hero-meta {
		flex: 1;
	}
	.hero-next {
		display: inline-flex;
		align-items: center;
		gap: 0.25rem;
		background: transparent;
		border: none;
		cursor: pointer;
		font: inherit;
		color: var(--color-foreground-subtle);
		transition: color var(--duration-fast) ease;
	}
	.hero-next:hover {
		color: var(--color-foreground);
	}

	/* Marginalia — sits in the right margin on wide screens, inline otherwise */
	.marginalia {
		font-family: var(--font-serif);
		font-style: italic;
		font-size: 0.875rem;
		line-height: 1.5;
		color: var(--color-foreground-subtle);
		margin-top: 1rem;
	}
	@media (min-width: 60rem) {
		.marginalia {
			position: absolute;
			right: -11.5rem;
			width: 9.5rem;
			margin-top: 0;
			top: 14rem;
			text-align: left;
			padding-left: 0.875rem;
			border-left: 1px solid var(--color-border);
		}
	}

	/* Ledger ---------------------------------------------------------------- */
	.eyebrow {
		font-family: var(--font-mono);
		font-size: 0.6875rem;
		text-transform: uppercase;
		letter-spacing: 0.16em;
		color: var(--color-foreground-subtle);
		display: block;
		margin-bottom: 0.875rem;
	}
	.ledger {
		display: flex;
		flex-direction: column;
		gap: 0.625rem;
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
		font-size: 0.875rem;
		color: var(--color-foreground);
		text-align: left;
	}
	.ledger-row.static {
		cursor: default;
	}
	.ledger-row .lbl {
		color: var(--color-foreground-subtle);
		flex-shrink: 0;
	}
	.ledger-row .leader {
		flex: 1;
		border-bottom: 1px dotted var(--color-border-strong);
		transform: translateY(-0.2em);
		min-width: 1.5rem;
	}
	.ledger-row .val {
		flex-shrink: 0;
		color: var(--color-foreground);
	}
	.ledger-row:not(.static):hover .val {
		color: var(--color-primary);
	}

	/* Foot ------------------------------------------------------------------ */
	.foot-cols {
		display: grid;
		grid-template-columns: 1fr 1fr;
		gap: 2rem;
	}
	@media (max-width: 34rem) {
		.foot-cols {
			grid-template-columns: 1fr;
			gap: 1.5rem;
		}
	}
	.foot-col {
		display: flex;
		flex-direction: column;
		min-width: 0;
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
	.continue-list {
		display: flex;
		flex-direction: column;
		gap: 0.375rem;
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
	.continue-empty {
		font-size: 0.875rem;
		color: var(--color-foreground-subtle);
	}

	.omnibar {
		margin-top: 2.5rem;
		opacity: 0.85;
		transition: opacity var(--duration-normal) ease;
	}
	.omnibar:focus-within {
		opacity: 1;
	}
</style>
