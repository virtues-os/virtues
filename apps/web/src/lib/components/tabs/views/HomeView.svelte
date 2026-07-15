<!--
	HomeView.svelte

	Home is one quiet page: the day line, your Likeness, today's entry, where
	you left off, and a composer. Replaces the two-page "spread" prototype —
	no preview harness, no hardcoded salience props; everything here is real.
-->

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
		getNarrativeIdentity,
		type Page,
	} from "$lib/api/client";
	import { formatDate } from "$lib/utils/dateUtils";

	let input = $state("");
	let narrative = $state<string | undefined>(undefined);

	const now = new Date();
	const longDate = formatDate(now, {
		weekday: "long",
		month: "long",
		day: "numeric",
	});

	// The Likeness glimpse: the first sentence of the narrative identity, if any.
	const likenessText = $derived.by(() => {
		const text = narrative?.trim();
		if (!text) return undefined;
		const first = text.match(/^.*?[.!?](?:\s|$)/)?.[0]?.trim();
		const g = first && first.length <= 220 ? first : text;
		return g.length > 200 ? g.slice(0, 198).trimEnd() + "…" : g;
	});

	type Recent = { route: string; title: string; ts: number };
	const recents = $derived.by<Recent[]>(() => {
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
		return [...chats, ...pages].sort((a, b) => b.ts - a.ts).slice(0, 5);
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
		getNarrativeIdentity<{ content?: string }>()
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

<div class="home-scroll">
	<div class="home-body">
		<div class="home-page">
			<!-- Day line -->
			<header class="folio-head">
				<span class="folio-date">{longDate}</span>
				<button
					type="button"
					class="folio-today"
					onclick={() => open(`/day/day_${todayDate}`, "Today")}
				>
					Today
					<Icon icon="ri:arrow-right-line" width="13" />
				</button>
			</header>

			<!-- Likeness -->
			{#if likenessText}
				<button
					type="button"
					class="likeness"
					onclick={() => open("/narrative-identity/present", "You")}
				>
					“{likenessText}”
				</button>
			{:else}
				<div class="likeness-empty">
					<p class="le-lead">This page becomes you.</p>
					<p class="le-body">
						Your Likeness is a few honest lines about who you are.
						Virtues drafts it from what it learns — you keep it true.
					</p>
					<button
						type="button"
						class="le-cta"
						onclick={() => open("/narrative-identity/present", "You")}
					>
						Begin your Likeness
						<Icon icon="ri:arrow-right-line" width="15" />
					</button>
				</div>
			{/if}

			<!-- Actions -->
			<div class="home-actions">
				<button type="button" class="examine" onclick={examineToday}>
					<Icon icon="ri:quill-pen-line" width="15" />
					{hasEntryToday ? "Continue today's entry" : "Examine today"}
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
		</div>
	</div>

	<!-- Composer docked across the foot — the one place you act -->
	<div class="composer-dock">
		<div class="composer-inner">
			<ChatInput
				bind:value={input}
				placeholder="Ask about today, or begin writing…"
				maxWidth="max-w-none"
				onSubmit={(text) => askVirtues(text)}
			/>
		</div>
	</div>
</div>

<style>
	.home-scroll {
		position: relative;
		height: 100%;
		width: 100%;
		display: flex;
		flex-direction: column;
		min-height: 0;
	}

	.home-body {
		flex: 1;
		min-height: 0;
		overflow-y: auto;
		display: flex;
		justify-content: center;
		padding: clamp(1.75rem, 5vh, 3rem) 2rem 2rem;
	}

	.home-page {
		width: 100%;
		max-width: 40rem;
		display: flex;
		flex-direction: column;
	}

	/* Day line -------------------------------------------------------------- */
	.folio-head {
		display: flex;
		align-items: baseline;
		justify-content: space-between;
		gap: 1rem;
		padding-bottom: 1rem;
		margin-bottom: 2.5rem;
		border-bottom: 1px solid var(--color-border);
		font-family: var(--font-mono);
		font-size: 0.75rem;
		letter-spacing: 0.02em;
		color: var(--color-foreground-subtle);
	}

	.folio-today {
		display: inline-flex;
		align-items: center;
		gap: 0.25rem;
		background: transparent;
		border: none;
		padding: 0;
		cursor: pointer;
		font: inherit;
		color: var(--color-foreground-muted);
		transition: color var(--duration-fast) ease;
	}
	.folio-today:hover {
		color: var(--color-foreground);
	}

	/* Likeness ---------------------------------------------------------------- */
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

	/* Actions ----------------------------------------------------------------- */
	.home-actions {
		display: flex;
		flex-direction: column;
		gap: 1.75rem;
		margin-top: 2.5rem;
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

	/* Composer dock ------------------------------------------------------------ */
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
		max-width: 40rem;
		opacity: 0.85;
		transition: opacity var(--duration-normal) ease;
	}
	.composer-inner:focus-within {
		opacity: 1;
	}
</style>
