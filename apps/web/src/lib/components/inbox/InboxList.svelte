<script lang="ts">
	/**
	 * The Inbox as a plain list. A row is one line: what it is at the start,
	 * the title, the thread it sits in beside the title, and when it is for
	 * at the end. A task of yours has its check, so it can be finished from
	 * the list. Anything else wears its mark. A heading folds its section,
	 * and the section eases shut, as does a question's panel.
	 *
	 * The page's own list (`inbox`) reads in the order `listSections` sets:
	 * Today, Questions, Insights, This week, Later, then Your server. A
	 * question is answered beside the list, or under its row in a narrow
	 * pane, where the place is on a map to answer from. Anywhere else the
	 * list is only the rings.
	 */
	import type { Snippet } from 'svelte';
	import { listSections, type InboxItem, type ListSection, type Part } from '$lib/inbox/items';
	import type { ProjectSummary } from '$lib/api/client';
	import Icon from '$lib/components/Icon.svelte';
	import ProjectGlyph from '$lib/components/ProjectGlyph.svelte';
	import TextAction from '$lib/components/TextAction.svelte';
	import TaskCheck from '$lib/components/tasks/TaskCheck.svelte';
	import { RINGS } from '$lib/inbox/items';
	import { toggleTaskDone } from '$lib/tasks/actions';
	import { motionMs } from '$lib/utils/motion';
	import { slide, type TransitionConfig } from '$lib/utils/transitions';
	import InboxMark from './InboxMark.svelte';

	let {
		items,
		questions,
		needs,
		picked = null,
		inbox = false,
		threadOf,
		onPick,
		under,
	}: {
		items: InboxItem[];
		/** What your server asks you, for the section after Today. */
		questions?: Part;
		/** What your server needs from you, for the last section. */
		needs?: Part;
		picked?: string | null;
		/** Lay it out as the page's own list, in the order `listSections` sets. */
		inbox?: boolean;
		/** The thread a thing sits in, when it sits in one. */
		threadOf: (it: InboxItem) => ProjectSummary | null;
		onPick: (key: string) => void;
		/** Drawn under a chosen question, where the page has no room beside the list. */
		under?: Snippet<[InboxItem]>;
	} = $props();

	/** A short part's rows ease in and out one at a time. */
	const sections = $derived<ListSection[]>(
		inbox ? listSections(items, questions, needs) : RINGS.map((name, ring) => ({ name, rows: items.filter((it) => it.ring === ring) })),
	);

	/** The sections you folded. */
	let folded = $state<Record<string, boolean>>({});

	/**
	 * A row easing in or out. Only the short parts do it: an answered
	 * question leaves and "Show the rest" brings rows in. The day groups
	 * change by the dozen when the signal control moves, and stay instant.
	 */
	const rowEase = (node: Element, on: boolean | undefined): TransitionConfig => (on ? slide(node) : { duration: 0 });
	/** A question's panel is tall, so it opens at the pace of a pane. */
	const panelEase = (node: Element): TransitionConfig => slide(node, { duration: motionMs('--motion-layout', 300) });
</script>

