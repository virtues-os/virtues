<script lang="ts">
	/**
	 * One insight, whole: on its own page (InsightView) and in the Threads
	 * door's Insights view beside the list. What it is about, the insight as
	 * its heading, when it leaves, and its action. "Why your server raised it" says the rule in
	 * one line and draws the evidence: for a clash, the two calendar entries
	 * on that afternoon's timeline with the overlap shaded; for a task with
	 * no time set aside, the task and the days left before it. Then what
	 * acting on it would change, and its history. The side column says what
	 * it is about, what kind of insight it is, when it was raised and leaves,
	 * how it was found, and holds "Fewer like this".
	 *
	 * `layout="page"` sits in the page frame; `"pane"` fits the column beside
	 * a list, with the side column after the insight.
	 */
	import type { NoteDetail, NoteEvent } from '$lib/api/client';
	import { ApiError, getNote, restoreNote, setNoteMuted } from '$lib/api/client';
	import { formatRelativeTimestamp } from '$lib/utils/dateUtils';
	import { toast } from 'svelte-sonner';
	import DetailFrame from '$lib/components/DetailFrame.svelte';
	import Icon from '$lib/components/Icon.svelte';
	import ProjectGlyph from '$lib/components/ProjectGlyph.svelte';
	import Switch from '$lib/components/settings/Switch.svelte';
	import { Button, TextAction } from '$lib';
	import { notesStore } from '$lib/stores/notes.svelte';
	import { skills } from '$lib/stores/skills.svelte';
	import { askVirtues } from '$lib/stores/pendingPrompt.svelte';
	import { windowShellStore } from '$lib/stores/window-shell.svelte';
	import { projectColor } from '$lib/sidebar/pin-colors';

	let {
		key,
		layout = 'page',
		current = $bindable(null),
	}: {
		key: string | null;
		layout?: 'page' | 'pane';
		/** The insight as shown, for a host that names its tab after it. */
		current?: NoteDetail | null;
	} = $props();

	let detail = $state<NoteDetail | null>(null);
	let gone = $state<string | null>(null);
	/** The server did not answer, as against an insight that has gone. */
	let failed = $state(false);
	$effect(() => {
		current = detail;
	});

	/** Beside a list the insight changes as you move down it, so a late answer for the last one is dropped. */
	async function load(k: string) {
		gone = null;
		failed = false;
		try {
			const read = await getNote(k);
			if (k === key) detail = read;
		} catch (e) {
			if (k !== key) return;
			detail = null;
			// A 404 is an insight that has gone, and the server says why.
			// Anything else is the server not answering.
			failed = !(e instanceof ApiError && e.status === 404);
			gone = failed
				? "Your server couldn't load this insight. Check that it's running, then try again."
				: 'This insight has gone. What it was about changed, or its moment passed.';
		}
	}

	// Reads again after the list reloads, so an Undo in a toast or another
	// view shows here too.
	$effect(() => {
		void notesStore.revision;
		if (key) void load(key);
	});

	/**
	 * Start a chat that runs the insight report skill on this insight, filed
	 * in its thread, so the report lands beside the thread's other pages. The
	 * skill list is loaded first, so the new chat reads `/insight-report` as
	 * the skill.
	 */
	async function writeUp() {
		// The insight on show now: beside a list, it may change while the skills load.
		const d = detail;
		if (!d) return;
		await skills.load();
		if (!skills.names.has('insight-report')) {
			toast.error("Your server can't write reports yet", {
				description: 'Update your server to get the insight report skill, then try again.',
			});
			return;
		}
		const thread = d.subjects.find((s) => s.startsWith('/thread/'))?.slice('/thread/'.length) ?? null;
		askVirtues(`/insight-report Write up this insight as a report: "${d.text}" (/insight/${encodeURIComponent(d.key)})`, thread);
	}

	function run() {
		if (detail) windowShellStore.openTabFromRoute(detail.action_route, { focusExisting: true });
	}

	async function bringBack() {
		if (!detail) return;
		try {
			await restoreNote(detail.key);
			await Promise.all([load(detail.key), notesStore.load()]);
		} catch (e) {
			toast.error("Your server couldn't bring that back", { description: 'Try again' });
		}
	}

	/** Says what changed and offers it back, since the choice outlives this insight. */
	async function toggleMuted(muted: boolean, quiet = false) {
		if (!detail) return;
		const k = detail.key;
		try {
			await setNoteMuted(k, muted);
			await Promise.all([load(k), notesStore.load()]);
		} catch (e) {
			toast.error("Your server couldn't change that", { description: 'Try again' });
			await load(k);
			return;
		}
		if (quiet) return;
		toast(muted ? 'Your server will raise fewer like this' : 'Your server will raise these again', {
			action: { label: 'Undo', onClick: () => void toggleMuted(!muted, true) },
		});
	}

	const HISTORY_LINE: Record<NoteDetail['history'][number]['kind'], string> = {
		raised: 'Your server raised it',
		set_aside: 'You set it aside',
		brought_back: 'You brought it back',
	};

	/** "Yesterday 4:12 PM", or the time alone for today. */
	function moment(iso: string): string {
		const day = formatRelativeTimestamp(iso);
		return day.includes(':') ? `Today ${day}` : `${day} ${clock(iso)}`;
	}

	async function notNow() {
		if (!detail) return;
		await notesStore.dismiss(detail);
		if (key) await load(key);
	}

	/**
	 * Times are shown in the home time zone the server wrote the sentence in,
	 * so the page agrees with its own sentence on a trip.
	 */
	function zone(): { timeZone?: string } {
		return detail?.time_zone ? { timeZone: detail.time_zone } : {};
	}

	function clock(iso: string | number): string {
		return new Date(iso).toLocaleTimeString('en-US', { hour: 'numeric', minute: '2-digit', ...zone() });
	}

	function dayAndTime(iso: string): string {
		const d = new Date(iso);
		return `${d.toLocaleDateString('en-US', { weekday: 'long', ...zone() })} ${clock(iso)}`;
	}

	/** When an insight leaves. Midnight at home reads as the end of the day before. */
	function leaves(iso: string): string {
		const d = new Date(iso);
		const parts = new Intl.DateTimeFormat('en-US', { hour: 'numeric', minute: 'numeric', hourCycle: 'h23', ...zone() }).formatToParts(d);
		const hour = Number(parts.find((p) => p.type === 'hour')?.value);
		const minute = Number(parts.find((p) => p.type === 'minute')?.value);
		if (hour === 0 && minute === 0) {
			const before = new Date(d.getTime() - 60_000);
			return `the end of ${before.toLocaleDateString('en-US', { weekday: 'long', ...zone() })}`;
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
			ticks.push({ x: pct(t), label: new Date(t).toLocaleTimeString('en-US', { hour: 'numeric', ...zone() }) });
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

{#if gone}
	<div class="state">
		<p>{gone}</p>
		{#if failed && key}
			{@const k = key}
			<TextAction onclick={() => load(k)}>Try again</TextAction>
		{:else if layout === 'page'}
			<TextAction onclick={() => windowShellStore.openTabFromRoute('/threads?view=insights', { focusExisting: true })}>
				See the other insights
			</TextAction>
		{/if}
	</div>
{:else if !detail || detail.key !== key}
	<div class="state"><Icon icon="mynaui:spinner" width="18" class="spin" /> Loading…</div>
{:else if layout === 'page'}
	<DetailFrame>{@render body(detail)}</DetailFrame>
{:else}
	<div class="pane-grid">{@render body(detail)}</div>
{/if}

{#snippet body(detail: NoteDetail)}
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
					<Button variant="secondary" onclick={run}>{detail.action}</Button>
					<Button variant="ghost" icon="mynaui:file-text" onclick={writeUp} tip="The assistant writes it up as a report on a page">
						Write this up
					</Button>
					{#if detail.is_dismissed}
						<TextAction onclick={bringBack}>Bring it back</TextAction>
					{:else}
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

				<section class="block">
					<h2 class="block-title">{detail.remedy_title}</h2>
					<p class="why">{detail.remedy}</p>
				</section>

				{#if detail.history.length}
					<section class="block">
						<h2 class="block-title">History</h2>
						<ol class="history">
							{#each detail.history as h, i (i)}
								<li>
									<span class="dot" aria-hidden="true"></span>
									<span>{HISTORY_LINE[h.kind]}</span>
									<span class="when">{moment(h.occurred_at)}</span>
								</li>
							{/each}
						</ol>
					</section>
				{/if}
			</main>

			<aside class="side" aria-label="About this insight">
				<div class="psec">Properties</div>
				<dl class="props">
					<dt>About</dt>
					<dd>
						{#each threads as t (t.id)}
							<button class="thread-link" onclick={() => windowShellStore.openTabFromRoute(`/thread/${t.id}`, { label: t.name, focusExisting: true })}>
								<ProjectGlyph project={t} size={16} tinted />
								<span>{t.name}</span>
							</button>
						{/each}
					</dd>
					<dt>Kind</dt>
					<dd>{detail.kind_label}</dd>
					{#if detail.raised_at}
						<dt>Raised</dt>
						<dd>{moment(detail.raised_at)}</dd>
					{/if}
					<dt>Leaves</dt>
					<dd>{leaves(detail.until).replace(/^the e/, 'E')}</dd>
					<dt>Found by</dt>
					<dd class="found">{detail.found_by}</dd>
				</dl>

				<div class="psec fewer">Fewer like this</div>
				<label class="mute">
					<Switch checked={detail.is_muted} label={detail.mute_label} onchange={(v) => toggleMuted(v)} />
					<span>{detail.mute_label}</span>
				</label>
			</aside>
{/snippet}

<style>
	/* Beside a list there is no room for a side column, so it follows the
	   insight. */
	.pane-grid {
		display: flex;
		flex-direction: column;
		gap: 40px;
		padding: 24px 40px 64px;
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
		font-size: var(--text-label);
		line-height: 24px;
	}
	.about-line {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: 8px;
		margin: 0;
		font-size: var(--text-caption);
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
		font-size: var(--text-label);
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
		font-size: var(--text-caption);
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
		font-size: var(--text-lead);
		line-height: 28px;
		color: var(--color-foreground);
	}
	.why {
		margin: 4px 0 0;
		font-size: var(--text-label);
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
		font-size: var(--text-caption);
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
		font-size: var(--text-caption);
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
		font-size: var(--text-label);
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
		flex: none;
		max-width: 60%;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.event-thread {
		flex: 0 1 auto;
		min-width: 0;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
		font-family: var(--font-serif-ui);
		font-size: var(--text-label);
		color: var(--color-foreground-muted);
	}
	.event-when {
		flex: none;
		margin-left: auto;
		font-size: var(--text-caption);
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
	.history {
		list-style: none;
		margin: 0;
		padding: 0;
	}
	.history li {
		display: grid;
		grid-template-columns: 16px minmax(0, 1fr) auto;
		align-items: baseline;
		gap: 8px;
		padding: 4px 0;
		font-size: var(--text-label);
		line-height: 20px;
		color: var(--color-foreground);
	}
	.history .dot {
		justify-self: center;
		width: 6px;
		height: 6px;
		border-radius: 50%;
		background: var(--color-foreground-subtle);
		transform: translateY(-2px);
	}
	.history .when {
		font-size: var(--text-small);
		color: var(--color-foreground-muted);
		font-variant-numeric: tabular-nums;
		white-space: nowrap;
	}
	.psec.fewer {
		margin-top: 32px;
	}
	.mute {
		display: flex;
		align-items: flex-start;
		gap: 8px;
		font-size: var(--text-small);
		line-height: 20px;
		color: var(--color-foreground);
		cursor: pointer;
	}
	.mute :global(button) {
		flex: none;
	}
	.side {
		padding-top: 4px;
	}
	.psec {
		margin: 0 0 8px;
		font-size: var(--text-caption);
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
		font-size: var(--text-small);
		line-height: 20px;
		color: var(--color-foreground-muted);
	}
	.props dd {
		margin: 0;
		min-width: 0;
		font-size: var(--text-label);
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
		font-size: var(--text-body);
		line-height: 24px;
		color: var(--color-foreground);
		cursor: pointer;
	}
	.thread-link:hover {
		color: var(--color-primary);
	}
	@media (prefers-reduced-motion: reduce) {
		.event {
			transition: none;
		}
	}
</style>
