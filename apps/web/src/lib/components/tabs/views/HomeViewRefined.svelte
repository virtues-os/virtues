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

	// HomeViewRefined — keeps the current dark aesthetic and type stack, but fixes
	// the spine: a thin persistent identity frame (Likeness + day) up top, a single
	// *salient* hero card as the focal point, the day read as one tidy block, and
	// chat demoted from hero to a quiet omnibar at the foot. Non-Likeness content
	// is HARDCODED until the salience / novelty / praxis endpoints exist.

	let input = $state("");
	let preferredName = $state<string | undefined>(undefined);
	let narrative = $state<string | undefined>(undefined);

	const now = new Date();
	const greeting = $derived.by(() => {
		const h = now.getHours();
		const part = h < 12 ? "Good morning" : h < 18 ? "Good afternoon" : "Good evening";
		return preferredName ? `${part}, ${preferredName}.` : `${part}.`;
	});
	const shortDate = now.toLocaleDateString(undefined, {
		weekday: "long",
		month: "long",
		day: "numeric",
	});

	// --- HARDCODED placeholders ---
	const stateLine = "Slept 6h 20m · Clear, 54°";
	// ------------------------------

	const narrativeGlimpse = $derived.by(() => {
		const text = narrative?.trim();
		if (!text) return undefined;
		const first = text.match(/^.*?[.!?](?:\s|$)/)?.[0]?.trim();
		const g = first && first.length <= 200 ? first : text;
		return g.length > 150 ? g.slice(0, 148).trimEnd() + "…" : g;
	});

	// One salient hero slot — same concept as the folio variant. Likeness leads by
	// default; click to cycle through what else the salience engine surfaced.
	type Hero = {
		kicker: string;
		body: string;
		meta: string;
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

<div class="rf-scroll">
	<div class="rf">
		<!-- Identity frame: thin, persistent, never the focal point -->
		<header class="frame">
			<h1 class="greeting">{greeting}</h1>
			<div class="frame-meta">
				<span>{shortDate}</span>
				<span class="sep">·</span>
				<span>{stateLine}</span>
			</div>
		</header>

		<!-- The salient hero card — the one thing to look at -->
		<section class="hero-card">
			<div class="hero-top">
				<span class="kicker">{hero.kicker}</span>
				{#if hero.dot}<span class="diff-dot" title="recently changed"></span>{/if}
			</div>
			<button
				type="button"
				class="hero-body"
				onclick={() =>
					hero.route ? open(hero.route, hero.routeLabel || "") : nextHero()}
			>
				{hero.body}
			</button>
			<div class="hero-foot">
				<span class="hero-meta">{hero.meta}</span>
				<button type="button" class="hero-next" onclick={nextHero}>
					next <Icon icon="ri:arrow-right-line" width="13" />
				</button>
			</div>
		</section>

		<!-- Today, examined — one tidy block -->
		<section class="examined">
			<span class="eyebrow">Today, examined</span>
			<button class="ex-row" onclick={() => open("/day", "Today")}>
				<span class="ex-lbl">Most novel</span>
				<span class="ex-val">Lunch with Sarah — first in 3 months</span>
			</button>
			<div class="ex-row static">
				<span class="ex-lbl">Still open</span>
				<span class="ex-val">3 promises, unkept</span>
			</div>
			<div class="ex-row static">
				<span class="ex-lbl">Undercurrent</span>
				<span class="ex-val">“tired” · 4× this week</span>
			</div>
		</section>

		<!-- Ritual + continue, side by side, both quiet -->
		<section class="cols">
			<div class="col">
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
					{hasEntryToday ? "Continue today's entry" : "Examine today"}
				</button>
			</div>
			<div class="col">
				<span class="eyebrow">Where you left off</span>
				{#if recents.length > 0}
					<div class="continue-list">
						{#each recents as r (r.route)}
							<button
								type="button"
								class="continue-item"
								onclick={() => open(r.route, r.title)}
							>
								<Icon icon={r.icon} width="14" class="ci-icon" />
								<span class="ci-title">{r.title}</span>
							</button>
						{/each}
					</div>
				{:else}
					<span class="continue-empty">Nothing yet.</span>
				{/if}
			</div>
		</section>

		<!-- Omnibar — demoted to the foot -->
		<div class="omnibar">
			<ChatInput
				bind:value={input}
				placeholder="Ask anything, or begin writing…"
				maxWidth="max-w-none"
				on:submit={(e) => askVirtues(e.detail)}
			/>
		</div>
	</div>
</div>

<style>
	.rf-scroll {
		height: 100%;
		width: 100%;
		overflow-y: auto;
		display: flex;
		justify-content: center;
	}
	.rf {
		width: 100%;
		max-width: 42rem;
		padding: clamp(2.5rem, 9vh, 6rem) 2rem 5rem;
		display: flex;
		flex-direction: column;
		gap: 2rem;
	}

	/* Identity frame -------------------------------------------------------- */
	.frame {
		display: flex;
		flex-direction: column;
		gap: 0.375rem;
	}
	.greeting {
		font-family: var(--font-serif);
		font-weight: 300;
		font-size: 1.875rem;
		line-height: 1.1;
		letter-spacing: -0.01em;
		color: var(--color-foreground);
		margin: 0;
	}
	.frame-meta {
		display: flex;
		gap: 0.5rem;
		font-size: 0.8125rem;
		color: var(--color-foreground-subtle);
	}
	.frame-meta .sep {
		opacity: 0.6;
	}

	/* Hero card — the focal element ---------------------------------------- */
	.hero-card {
		display: flex;
		flex-direction: column;
		gap: 0.875rem;
		padding: 1.5rem 1.625rem;
		border: 1px solid var(--color-border);
		border-radius: 1rem;
		background: var(--color-surface-elevated);
	}
	.hero-top {
		display: flex;
		align-items: center;
		gap: 0.5rem;
	}
	.kicker {
		font-family: var(--font-mono);
		font-size: 0.6875rem;
		text-transform: uppercase;
		letter-spacing: 0.14em;
		color: var(--color-foreground-subtle);
	}
	.diff-dot {
		width: 6px;
		height: 6px;
		border-radius: 50%;
		background: var(--color-primary);
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
		font-size: clamp(1.4rem, 2.8vw, 1.75rem);
		line-height: 1.32;
		letter-spacing: -0.01em;
		color: var(--color-foreground);
		transition: color var(--duration-fast) ease;
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

	/* Today, examined ------------------------------------------------------- */
	.eyebrow {
		font-family: var(--font-mono);
		font-size: 0.6875rem;
		text-transform: uppercase;
		letter-spacing: 0.14em;
		color: var(--color-foreground-subtle);
		display: block;
		margin-bottom: 0.875rem;
	}
	.examined {
		display: flex;
		flex-direction: column;
	}
	.ex-row {
		display: flex;
		align-items: baseline;
		gap: 1rem;
		width: 100%;
		background: transparent;
		border: none;
		border-top: 1px solid var(--color-border-subtle);
		padding: 0.625rem 0;
		cursor: pointer;
		text-align: left;
	}
	.ex-row.static {
		cursor: default;
	}
	.ex-row:last-child {
		border-bottom: 1px solid var(--color-border-subtle);
	}
	.ex-lbl {
		flex-shrink: 0;
		width: 6.5rem;
		font-family: var(--font-mono);
		font-size: 0.75rem;
		color: var(--color-foreground-subtle);
	}
	.ex-val {
		font-size: 0.9375rem;
		color: var(--color-foreground);
	}
	.ex-row:not(.static):hover .ex-val {
		color: var(--color-primary);
	}

	/* Columns --------------------------------------------------------------- */
	.cols {
		display: grid;
		grid-template-columns: 1fr 1fr;
		gap: 2rem;
	}
	@media (max-width: 34rem) {
		.cols {
			grid-template-columns: 1fr;
			gap: 1.5rem;
		}
	}
	.col {
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
		background: var(--color-surface);
	}
	.continue-list {
		display: flex;
		flex-direction: column;
		gap: 0.125rem;
	}
	.continue-item {
		display: flex;
		align-items: center;
		gap: 0.5rem;
		width: 100%;
		text-align: left;
		background: transparent;
		border: none;
		padding: 0.375rem 0.5rem;
		margin: 0 -0.5rem;
		border-radius: 0.5rem;
		cursor: pointer;
		transition: background-color var(--duration-fast) ease;
	}
	.continue-item:hover {
		background: var(--color-surface-elevated);
	}
	.continue-item :global(.ci-icon) {
		color: var(--color-foreground-subtle);
		flex-shrink: 0;
	}
	.ci-title {
		min-width: 0;
		font-size: 0.9375rem;
		color: var(--color-foreground-muted);
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.continue-empty {
		font-size: 0.875rem;
		color: var(--color-foreground-subtle);
	}

	.omnibar {
		opacity: 0.8;
		transition: opacity var(--duration-normal) ease;
	}
	.omnibar:focus-within {
		opacity: 1;
	}
</style>
