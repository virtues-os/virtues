<!--
	ThingPage.svelte

	A thing's page: its name, the person's sentence about it, its article, and
	what they put under it. A thing goes on and cannot be finished, so the page
	has no state of its own; the projects under it end and the tasks under it
	get done.
-->

<script lang="ts">
	import { onMount } from "svelte";
	import Markdown from "$lib/components/Markdown.svelte";
	import TextAction from "$lib/components/TextAction.svelte";
	import TaskList from "$lib/components/tasks/TaskList.svelte";
	import NotesRail from "./NotesRail.svelte";
	import { getArticle, getThing, startThingArticle, type ThingDetailApi } from "$lib/wiki/api";
	import { thingsStore } from "$lib/stores/things.svelte";
	import { projectStore } from "$lib/stores/project.svelte";
	import { tasksStore } from "$lib/stores/tasks.svelte";
	import { windowShellStore } from "$lib/stores/window-shell.svelte";
	import { usePageCrumbs } from "$lib/stores/paneActions.svelte";
	import { confirmAction } from "$lib/stores/dialog.svelte";
	import { wikiCrumb } from "$lib/tabs/crumbs";
	import { newProject, openProject, outcomeLabel } from "$lib/utils/projectActions";
	import { toast } from "svelte-sonner";

	interface Props {
		page: ThingDetailApi;
		/** Whether its tab is the one showing, so its task list loads and takes keys. */
		active?: boolean;
		/** The thing was renamed, so the tab can follow. */
		onRenamed?: (title: string) => void;
	}

	let { page, active = true, onRenamed }: Props = $props();

	/** A newer copy after an edit made here, so a rename or a new article shows without a reload. */
	let edited = $state<ThingDetailApi | null>(null);
	const thing = $derived(edited && edited.id === page.id ? edited : page);

	// The path: Wiki / Things / this one.
	// TODO(2026-11-16): draw the path with WikiCrumbs once it takes a path.
	// Until then a phone shows no breadcrumb on this page.
	usePageCrumbs(() => ({
		glyph: "wiki",
		crumbs: [wikiCrumb(), { label: "Things", route: "/wiki/things" }, { label: thing.title }],
	}));

	// ── name and sentence ───────────────────────────────────────────────────

	let titleDraft = $state("");
	let summaryDraft = $state("");
	let titleFocused = $state(false);
	let summaryFocused = $state(false);
	$effect(() => {
		if (!titleFocused) titleDraft = thing.title;
		if (!summaryFocused) summaryDraft = thing.summary ?? "";
	});

	async function save(fields: { title?: string; summary?: string }, failure: string) {
		try {
			const saved = await thingsStore.update(thing.id, fields);
			edited = { ...thing, ...saved };
			if (fields.title) onRenamed?.(saved.title);
		} catch (e) {
			toast.error(failure, { description: e instanceof Error ? e.message : undefined });
		}
	}

	async function commitTitle() {
		titleFocused = false;
		const title = titleDraft.trim();
		if (!title || title === thing.title) {
			titleDraft = thing.title;
			return;
		}
		await save({ title }, "Your server couldn't rename that");
	}

	async function commitSummary() {
		summaryFocused = false;
		const summary = summaryDraft.trim();
		if (summary === (thing.summary ?? "")) return;
		await save({ summary }, "Your server couldn't save that sentence");
	}

	const span = $derived.by(() => {
		const year = (d: string | null) => (d ? d.slice(0, 4) : null);
		const from = year(thing.started_at);
		const to = year(thing.ended_at);
		if (from && to) return from === to ? from : `${from}–${to}`;
		if (from) return `Since ${from}`;
		return to ? `Until ${to}` : null;
	});

	// ── its article ─────────────────────────────────────────────────────────

	let starting = $state(false);

	async function reload() {
		const fresh = await getThing(thing.id);
		if (fresh) {
			edited = fresh;
			thingsStore.put(fresh);
		}
	}

	async function openArticle() {
		const article = await getArticle("thing", thing.id);
		if (article?.page_id) windowShellStore.openTabFromRoute(`/page/${article.page_id}`);
		else toast.error("Your server couldn't open that page", { description: "Try again" });
	}

	/**
	 * The article says no more than their own sentence, which the header
	 * already shows, so the page does not print it twice.
	 */
	const seedOnly = $derived(
		!!thing.article && [thing.summary?.trim(), "Nothing written yet."].includes(thing.article.trim()),
	);

	/** Seed the article with their sentence. The wiki editor fills it in when it is on. */
	async function startArticle() {
		starting = true;
		try {
			await startThingArticle(thing.id);
			await reload();
		} catch (e) {
			toast.error("Your server couldn't start that page", {
				description: e instanceof Error ? e.message : undefined,
			});
		} finally {
			starting = false;
		}
	}

	// ── what sits under it ──────────────────────────────────────────────────

	onMount(() => {
		if (!projectStore.loaded) void projectStore.load();
	});

	const openProjects = $derived(projectStore.projects.filter((p) => p.thing_id === thing.id));
	const endedProjects = $derived(projectStore.archived.filter((p) => p.thing_id === thing.id));

	function endedOn(iso: string | null): string {
		if (!iso) return "";
		return new Date(iso).toLocaleDateString("en-US", { month: "short", day: "numeric", year: "numeric" });
	}

	// ── removing it ─────────────────────────────────────────────────────────

	async function remove() {
		const ok = await confirmAction({
			title: `Remove "${thing.title}"?`,
			body: "Its page goes too. The projects and tasks under it stay.",
			confirmLabel: "Remove",
			danger: true,
		});
		if (!ok) return;
		try {
			await thingsStore.remove(thing.id);
			void projectStore.load();
			void tasksStore.refresh();
			windowShellStore.navigate("/wiki/things", { label: "Wiki" });
		} catch (e) {
			toast.error("Your server couldn't remove that", {
				description: e instanceof Error ? e.message : "It's still here. Try again",
			});
		}
	}
