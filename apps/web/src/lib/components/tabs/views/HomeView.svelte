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

	// Home — the default landing / "Return" surface. Three calm bands: ask · today
	// · continue. Recents are real; the "today" band is placeholder data until the
	// praxis / calendar / biometric endpoints exist (marked HARDCODED below).

	let input = $state("");
	let inputFocused = $state(false);
	let preferredName = $state<string | undefined>(undefined);
	// The self-model prose the user authored (real, persisted). A one-line glimpse
	// of it on Home is the single thing no other product could render — see
	// NarrativeIdentityView "present" checkpoint.
	let narrative = $state<string | undefined>(undefined);

	const greeting = $derived.by(() => {
		const h = new Date().getHours();
		const part = h < 12 ? "Good morning" : h < 18 ? "Good afternoon" : "Good evening";
		return preferredName ? `${part}, ${preferredName}.` : `${part}.`;
	});

	// --- HARDCODED placeholders (wire to real endpoints later) ---
	const stateLine = "Slept 6h 20m · Clear, 54°";
	const praxis = ["Morning examen", "Fast until noon"];
	const agenda = ["9:00 Standup", "2:00 Dentist"];
	const highlight = "Lunch with Sarah — first time in 3 months";
	// -------------------------------------------------------------

	const suggestions = [
		"What should I focus on today?",
		"Summarize what happened yesterday",
		"What have I been putting off?",
	];

	// Real "pick up where you left off" — recent chats + pages, freshest first.
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
		return [...chats, ...pages].sort((a, b) => b.ts - a.ts).slice(0, 5);
	});

	function open(route: string, title: string) {
		windowShellStore.openTabFromRoute(route, { label: title });
	}

	// Journal — today's reflection pages (same entities as the day page's "Your
	// writing"). Journaling stays opt-in: pages are pages, this is just a calm
	// entry point into today's. Date is the local calendar day (YYYY-MM-DD).
	const todayDate = (() => {
		const d = new Date();
		const y = d.getFullYear();
		const m = String(d.getMonth() + 1).padStart(2, "0");
		const day = String(d.getDate()).padStart(2, "0");
		return `${y}-${m}-${day}`;
	})();

	let journal = $state<Page[]>([]);

	async function newJournalEntry() {
		try {
			const page = await createReflection(todayDate);
			journal = [...journal, page];
			open(`/page/${page.id}`, page.title || "Untitled");
		} catch (e) {
			console.error("Failed to create journal entry:", e);
		}
	}

	// A short glimpse of the authored self-model — first sentence or ~130 chars.
	const narrativeGlimpse = $derived.by(() => {
		const text = narrative?.trim();
		if (!text) return undefined;
		const firstSentence = text.match(/^.*?[.!?](?:\s|$)/)?.[0]?.trim();
		const glimpse = firstSentence && firstSentence.length <= 160 ? firstSentence : text;
		return glimpse.length > 140 ? glimpse.slice(0, 138).trimEnd() + "…" : glimpse;
	});

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
		if (chatSessions.sessions.length === 0 && !chatSessions.isLoading) chatSessions.load();
		if (pagesStore.pages.length === 0 && !pagesStore.pagesLoading) pagesStore.loadPages();
		getReflectionsForDate(todayDate)
			.then((pages) => (journal = pages))
			.catch(() => {});
		setTimeout(() => (inputFocused = true), 60);
	});
</script>

