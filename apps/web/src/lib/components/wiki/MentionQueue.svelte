<script lang="ts" module>
	import { getMentionQueue } from '$lib/api/client';
	/**
	 * A surface only *asks* for a decision once it recurs across separate records.
	 * A name said once is dust and should not demand attention; a name that keeps
	 * turning up is a fixture of your life. This is what lets the badge reach zero
	 * — badge everything and the count never clears, and a count that never clears
	 * is wallpaper inside a week.
	 */
	export const RECURRING = 3;

	/**
	 * The badge count, without mounting the queue — the nav needs it before the
	 * user ever opens the tab, which is the entire point of a badge.
	 */
	export async function fetchRecurringCount(): Promise<number> {
		try {
			const groups = await getMentionQueue<{ sources: number }[]>();
			return groups.filter((g) => g.sources >= RECURRING).length;
		} catch {
			return 0;
		}
	}
</script>

<script lang="ts">
	/**
	 * The mention review queue — the one place a name from prose becomes a person.
	 *
	 * Nothing in the pipeline links a spoken or written name by guessing. The
	 * resolver links only exact, unambiguous matches; everything else floats here
	 * and a human decides. A wrong link is a lie about someone's life and it is
	 * invisible — it looks exactly like a right one. An unresolved mention is
	 * merely dust: stored, searchable, costing nothing but a decision not yet made.
	 *
	 * Grouped by SURFACE, not by mention. Per-mention this never converges — a year
	 * of transcripts outruns anyone, and an inbox that can't be emptied gets
	 * abandoned. Per surface it's a short list, and each decision is permanent:
	 * linking "Sarah" writes an alias, backfills every past mention, and resolves
	 * every future one without asking again. One decision per name, once.
	 */
	import { onMount } from 'svelte';
	import Icon from '$lib/components/Icon.svelte';
	import { resolveMention } from '$lib/api/client';

	interface Candidate {
		entity_type: string;
		entity_id: string;
		name: string;
		reason: string;
	}

	interface SurfaceGroup {
		normalized: string;
		surface: string;
		mention_type: 'person' | 'place' | 'org';
		count: number;
		/** Distinct records it appears in — the recurrence signal. */
		sources: number;
		snippets: string[];
		first_seen: string | null;
		last_seen: string | null;
		candidates: Candidate[];
	}

	let groups = $state<SurfaceGroup[]>([]);
	let loading = $state(true);
	let error = $state<string | null>(null);
	let busy = $state<string | null>(null);
	let showAll = $state(false);

	const TYPE = {
		person: { icon: 'ri:user-line', label: 'Person' },
		place: { icon: 'ri:map-pin-line', label: 'Place' },
		org: { icon: 'ri:building-line', label: 'Organization' }
	} as const;

	/** Lets the wiki nav badge the tab without a second fetch. */
	let { oncount }: { oncount?: (n: number) => void } = $props();

	const recurring = $derived(groups.filter((g) => g.sources >= RECURRING));
	const once = $derived(groups.filter((g) => g.sources < RECURRING));
	const shown = $derived(showAll ? groups : recurring);

	// The badge counts RECURRING surfaces only — see RECURRING above.
	$effect(() => oncount?.(recurring.length));

	onMount(load);

	async function load() {
		loading = true;
		error = null;
		try {
			groups = await getMentionQueue<SurfaceGroup[]>();
		} catch (e) {
			error = e instanceof Error ? e.message : 'Failed to load the review queue';
		} finally {
			loading = false;
		}
	}

	function key(g: SurfaceGroup) {
		return `${g.mention_type}:${g.normalized}`;
	}

	async function act(g: SurfaceGroup, path: string, body: Record<string, unknown>) {
		busy = key(g);
		try {
			await resolveMention(path, {
				normalized: g.normalized,
				mention_type: g.mention_type,
				...body
			});
			// Drop it locally rather than refetching — the row is resolved either
			// way, and the whole list re-sorting under the cursor is disorienting
			// when you're working down a list.
			groups = groups.filter((x) => key(x) !== key(g));
		} catch (e) {
			error = e instanceof Error ? e.message : 'That did not save';
		} finally {
			busy = null;
		}
	}

	/** Link to an existing entity: writes the alias, backfills the history. */
	const link = (g: SurfaceGroup, c: Candidate) => act(g, 'link', { entity_id: c.entity_id });

	/** Mint the entity, then link it. Same decision, one step earlier. */
	const create = (g: SurfaceGroup) => act(g, 'create', { name: g.surface });

	/**
	 * It names nothing ("Unsubscribe", "Sent from my iPhone"). Never asked again.
	 * NOT a delete — the mentions stay searchable, and linking the surface later
	 * still picks them up.
	 */
	const dismiss = (g: SurfaceGroup) => act(g, 'dismiss', {});
