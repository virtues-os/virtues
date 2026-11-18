<script lang="ts">
	/**
	 * One note's page: what it is about, the note as its heading, when it
	 * leaves, and its action. "Why your server raised it" says the rule in
	 * one line and draws the evidence: for a clash, the two calendar entries
	 * on that afternoon's timeline with the overlap shaded; for a task with
	 * no time set aside, the task and the days left before it. The side
	 * column says what it is about, what kind of note it is, when it leaves
	 * and how it was found.
	 */
	import type { Tab } from '$lib/tabs/types';
	import type { NoteDetail, NoteEvent } from '$lib/api/client';
	import { getNote } from '$lib/api/client';
	import Icon from '$lib/components/Icon.svelte';
	import ProjectGlyph from '$lib/components/ProjectGlyph.svelte';
	import { Button, TextAction } from '$lib';
	import { notesStore } from '$lib/stores/notes.svelte';
	import { windowShellStore } from '$lib/stores/window-shell.svelte';
	import { projectColor } from '$lib/sidebar/pin-colors';

	let { tab }: { tab: Tab; active?: boolean } = $props();

	const key = $derived.by(() => {
		const m = tab.route.match(/^\/insight\/([^?#]+)/);
		return m ? decodeURIComponent(m[1]) : null;
	});

	let detail = $state<NoteDetail | null>(null);
	let gone = $state<string | null>(null);

	async function load(k: string) {
		gone = null;
		try {
			detail = await getNote(k);
		} catch (e) {
			detail = null;
			const said = e instanceof Error ? e.message : '';
			// The server says when a note has gone; anything else is the server
			// not answering.
			gone = /has gone/.test(said)
				? said
				: "Your server couldn't load this note. Check that it's running, then try again.";
		}
	}

	$effect(() => {
		if (key) void load(key);
	});

	$effect(() => {
		if (detail && tab.label !== detail.text) windowShellStore.updateTab(tab.id, { label: detail.text });
	});

	function run() {
		if (detail) windowShellStore.openTabFromRoute(detail.action_route, { focusExisting: true });
	}

	async function notNow() {
		if (!detail) return;
		await notesStore.dismiss(detail);
		if (key) await load(key);
	}

	function clock(iso: string): string {
		return new Date(iso).toLocaleTimeString('en-US', { hour: 'numeric', minute: '2-digit' });
	}

	function dayAndTime(iso: string): string {
		const d = new Date(iso);
		return `${d.toLocaleDateString('en-US', { weekday: 'long' })} ${clock(iso)}`;
	}

	/** When a note leaves. Local midnight reads as the end of the day before. */
	function leaves(iso: string): string {
		const d = new Date(iso);
		if (d.getHours() === 0 && d.getMinutes() === 0) {
			const before = new Date(d.getTime() - 60_000);
			return `the end of ${before.toLocaleDateString('en-US', { weekday: 'long' })}`;
		}
		return dayAndTime(iso);
	}

	function span(e: NoteEvent): string {
		return `${dayAndTime(e.started_at)} to ${clock(e.ended_at)}`;
	}

	/**
	 * The afternoon the clash sits in: whole hours from an hour before the
	 * first entry starts to an hour after the last one ends, each entry a bar
	 * in its thread's color, the overlap shaded.
	 */
	const axis = $derived.by(() => {
		if (!detail || detail.events.length < 2) return null;
		const starts = detail.events.map((e) => new Date(e.started_at).getTime());
		const ends = detail.events.map((e) => new Date(e.ended_at).getTime());
		const hour = 3_600_000;
		const from = Math.floor(Math.min(...starts) / hour) * hour - hour;
		const to = Math.ceil(Math.max(...ends) / hour) * hour + hour;
		const pct = (t: number) => ((t - from) / (to - from)) * 100;
		const ticks: { x: number; label: string }[] = [];
		for (let t = from; t <= to; t += hour) {
			ticks.push({ x: pct(t), label: new Date(t).toLocaleTimeString('en-US', { hour: 'numeric' }) });
		}
		const overFrom = Math.max(...starts);
		const overTo = Math.min(...ends);
		return {
			ticks,
			bars: detail.events.map((e, i) => ({
				event: e,
				left: pct(starts[i]),
				width: pct(ends[i]) - pct(starts[i]),
				color: projectColor({ id: e.thread_id, accent_color: e.thread_color }),
			})),
			over: overTo > overFrom ? { left: pct(overFrom), width: pct(overTo) - pct(overFrom) } : null,
		};
	});

	/** The days from today to the task's day, today first. */
	const runway = $derived.by(() => {
		if (!detail?.task) return [];
		const out: { label: string; due: boolean }[] = [];
		const due = new Date(`${detail.task.due_on}T12:00:00`);
		const day = new Date();
		day.setHours(12, 0, 0, 0);
		for (let i = 0; i < 7 && day <= due; i++) {
			const isDue = day.toDateString() === due.toDateString();
			out.push({ label: i === 0 ? 'Today' : day.toLocaleDateString('en-US', { weekday: 'short' }), due: isDue });
			day.setDate(day.getDate() + 1);
		}
		return out;
	});

	const threads = $derived.by(() => {
		if (!detail) return [];
		if (detail.events.length) {
			const seen = new Map<string, NoteEvent>();
			for (const e of detail.events) if (!seen.has(e.thread_id)) seen.set(e.thread_id, e);
			return [...seen.values()].map((e) => ({ id: e.thread_id, name: e.thread_name, icon: e.thread_icon, accent_color: e.thread_color }));
		}
		return detail.task
			? [{ id: detail.task.thread_id, name: detail.task.thread_name, icon: null, accent_color: null }]
			: [];
	});
</script>

<div class="insight-page">
	{#if gone}
		<div class="state">
			<p>{gone}</p>
			<TextAction onclick={() => windowShellStore.openTabFromRoute('/threads?view=insights', { focusExisting: true })}>
				See the other insights
			</TextAction>
		</div>
	{:else if !detail}
		<div class="state"><Icon icon="mynaui:spinner" width="18" class="spin" /> Loading…</div>
	{:else}
		<div class="frame">
			<main class="main">
				<p class="about-line">
					<span class="mark" class:quiet={detail.is_dismissed} aria-hidden="true"></span>
					Insight about
					{#if detail.task}
						<span class="about-name">{detail.task.title}</span>
					{:else}
						{#each threads as t, i (t.id)}
							{#if i > 0}and{/if}
							<span class="about-thread"><ProjectGlyph project={t} size={14} /><span class="about-name">{t.name}</span></span>
						{/each}
					{/if}
				</p>
				<h1 class="headline">{detail.text}</h1>
				<p class="leaves">
					{#if detail.is_dismissed}
						Set aside. It leaves on its own after {leaves(detail.until)}.
					{:else}
						Matters until {leaves(detail.until)}. It leaves on its own after that.
					{/if}
				</p>
				<div class="acts">
					<Button variant="secondary" size="sm" onclick={run}>{detail.action}</Button>
					{#if !detail.is_dismissed}
						<TextAction onclick={notNow}>Not now</TextAction>
					{/if}
				</div>

				<section class="block">
					<h2 class="block-title">Why your server raised it</h2>
					<p class="why">{detail.why}</p>

					{#if axis}
						<div class="axis" role="img" aria-label={detail.events.map(span).join('; ')}>
							{#each axis.ticks as t (t.x)}
								<span class="gridline" style={`left: ${t.x}%`}></span>
								<span class="tick" style={`left: ${t.x}%`}>{t.label}</span>
							{/each}
							{#if axis.over}
								<span class="over" style={`left: ${axis.over.left}%; width: ${axis.over.width}%`}></span>
							{/if}
							{#each axis.bars as b, i (b.event.id)}
								<span
									class="bar"
									style={`left: ${b.left}%; width: ${b.width}%; top: ${32 + i * 40}px; --bar: ${b.color}`}
								>
									{b.event.title}
								</span>
							{/each}
						</div>
						<ul class="events">
							{#each detail.events as e (e.id)}
								<li>
									<button class="event" onclick={() => windowShellStore.openRouteBeside(`/record/calendar_event/${e.id}`)}>
										<Icon icon="atlas:calendar" width="16" />
										<span class="event-title">{e.title}</span>
										<span class="event-thread">{e.thread_name}</span>
										<span class="event-when">{span(e)}</span>
									</button>
								</li>
							{/each}
						</ul>
					{/if}

					{#if detail.task}
						<div class="runway" role="img" aria-label={`Due ${runway.at(-1)?.label ?? ''}, with nothing set aside before then`}>
							{#each runway as d, i (i)}
								<span class="day" class:due={d.due}>
									<span class="day-cell"></span>
									<span class="day-label">{d.label}</span>
								</span>
							{/each}
						</div>
						<ul class="events">
							<li>
								<button class="event" onclick={() => windowShellStore.openRouteBeside(`/task/${detail?.task?.id}`)}>
									<span class="ring" aria-hidden="true"></span>
									<span class="event-title">{detail.task.title}</span>
									<span class="event-thread">{detail.task.thread_name}</span>
									<span class="event-when">{detail.when}</span>
								</button>
							</li>
						</ul>
					{/if}
				</section>
			</main>

			<aside class="side" aria-label="About this insight">
				<div class="psec">Properties</div>
				<dl class="props">
					<dt>About</dt>
					<dd>
						{#each threads as t (t.id)}
							<button class="thread-link" onclick={() => windowShellStore.openTabFromRoute(`/thread/${t.id}`, { label: t.name, focusExisting: true })}>
								<ProjectGlyph project={t} size={16} />
								<span>{t.name}</span>
							</button>
						{/each}
					</dd>
					<dt>Kind</dt>
					<dd>{detail.kind_label}</dd>
					<dt>Leaves</dt>
					<dd>{leaves(detail.until).replace(/^the e/, 'E')}</dd>
					<dt>Found by</dt>
					<dd class="found">{detail.found_by}</dd>
				</dl>
			</aside>
		</div>
	{/if}
</div>

<style>
	.insight-page {
		width: 100%;
		height: 100%;
		overflow-y: auto;
	}
	.frame {
		display: grid;
		grid-template-columns: minmax(0, 1fr) 280px;
		gap: 48px;
		max-width: 1080px;
		margin: 0 auto;
		padding: 48px 32px 96px;
	}
	.main {
		min-width: 0;
	}
	.state {
		display: flex;
		flex-direction: column;
		align-items: flex-start;
		gap: 8px;
		padding: 48px 32px;
		color: var(--color-foreground-muted);
	}
	.state p {
		margin: 0;
		font-size: 15px;
		line-height: 24px;
	}
	.about-line {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: 8px;
		margin: 0;
		font-size: 12px;
		color: var(--color-foreground-muted);
	}
	.mark {
		width: 6px;
		height: 6px;
		border-radius: 50%;
		background: var(--color-primary);
	}
	.mark.quiet {
		background: var(--color-foreground-subtle);
	}
	.about-thread {
		display: inline-flex;
		align-items: center;
		gap: 4px;
	}
	.about-name {
		font-family: var(--font-serif-ui);
		font-size: 14px;
		color: var(--color-foreground);
	}
	.headline {
		margin: 12px 0 0;
		max-width: 680px;
		font-family: var(--font-serif);
		font-weight: 400;
		font-size: var(--md-h2-size);
		line-height: 32px;
		color: var(--color-foreground);
	}
	.leaves {
		margin: 8px 0 0;
		font-size: 12px;
		line-height: 16px;
		color: var(--color-foreground-muted);
	}
	.acts {
		display: flex;
		align-items: center;
		gap: 16px;
		margin-top: 16px;
	}
	.block {
		margin-top: 40px;
	}
	.block-title {
		margin: 0;
		font-family: var(--font-serif);
		font-weight: 400;
		font-size: 18px;
		line-height: 28px;
		color: var(--color-foreground);
	}
	.why {
		margin: 4px 0 0;
		font-size: 14px;
		line-height: 20px;
		color: var(--color-foreground);
	}
	.axis {
		position: relative;
		height: 124px;
		margin-top: 16px;
	}
	.gridline {
		position: absolute;
		top: 20px;
		bottom: 0;
		border-left: 1px solid var(--color-border);
	}
	.tick {
		position: absolute;
		top: 0;
		transform: translateX(-50%);
		font-size: 12px;
		color: var(--color-foreground-muted);
		font-variant-numeric: tabular-nums;
		white-space: nowrap;
	}
	.over {
		position: absolute;
		top: 24px;
		bottom: 0;
		background: color-mix(in srgb, var(--color-primary) 12%, transparent);
	}
	.bar {
		position: absolute;
		height: 28px;
		padding: 0 8px;
		box-sizing: border-box;
		border-radius: var(--radius-md);
		background: var(--bar);
		font-size: 12px;
		line-height: 28px;
		color: var(--color-background);
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.events {
		list-style: none;
		margin: 12px 0 0;
		padding: 0;
	}
	.event {
		display: flex;
		align-items: center;
		gap: 12px;
		width: 100%;
		min-height: 40px;
		padding: 0 12px;
		margin: 0 -12px;
		border: 0;
		border-radius: var(--radius-md);
		background: transparent;
		font: inherit;
		font-size: 14px;
		color: var(--color-foreground);
		text-align: left;
		cursor: pointer;
		transition: background-color var(--motion-hover-out) var(--ease-premium);
	}
	.event:hover {
		background: var(--hover-bg);
		transition-duration: var(--motion-hover-in);
	}
	.event :global(svg) {
		flex: none;
		color: var(--color-foreground-muted);
	}
	.event-title {
		min-width: 0;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.event-thread {
		flex: none;
		font-family: var(--font-serif-ui);
		font-size: 14px;
		color: var(--color-foreground-muted);
	}
	.event-when {
		flex: none;
		margin-left: auto;
		font-size: 12px;
		color: var(--color-foreground-muted);
		white-space: nowrap;
	}
	.ring {
		flex: none;
		width: 14px;
		height: 14px;
		border-radius: 50%;
		border: 1px solid var(--color-foreground-muted);
		box-sizing: border-box;
	}
	.runway {
		display: flex;
		gap: 8px;
		margin-top: 16px;
	}
	.day {
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: 4px;
	}
	.day-cell {
		width: 56px;
		height: 28px;
		border-radius: var(--radius-md);
		background: var(--hover-bg);
		box-sizing: border-box;
	}
	.day.due .day-cell {
		background: none;
		border: 1px solid var(--color-foreground-muted);
	}
	.day-label {
		font-size: 12px;
		color: var(--color-foreground-muted);
	}
	.day.due .day-label {
		color: var(--color-foreground);
	}
	.side {
		padding-top: 4px;
	}
	.psec {
		margin: 0 0 8px;
		font-size: 12px;
		line-height: 16px;
		color: var(--color-foreground-muted);
	}
	.props {
		display: grid;
		grid-template-columns: 72px minmax(0, 1fr);
		gap: 12px;
		margin: 0;
		align-items: baseline;
	}
	.props dt {
		font-size: 13px;
		line-height: 20px;
		color: var(--color-foreground-muted);
	}
	.props dd {
		margin: 0;
		min-width: 0;
		font-size: 14px;
		line-height: 20px;
		color: var(--color-foreground);
	}
	.found {
		color: var(--color-foreground-muted);
	}
	.thread-link {
		display: flex;
		align-items: center;
		gap: 8px;
		padding: 0;
		border: 0;
		background: none;
		font-family: var(--font-serif-ui);
		font-size: 16px;
		line-height: 24px;
		color: var(--color-foreground);
		cursor: pointer;
	}
	.thread-link:hover {
		color: var(--color-primary);
	}
	@media (max-width: 900px) {
		.frame {
			grid-template-columns: minmax(0, 1fr);
			padding: 24px 16px 64px;
		}
	}
	@media (prefers-reduced-motion: reduce) {
		.event {
			transition: none;
		}
	}
	:global(.spin) {
		animation: spin 0.8s linear infinite;
	}
	@keyframes spin {
		to {
			transform: rotate(360deg);
		}
	}
</style>