<div class="home-scroll">
	<div class="home-column">
		<h1 class="greeting">{greeting}</h1>

		{#if narrativeGlimpse}
			<button
				type="button"
				class="glimpse"
				onclick={() => open("/narrative-identity/present", "You")}
			>
				<span class="glimpse-quote">“{narrativeGlimpse}”</span>
			</button>
		{/if}

		<div class="composer">
			<ChatInput
				bind:value={input}
				bind:focused={inputFocused}
				placeholder="Ask anything"
				maxWidth="max-w-none"
				on:submit={(e) => askVirtues(e.detail)}
			/>
		</div>

		<div class="chips">
			{#each suggestions as s (s)}
				<button type="button" class="chip" onclick={() => askVirtues(s)}>{s}</button>
			{/each}
		</div>

		<!-- Band: Today -->
		<section class="band">
			<h2 class="band-title">Today</h2>

			<div class="state-line">
				<Icon icon="ri:moon-line" width="14" class="state-icon" />
				<span>{stateLine}</span>
			</div>

			<div class="today-cols">
				<div class="col">
					<span class="col-head">Praxis</span>
					{#each praxis as p (p)}
						<div class="col-item"><span class="dot"></span>{p}</div>
					{/each}
				</div>
				<div class="col">
					<span class="col-head">Agenda</span>
					{#each agenda as a (a)}
						<div class="col-item"><span class="dot"></span>{a}</div>
					{/each}
				</div>
			</div>

			<div class="highlight">
				<Icon icon="ri:sparkling-2-line" width="14" class="highlight-icon" />
				<span><span class="highlight-label">Most novel so far</span> — {highlight}</span>
			</div>

			<button type="button" class="open-today" onclick={() => open("/day", "Today")}>
				Open today
				<Icon icon="ri:arrow-right-s-line" width="16" />
			</button>
		</section>

		<!-- Band: Journal -->
		<section class="band">
			<div class="band-head">
				<h2 class="band-title">Journal</h2>
				{#if journal.length > 0}
					<button type="button" class="band-action" onclick={newJournalEntry}>
						<Icon icon="ri:add-line" width="15" />
						New entry
					</button>
				{/if}
			</div>
			{#if journal.length > 0}
				<div class="recent-list">
					{#each journal as j (j.id)}
						<button
							type="button"
							class="recent-item"
							onclick={() => open(`/page/${j.id}`, j.title || "Untitled")}
						>
							<Icon
								icon={j.icon || "ri:quill-pen-line"}
								width="15"
								class="recent-icon"
							/>
							<span class="recent-title">{j.title || "Untitled"}</span>
							<Icon icon="ri:arrow-right-up-line" width="14" class="recent-go" />
						</button>
					{/each}
				</div>
			{:else}
				<button type="button" class="journal-empty" onclick={newJournalEntry}>
					<Icon icon="ri:quill-pen-line" width="15" />
					Write today’s entry
				</button>
			{/if}
		</section>

		<!-- Band: Continue -->
		<section class="band">
			<h2 class="band-title">Recents</h2>
			{#if recents.length > 0}
				<div class="recent-list">
					{#each recents as r (r.route)}
						<button type="button" class="recent-item" onclick={() => open(r.route, r.title)}>
							<Icon icon={r.icon} width="15" class="recent-icon" />
							<span class="recent-title">{r.title}</span>
							<Icon icon="ri:arrow-right-up-line" width="14" class="recent-go" />
						</button>
					{/each}
				</div>
			{:else}
				<p class="band-empty">Nothing yet — ask something above to begin.</p>
			{/if}
		</section>
	</div>
</div>

<style>
	.home-scroll {
		height: 100%;
		width: 100%;
		overflow-y: auto;
		display: flex;
		justify-content: center;
	}

	.home-column {
		width: 100%;
		max-width: 44rem;
		padding: clamp(2.5rem, 10vh, 7rem) 2rem 6rem;
		display: flex;
		flex-direction: column;
		gap: 1.25rem;
	}

	.greeting {
		font-family: var(--font-serif);
		font-size: 2.25rem;
		font-weight: 400;
		line-height: 1.15;
		letter-spacing: -0.01em;
		color: var(--color-foreground);
		text-align: center;
		margin-bottom: 0;
	}

	/* The glimpse — the user's own authored words, the one line no other product
	   could show. A quiet pull-quote, not a control; clicks through to "You". */
	.glimpse {
		display: block;
		width: 100%;
		max-width: 30rem;
		margin: 0.375rem auto 0.25rem;
		padding: 0.25rem 0.5rem;
		background: transparent;
		border: none;
		cursor: pointer;
		text-align: center;
	}

	.glimpse-quote {
		font-family: var(--font-serif);
		font-size: 1.0625rem;
		font-style: italic;
		line-height: 1.55;
		color: var(--color-foreground-muted);
		transition: color 0.15s ease;
	}

	.glimpse:hover .glimpse-quote {
		color: var(--color-foreground);
	}

	.composer {
		width: 100%;
	}

	.chips {
		display: flex;
		flex-wrap: wrap;
		gap: 0.5rem;
		justify-content: center;
	}

	.chip {
		padding: 0.4375rem 0.8125rem;
		border-radius: 999px;
		border: 1px solid var(--color-border);
		background: transparent;
		color: var(--color-foreground-muted);
		font-size: 0.8125rem;
		cursor: pointer;
		transition:
			background-color 0.15s ease,
			color 0.15s ease,
			border-color 0.15s ease;
	}

	.chip:hover {
		background: var(--color-surface-elevated);
		color: var(--color-foreground);
		border-color: var(--color-border-strong);
	}

	.band {
		margin-top: 1.5rem;
		display: flex;
		flex-direction: column;
	}

	.band-title {
		font-family: var(--font-sans);
		font-size: 1.125rem;
		font-weight: 500;
		color: var(--color-foreground);
		margin-bottom: 0.75rem;
	}

	/* Band header with a trailing action (e.g. Journal "New entry") */
	.band-head {
		display: flex;
		align-items: baseline;
		justify-content: space-between;
		margin-bottom: 0.75rem;
	}

	.band-head .band-title {
		margin-bottom: 0;
	}

	.band-action {
		display: inline-flex;
		align-items: center;
		gap: 0.1875rem;
		padding: 0.25rem 0.25rem;
		background: transparent;
		border: none;
		color: var(--color-foreground-muted);
		font-size: 0.8125rem;
		cursor: pointer;
		transition: color 0.15s ease;
	}

	.band-action:hover {
		color: var(--color-foreground);
	}

	/* Empty-state call to action for the Journal band */
	.journal-empty {
		display: inline-flex;
		align-items: center;
		gap: 0.4375rem;
		align-self: flex-start;
		padding: 0.5625rem 0.75rem;
		border: 1px dashed var(--color-border);
		border-radius: 0.625rem;
		background: transparent;
		color: var(--color-foreground-muted);
		font-size: 0.9375rem;
		cursor: pointer;
		transition:
			color 0.15s ease,
			border-color 0.15s ease,
			background-color 0.12s ease;
	}

	.journal-empty:hover {
		color: var(--color-foreground);
		border-color: var(--color-border-strong);
		background: var(--color-surface-elevated);
	}

	.state-line {
		display: flex;
		align-items: center;
		gap: 0.4375rem;
		font-size: 0.8125rem;
		color: var(--color-foreground-muted);
		padding: 0 0.25rem 0.75rem;
	}

	.state-line :global(.state-icon) {
		color: var(--color-foreground-subtle);
	}

	.today-cols {
		display: grid;
		grid-template-columns: 1fr 1fr;
		gap: 1rem;
		padding: 0 0.25rem;
	}

	.col {
		display: flex;
		flex-direction: column;
		gap: 0.375rem;
	}

	.col-head {
		font-size: 0.6875rem;
		font-weight: 500;
		text-transform: uppercase;
		letter-spacing: 0.08em;
		color: var(--color-foreground-subtle);
	}

	.col-item {
		display: flex;
		align-items: center;
		gap: 0.5rem;
		font-size: 0.9375rem;
		color: var(--color-foreground);
	}

	.col-item .dot {
		width: 5px;
		height: 5px;
		border-radius: 50%;
		background: var(--color-foreground-subtle);
		flex-shrink: 0;
	}

	.highlight {
		display: flex;
		align-items: center;
		gap: 0.4375rem;
		margin-top: 0.875rem;
		padding: 0 0.25rem;
		font-size: 0.9375rem;
		color: var(--color-foreground);
	}

	.highlight :global(.highlight-icon) {
		color: var(--color-primary);
		flex-shrink: 0;
	}

	.highlight-label {
		color: var(--color-foreground-muted);
	}

	.open-today {
		display: inline-flex;
		align-items: center;
		gap: 0.125rem;
		align-self: flex-start;
		margin-top: 0.875rem;
		padding: 0.25rem 0.25rem;
		background: transparent;
		border: none;
		color: var(--color-primary);
		font-size: 0.875rem;
		cursor: pointer;
	}

	.open-today:hover {
		text-decoration: underline;
	}

	.recent-list {
		display: flex;
		flex-direction: column;
		gap: 0.25rem;
	}

	.recent-item {
		display: flex;
		align-items: center;
		gap: 0.625rem;
		width: 100%;
		padding: 0.5625rem 0.75rem;
		border: none;
		background: transparent;
		text-align: left;
		font: inherit;
		border-radius: 0.625rem;
		cursor: pointer;
		transition: background-color 0.12s ease;
	}

	.recent-item:hover {
		background: var(--color-surface-elevated);
	}

	.recent-item :global(.recent-icon) {
		color: var(--color-foreground-subtle);
		flex-shrink: 0;
	}

	.recent-title {
		flex: 1;
		min-width: 0;
		font-size: 0.9375rem;
		color: var(--color-foreground);
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.recent-item :global(.recent-go) {
		color: var(--color-foreground-subtle);
		opacity: 0;
		transition: opacity 0.12s ease;
		flex-shrink: 0;
	}

	.recent-item:hover :global(.recent-go) {
		opacity: 1;
	}

	.band-empty {
		font-size: 0.9375rem;
		color: var(--color-foreground-subtle);
		padding: 0.25rem 0.75rem;
	}
</style>