</script>

<div class="page-layout">
	<article class="wiki-article">
		<div class="page-content">
			<header class="page-header">
				<textarea
					class="title-input"
					bind:value={titleDraft}
					rows="1"
					aria-label="Name"
					onfocus={() => (titleFocused = true)}
					onblur={commitTitle}
					onkeydown={(e) => {
						if (e.key === "Enter") {
							e.preventDefault();
							e.currentTarget.blur();
						}
						if (e.key === "Escape") {
							titleDraft = thing.title;
							e.currentTarget.blur();
						}
					}}
				></textarea>
				<textarea
					class="summary-input"
					bind:value={summaryDraft}
					rows="1"
					aria-label="A sentence about it"
					placeholder="A sentence about it, in your own words."
					onfocus={() => (summaryFocused = true)}
					onblur={commitSummary}
					onkeydown={(e) => {
						if (e.key === "Enter") {
							e.preventDefault();
							e.currentTarget.blur();
						}
						if (e.key === "Escape") {
							summaryDraft = thing.summary ?? "";
							e.currentTarget.blur();
						}
					}}
				></textarea>
				{#if span}
					<div class="page-meta">{span}</div>
				{/if}
			</header>

			<hr class="divider" />

			<section class="section" id="article">
				{#if thing.article && !seedOnly}
					<Markdown content={thing.article} refVariant="quiet" variant="article" />
					<p class="aside">
						<TextAction inline onclick={openArticle}>Edit its page</TextAction>
					</p>
				{:else if thing.has_article}
					<p class="aside">
						Its page has your sentence so far. With the Wiki Editor applet on, your server
						searches your data and adds what it finds.
						<TextAction inline onclick={openArticle}>Open its page</TextAction>
					</p>
				{:else}
					<p class="stub">
						No page yet.
						<TextAction inline loading={starting} loadingLabel="Starting…" onclick={startArticle}>
							Start its page
						</TextAction>
					</p>
					<p class="aside">
						It starts with your sentence. With the Wiki Editor applet on, your server searches
						your data and adds what it finds.
					</p>
				{/if}
			</section>

			<section class="section" id="projects" aria-labelledby="thing-projects">
				<h2 class="section-title" id="thing-projects">Projects</h2>
				{#if openProjects.length === 0 && endedProjects.length === 0}
					<p class="aside">No projects under {thing.title} yet.</p>
				{:else}
					<ul class="rows">
						{#each openProjects as p (p.id)}
							<li class="row">
								<button type="button" class="row-title" onclick={() => openProject(p)}>{p.name}</button>
							</li>
						{/each}
						{#each endedProjects as p (p.id)}
							<li class="row ended">
								<button type="button" class="row-title" onclick={() => openProject(p)}>{p.name}</button>
								<span class="row-facts">{outcomeLabel(p)} {endedOn(p.archived_at)}</span>
							</li>
						{/each}
					</ul>
				{/if}
				<div class="add">
					<TextAction onclick={() => newProject(undefined, { thing_id: thing.id })}>New project</TextAction>
				</div>
			</section>

			<section class="section" id="tasks" aria-labelledby="thing-tasks">
				<h2 class="section-title" id="thing-tasks">Tasks</h2>
				<TaskList {active} route={`/thing/${thing.id}`} thingId={thing.id} emptyText={null} />
			</section>

			<NotesRail subjectType="thing" subjectId={thing.id} />

			<p class="remove">
				<TextAction quiet onclick={remove}>Remove this thing</TextAction>
			</p>
		</div>
	</article>
</div>

<style>
	.page-layout {
		display: flex;
		height: 100%;
		width: 100%;
		overflow: hidden;
	}

	.wiki-article {
		flex: 1;
		min-width: 0;
		overflow-y: auto;
		scrollbar-width: none;
		-ms-overflow-style: none;
		padding: 0.5rem 2rem 2rem;
	}

	.wiki-article::-webkit-scrollbar {
		display: none;
	}

	.page-content {
		max-width: 48rem;
		margin: 0 auto;
		padding-top: 2rem;
		padding-bottom: 4rem;
	}

	.page-header {
		margin-bottom: 1rem;
	}

	.title-input,
	.summary-input {
		display: block;
		width: 100%;
		resize: none;
		overflow: hidden;
		border: none;
		background: transparent;
		outline: none;
		padding: 0;
		field-sizing: content;
	}

	.title-input {
		font-family: var(--font-serif);
		font-size: 1.75rem;
		font-weight: 400;
		line-height: 1.3;
		color: var(--color-foreground);
	}

	.summary-input {
		margin-top: 0.25rem;
		min-height: 1.5rem;
		font-family: var(--font-serif);
		font-size: 1.0625rem;
		line-height: 1.5;
		color: var(--color-foreground-muted);
	}

	.summary-input:focus {
		color: var(--color-foreground);
	}

	.title-input::placeholder,
	.summary-input::placeholder {
		color: var(--color-foreground-subtle);
	}

	.page-meta {
		margin-top: 0.5rem;
		font-size: 0.875rem;
		color: var(--color-foreground-subtle);
		font-variant-numeric: tabular-nums;
	}

	.divider {
		border: none;
		border-top: 1px solid var(--color-border);
		margin: 1rem 0 1.5rem;
	}

	.section {
		margin-bottom: 2rem;
	}

	.section-title {
		font-family: var(--font-serif);
		font-size: 1.375rem;
		font-weight: 400;
		line-height: 1.35;
		color: var(--color-foreground);
		margin: 0 0 0.5rem;
	}

	.stub {
		margin: 0;
		font-family: var(--font-serif);
		font-size: 0.9375rem;
		color: var(--color-foreground-subtle);
	}

	.aside {
		margin: 0.5rem 0 0;
		font-size: 0.8125rem;
		line-height: 1.5;
		color: var(--color-foreground-subtle);
	}

	.rows {
		list-style: none;
		margin: 0;
		padding: 0;
	}

	.row {
		display: flex;
		align-items: center;
		gap: 0.75rem;
		min-height: 2.5rem;
		border-bottom: 1px solid var(--color-border);
	}

	.row-title {
		flex: 1;
		min-width: 0;
		background: none;
		border: none;
		padding: 0.25rem 0;
		text-align: left;
		font: inherit;
		font-size: 0.9375rem;
		line-height: 1.4;
		color: var(--color-foreground);
		cursor: pointer;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.row-title:hover {
		text-decoration: underline;
		text-underline-offset: 3px;
	}

	.row.ended .row-title {
		color: var(--color-foreground-muted);
	}

	.row-facts {
		flex-shrink: 0;
		font-size: 0.8125rem;
		color: var(--color-foreground-muted);
		font-variant-numeric: tabular-nums;
	}

	.add {
		margin-top: 0.75rem;
	}

	.remove {
		margin: 3rem 0 0;
	}

	@media (max-width: 900px) {
		.wiki-article {
			padding: 1rem;
		}

		.title-input {
			font-size: 1.5rem;
		}
	}
</style>
