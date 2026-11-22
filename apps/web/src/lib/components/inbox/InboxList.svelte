<script lang="ts">
	/**
	 * The Inbox as a plain list. A row is one line: what it is at the start,
	 * the title, the thread it sits in beside the title, and when it is for
	 * at the end. A task of yours has its check, so it can be finished from
	 * the list. Anything else wears its mark. A heading folds its section.
	 *
	 * The page's own list (`inbox`) reads in three parts. What your server
	 * asks or needs comes first, and a question with a yes or no can be
	 * answered from its row. What your server noticed comes next. Then come
	 * the things in your threads, nearest first, under the name of each ring.
	 * Anywhere else the list is only the rings.
	 */
	import type { Snippet } from 'svelte';
	import type { ProjectSummary } from '$lib/api/client';
	import Icon from '$lib/components/Icon.svelte';
	import ProjectGlyph from '$lib/components/ProjectGlyph.svelte';
	import TextAction from '$lib/components/TextAction.svelte';
	import TaskCheck from '$lib/components/tasks/TaskCheck.svelte';
	import { RINGS, type InboxItem } from '$lib/inbox/items';
	import type { PlaceAnswer } from '$lib/inbox/questions';
	import { toggleTaskDone } from '$lib/tasks/actions';
	import InboxMark from './InboxMark.svelte';

	let {
		items,
		server = [],
		serverMore = 0,
		picked = null,
		inbox = false,
		threadOf,
		onPick,
		onReply,
		onServerMore,
		under,
	}: {
		items: InboxItem[];
		/** What your server asks or needs, for the first section. */
		server?: InboxItem[];
		/** How many more of those there are than `server` holds. */
		serverMore?: number;
		picked?: string | null;
		/** Lay it out as the page's own list: your server, insights, then the rings. */
		inbox?: boolean;
		/** The thread a thing sits in, when it sits in one. */
		threadOf: (it: InboxItem) => ProjectSummary | null;
		onPick: (key: string) => void;
		onReply?: (it: InboxItem, answer: PlaceAnswer) => void;
		onServerMore?: () => void;
		/** Drawn under a chosen question, where the page has no room beside the list. */
		under?: Snippet<[InboxItem]>;
	} = $props();

	const SERVER = 'Your server';
	const INSIGHTS = 'Insights';
	const sections = $derived([
		...(inbox
			? [
					{ name: SERVER, rows: server },
					{ name: INSIGHTS, rows: items.filter((it) => it.kind === 'insight') },
				]
			: []),
		...RINGS.map((name, ring) => ({ name, rows: items.filter((it) => it.ring === ring && !(inbox && it.kind === 'insight')) })),
	]);

	/** A question is answered from its row when a yes or a no settles it. */
	const quick = (it: InboxItem) => (it.question && !it.question.typed && it.question.replies.length <= 2 ? it.question.replies : []);

	/** The sections you folded. */
	let folded = $state<Record<string, boolean>>({});
</script>

{#each sections as s (s.name)}
	{#if s.rows.length > 0}
		{@const open = !folded[s.name]}
		<section aria-label={s.name}>
			<h3 class="group">
				<button type="button" class="fold" aria-expanded={open} onclick={() => (folded[s.name] = open)}>
					{s.name}
					<Icon icon={open ? 'mynaui:chevron-down' : 'mynaui:chevron-right'} width="14" />
				</button>
			</h3>
			{#if open}
				<ul>
					{#each s.rows as it (it.key)}
						{@const thread = threadOf(it)}
						{@const mine = it.kind === 'task' && it.task}
						<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_noninteractive_element_interactions -->
						<li class="row" class:on={it.key === picked} data-inbox-key={it.key} onclick={() => onPick(it.key)}>
							<!-- svelte-ignore a11y_no_static_element_interactions -->
							<span class="mk" onclick={(e) => mine && e.stopPropagation()}>
								{#if mine}
									<TaskCheck done={false} label={`Complete ${it.title}`} onclick={() => void toggleTaskDone(mine)} />
								{:else}
									<InboxMark kind={it.kind} />
								{/if}
							</span>
							<button type="button" class="title" class:sentence={it.kind === 'insight'} aria-pressed={it.key === picked}>{it.title}</button>
							{#if thread}
								<span class="home">
									<ProjectGlyph project={thread} size={14} tinted />
									<span class="home-name">{thread.name || 'Untitled thread'}</span>
								</span>
							{:else if it.question}
								<span class="home"><span class="home-name">{it.question.about}</span></span>
							{/if}
							{#if it.question}
								<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
								<span class="replies" onclick={(e) => e.stopPropagation()}>
									{#each quick(it) as r (r.label)}
										<TextAction quiet={r.label === 'No'} onclick={() => onReply?.(it, r.answer)}>{r.label}</TextAction>
									{:else}
										<TextAction onclick={() => onPick(it.key)}>Answer</TextAction>
									{/each}
								</span>
							{:else if !it.attention}
								<span class="when">{it.meta}</span>
							{/if}
						</li>
						{#if under && it.question && it.key === picked}
							<li class="under">{@render under(it)}</li>
						{/if}
					{/each}
				</ul>
				{#if s.name === SERVER && serverMore > 0}
					<p class="more"><TextAction quiet onclick={() => onServerMore?.()}>Show the rest</TextAction></p>
				{/if}
			{/if}
		</section>
	{/if}
{/each}

<style>
	section {
		margin-bottom: 16px;
	}
	.group {
		margin: 0 0 4px;
		font-size: var(--text-small);
		font-weight: 400;
		line-height: 16px;
	}
	/* A section's name folds it. The arrow shows only when it is pointed at or folded. */
	.fold {
		display: inline-flex;
		align-items: center;
		gap: 4px;
		padding: 0;
		border: 0;
		background: none;
		font: inherit;
		color: var(--color-foreground-muted);
		cursor: pointer;
	}
	.fold :global(svg) {
		opacity: 0;
		transition: opacity var(--motion-hover-out) var(--ease-premium);
	}
	.fold:hover :global(svg),
	.fold[aria-expanded='false'] :global(svg) {
		opacity: 1;
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
	/* An insight is a sentence, so it may run to a second line. */
	.title.sentence {
		display: -webkit-box;
		-webkit-line-clamp: 2;
		line-clamp: 2;
		-webkit-box-orient: vertical;
		white-space: normal;
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
	/* A question's answers sit where a task's day does. */
	.replies {
		display: flex;
		flex: none;
		align-items: center;
		align-self: center;
		gap: 12px;
		margin-left: auto;
		line-height: 20px;
	}
	.under {
		padding: 4px 0 12px 32px;
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