{#each sections as s (s.name)}
	{#if s.rows.length > 0}
		{@const open = !folded[s.name]}
		<section aria-label={s.name}>
			<h3 class="group">
				<button type="button" class="fold" aria-expanded={open} onclick={() => (folded[s.name] = open)}>
					{s.name}
					<Icon icon="mynaui:chevron-down" width="16" />
				</button>
			</h3>
			{#if open}
				<div class="rows" transition:slide>
					<ul>
						{#each s.rows as it (it.key)}
							{@const thread = threadOf(it)}
							{@const mine = it.kind === 'task' && it.task}
							{@const openUnder = !!under && !!it.question && it.key === picked}
							<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_noninteractive_element_interactions -->
							<li class="row" class:on={it.key === picked} data-inbox-key={it.key} onclick={() => onPick(it.key)} transition:rowEase={s.short}>
								<!-- svelte-ignore a11y_no_static_element_interactions -->
								<span class="mk" onclick={(e) => mine && e.stopPropagation()}>
									{#if mine}
										<TaskCheck done={false} label={`Complete ${it.title}`} glyph onclick={() => void toggleTaskDone(mine)} />
									{:else}
										<InboxMark kind={it.kind} glyph={it.glyph} />
									{/if}
								</span>
								<button type="button" class="title" aria-pressed={it.key === picked}>{it.title}</button>
								{#if thread}
									<span class="home">
										<ProjectGlyph project={thread} size={14} tinted />
										<span class="home-name">{thread.name || 'Untitled thread'}</span>
									</span>
								{:else if it.question}
									<span class="home"><span class="home-name">{it.question.about}</span></span>
								{/if}
								{#if !it.question && !it.attention}
									<span class="when">{it.meta}</span>
								{/if}
							</li>
							{#if openUnder && under}
								<li class="under" transition:panelEase>{@render under(it)}</li>
							{/if}
						{/each}
					</ul>
					{#if s.more}
						<p class="more"><TextAction quiet onclick={() => s.onMore?.()}>Show the rest</TextAction></p>
					{/if}
				</div>
			{/if}
		</section>
	{/if}
{/each}

<style>
	section {
		margin-bottom: 24px;
	}
	/* A section's heading is the serif the app's other headings use, a step
	   above the rows under it. */
	.group {
		margin: 0 0 4px;
		font-family: var(--font-serif);
		font-size: var(--text-lead);
		font-weight: 400;
		line-height: 24px;
	}
	/* A section's name folds it. The arrow shows only when it is pointed at
	   or folded, and turns to point at the folded rows. */
	.fold {
		display: inline-flex;
		align-items: center;
		gap: 4px;
		padding: 0;
		border: 0;
		background: none;
		font: inherit;
		color: var(--color-foreground);
		cursor: pointer;
	}
	.fold :global(svg) {
		opacity: 0;
		color: var(--color-foreground-muted);
		transition:
			opacity var(--motion-hover-out) var(--ease-premium),
			transform var(--motion-state) var(--ease-premium);
	}
	.fold:hover :global(svg),
	.fold[aria-expanded='false'] :global(svg) {
		opacity: 1;
	}
	.fold[aria-expanded='false'] :global(svg) {
		transform: rotate(-90deg);
	}
	/* The fold clips what it closes, so it is as wide as a row's fill. */
	.rows {
		margin: 0 -12px;
		padding: 0 12px;
	}
	ul {
		margin: 0;
		padding: 0;
		list-style: none;
	}
	/* One line: what it is, the title, its thread beside the title, and when
	   it is for at the far end. */
	.row {
		display: flex;
		align-items: flex-start;
		gap: 12px;
		min-height: 36px;
		margin: 0 -12px;
		padding: 8px 12px;
		box-sizing: border-box;
		border-radius: var(--radius-md);
		cursor: pointer;
		transition: background-color var(--motion-hover-out) var(--ease-premium);
	}
	.row:hover {
		background: var(--hover-bg);
		transition-duration: var(--motion-hover-in);
	}
	.row.on {
		background: var(--active-bg);
	}
	.mk {
		display: grid;
		flex: none;
		place-items: center;
		width: 20px;
		height: 20px;
	}
	.title {
		flex: 0 1 auto;
		min-width: 0;
		padding: 0;
		border: 0;
		background: none;
		overflow: hidden;
		font: inherit;
		font-size: var(--text-label);
		line-height: 20px;
		text-align: left;
		text-overflow: ellipsis;
		white-space: nowrap;
		color: var(--color-foreground);
		cursor: pointer;
	}
	/* In a narrow column the thread's name gives up its width before the
	   title does, down to its glyph, which still says which thread it is. */
	.home {
		display: flex;
		flex: 0 1000 auto;
		align-items: center;
		gap: 8px;
		min-width: 14px;
		height: 20px;
		overflow: hidden;
	}
	.home :global(svg) {
		flex: none;
	}
	.home-name {
		min-width: 0;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
		font-family: var(--font-serif-ui);
		font-size: var(--text-small);
		color: var(--color-foreground-muted);
	}
	.under {
		padding: 8px 0 16px 32px;
	}
	.more {
		margin: 4px 0 0 32px;
	}
	.when {
		flex: none;
		margin-left: auto;
		font-size: var(--text-caption);
		line-height: 20px;
		white-space: nowrap;
		color: var(--color-foreground-muted);
		font-variant-numeric: tabular-nums;
	}

	@media (prefers-reduced-motion: reduce) {
		.row,
		.fold :global(svg) {
			transition: none;
		}
	}
</style>