</script>

<div class="flex flex-col gap-4">
	{#if loading}
		<div class="py-12 text-center text-sm text-foreground-muted">Loading…</div>
	{:else if error}
		<div class="py-4 px-3 rounded-md bg-error/10 text-sm text-error flex items-center justify-between">
			<span>{error}</span>
			<button class="underline" onclick={load}>Retry</button>
		</div>
	{/if}

	{#if !loading && groups.length === 0}
		<!-- The honest empty state. Nothing floating means every name we found was
		     one we could already place — not that nothing was found. -->
		<div class="py-16 text-center">
			<Icon icon="ri:check-line" class="w-6 h-6 mx-auto mb-3 text-foreground-subtle" />
			<p class="text-sm text-foreground">Nothing to review</p>
			<p class="text-xs text-foreground-subtle mt-1 max-w-sm mx-auto">
				Every name we found in your records matched someone or somewhere we
				already know. Names we can't place will collect here.
			</p>
		</div>
	{:else if !loading}
		{#if recurring.length === 0 && !showAll}
			<div class="py-10 text-center">
				<p class="text-sm text-foreground">Nothing worth your attention</p>
				<p class="text-xs text-foreground-subtle mt-1">
					{once.length}
					{once.length === 1 ? 'name was' : 'names were'} mentioned once and left as-is.
				</p>
			</div>
		{/if}

		{#each shown as g (key(g))}
			{@const t = TYPE[g.mention_type]}
			<div
				class="border border-border rounded-lg p-4 bg-surface {busy === key(g)
					? 'opacity-50 pointer-events-none'
					: ''}"
			>
				<div class="flex items-start justify-between gap-4">
					<div class="min-w-0">
						<div class="flex items-center gap-2">
							<Icon icon={t.icon} class="w-4 h-4 text-foreground-subtle shrink-0" />
							<span class="text-sm font-medium text-foreground truncate">{g.surface}</span>
							<span class="text-xs text-foreground-subtle shrink-0">
								{g.count}
								{g.count === 1 ? 'mention' : 'mentions'}
								{#if g.sources > 1}· {g.sources} records{/if}
							</span>
						</div>

						<!-- The quotations. THE reason this page is answerable: a bare
						     name can't be recognized, a sentence can. -->
						{#if g.snippets.length}
							<div class="mt-2 flex flex-col gap-1">
								{#each g.snippets as s}
									<p class="text-xs text-foreground-muted italic truncate">“{s}”</p>
								{/each}
							</div>
						{:else}
							<p class="mt-2 text-xs text-foreground-subtle">No surrounding text captured.</p>
						{/if}
					</div>

					<button
						class="text-xs text-foreground-subtle hover:text-foreground shrink-0"
						onclick={() => dismiss(g)}
						title="Names nothing. Won't be asked again; the mentions stay searchable."
					>
						Dismiss
					</button>
				</div>

				<div class="mt-3 flex flex-wrap items-center gap-2">
					{#each g.candidates as c}
						<button
							class="px-2.5 py-1 rounded-md border border-border text-xs text-foreground hover:border-border-strong hover:bg-background transition-colors"
							onclick={() => link(g, c)}
							title={c.reason}
						>
							{c.name}
						</button>
					{/each}

					<button
						class="px-2.5 py-1 rounded-md border border-dashed border-border text-xs text-foreground-muted hover:text-foreground hover:border-border-strong transition-colors"
						onclick={() => create(g)}
					>
						+ New {t.label.toLowerCase()}
					</button>
				</div>

				{#if g.candidates.some((c) => c.reason.startsWith('exact name'))}
					<!-- The three-Sarahs case, surfaced honestly rather than guessed at. -->
					<p class="mt-2 text-xs text-foreground-subtle">
						More than one match — we won't choose for you.
					</p>
				{/if}
			</div>
		{/each}

		{#if once.length > 0}
			<button
				class="self-start text-xs text-foreground-subtle hover:text-foreground"
				onclick={() => (showAll = !showAll)}
			>
				{showAll
					? 'Hide one-off names'
					: `Show ${once.length} name${once.length === 1 ? '' : 's'} mentioned only once`}
			</button>
		{/if}
	{/if}
</div>
