<script lang="ts">
	import type { Tab } from "$lib/tabs/types";
	import { Page, Textarea } from "$lib";
	import Icon from "$lib/components/Icon.svelte";
	import Modal from "$lib/components/Modal.svelte";
	import SubNav, { type SubNavItem } from "$lib/components/SubNav.svelte";
	import { slide } from "svelte/transition";
	import { onMount } from "svelte";

	let { tab, active }: { tab: Tab; active: boolean } = $props();

	// "You" — the self-model, read as the prudence triad. Three registers behind a
	// timeline-ordered tab bar: Past (a portrait — the record and its chapters),
	// Present (the checkpoint that grounds every chat — REAL, persisted, injected),
	// Future (who you're becoming). Everything but the checkpoint is HARDCODED to
	// the intended data shapes until the endpoints exist.

	type Reg = "past" | "present" | "future";
	// Register is derived from the route (per-pane, deep-linkable, restores) — SubNav
	// owns the writing side. Present is the default the bare route resolves to.
	const reg = $derived<Reg>(
		(tab.route.match(/^\/narrative-identity\/(past|present|future)$/)?.[1] as Reg) ?? "present"
	);
	const registers: SubNavItem[] = [
		{ id: "past", label: "Past" },
		{ id: "present", label: "Present" },
		{ id: "future", label: "Future" },
	];

	// ── PRESENT (real) ────────────────────────────────────────────────
	let loading = $state(true);
	let content = $state("");
	let updatedAt = $state<string | null>(null);
	let charCount = $derived(content.length);
	let showExamples = $state(false);
	let editing = $state(false);

	const MIN_CHARS = 100;
	const MAX_CHARS = 800;
	let tooShort = $derived(charCount > 0 && charCount < MIN_CHARS);
	let tooLong = $derived(charCount > MAX_CHARS);

	// ── PAST (hardcoded portrait data) ────────────────────────────────
	// Co-authored arc: the SYSTEM segments the timeline (changepoint markers from
	// regime shifts in the W5H context — where the novel became the new routine);
	// the USER authors the fortune line (Vonnegut good↔ill) and the chapter titles.
	// Fortune path: y small = good fortune (top). Wavy, with a real down-stretch.
	const FORTUNE_PATH =
		"M0,20 C7,24 13,33 21,34 C29,35 34,29 40,23 C48,15 55,12 63,15 C71,18 79,10 87,13 C93,15 97,11 100,9";
	// Named era boundaries (x%) + a dot's y on the line.
	const boundaries = [
		{ x: 40, y: 23 },
		{ x: 71, y: 18 },
	];
	const nowDot = { x: 100, y: 9 };
	// A freshly-detected shift the system hasn't been given a name for yet.
	const detected = { x: 88 };

	const chapters = [
		{ title: "The grind years", dates: "2019–2021", mid: 20, summary: "Heads-down, building, running on urgency." },
		{ title: "Slowing down on purpose", dates: "2021–2023", mid: 55, summary: "Chose wisdom over output; started to rest." },
		{ title: "Fatherhood & faith", dates: "2023–now", mid: 85, summary: "Two kids, a return to the parish, a softer pace." },
	];

	const people = ["Sarah", "Dad", "The parish", "Marcus", "Elena"];
	const threads = ["faith", "fatherhood", "craft", "restlessness", "patience"];

	// Coverage heatmap (demoted to "receipts"). Deterministic pseudo-noise.
	const WEEKS = 52;
	const heat = Array.from({ length: WEEKS * 7 }, (_, i) => {
		const h = Math.abs(Math.sin(i * 12.9898) * 43758.5453);
		const frac = h - Math.floor(h);
		const recency = (i / (WEEKS * 7)) * 0.45;
		return Math.min(4, Math.floor((frac * 0.85 + recency) * 5));
	});
	const daysOfData = heat.filter((l) => l > 0).length;

	// ── FUTURE (hardcoded) ────────────────────────────────────────────
	const aspirations = [
		{ icon: "ri:time-line", text: "Close the laptop by 6pm and be present" },
		{ icon: "ri:run-line", text: "Run a half marathon" },
		{ icon: "ri:book-2-line", text: "Read the Summa, slowly" },
		{ icon: "ri:seedling-line", text: "Learn patience with the kids" },
		{ icon: "ri:map-2-line", text: "Walk the Camino de Santiago" },
	];

	const examples = [
		{
			label: "A builder learning patience",
			content: `I'm a software engineer and father of two young kids. I believe in craft, privacy, and building things that respect people's attention. My faith is important to me but I hold it privately. I'm a workaholic — I know this about myself and I'm actively trying to close the laptop by 6pm and be present with my family. I tend toward urgency and I'm learning that the important things usually aren't urgent. I want to build something meaningful but I'm in a season of slowing down on purpose, not speeding up. I run to think. I read theology and philosophy before bed. I'm more interested in wisdom than productivity right now but the pull toward output is still strong every day.`,
		},
		{
			label: "A nurse reclaiming herself",
			content: `Twelve years in emergency medicine. I care deeply about people but I've neglected myself — I drink more than I should, I isolate when I'm stressed, and I've gained weight I'm not happy about. I don't want to be reminded about any of that. I'm starting a public health masters because I think I can help more people at the systems level than one patient at a time. I'm Catholic, it matters to me, and I pray most mornings even when I don't feel like it. I'm introverted but people assume I'm not because I'm good in a crisis. I want a life that has room for slowness.`,
		},
		{
			label: "A student between worlds",
			content: `Second year of law school. My parents immigrated from Guatemala and I'm the first in my family to go to graduate school. I carry that weight proudly but it makes it hard to admit when I'm struggling. I care about housing justice. I'm not religious but I'm spiritual in a way I can't fully articulate. My vice is perfectionism. I overwork, I over-prepare, I don't rest until I crash. I want to become someone who can do important work without destroying herself in the process.`,
		},
	];

	onMount(async () => {
		await load();
	});

	async function load() {
		loading = true;
		try {
			const res = await fetch("/api/wiki/narrative-identity");
			if (res.ok) {
				const data = await res.json();
				content = data.content || "";
				updatedAt = data.updated_at;
			}
		} catch (err) {
			console.error("Failed to load narrative identity:", err);
		} finally {
			loading = false;
		}
	}

	async function save(value: string) {
		const res = await fetch("/api/wiki/narrative-identity", {
			method: "PUT",
			headers: { "Content-Type": "application/json" },
			body: JSON.stringify({ content: value }),
		});
		if (res.ok) {
			const data = await res.json();
			updatedAt = data.updated_at;
		} else {
			throw new Error("Failed to save");
		}
	}

	function warningMessage(): string | false {
		if (tooLong) return `${charCount - MAX_CHARS} over the ${MAX_CHARS} character limit`;
		if (tooShort) return `${MIN_CHARS - charCount} more characters needed`;
		return false;
	}
</script>

<Page
	title="Narrative"
	description="How your Virtues understands you — where you've been, who you are now, and who you're becoming. You are its author."
	maxWidth="narrow"
>
	{#if loading}
		<div class="flex items-center justify-center text-foreground-muted" style="padding: 64px 0;">
			Loading...
		</div>
	{:else}
		<!-- Timeline-ordered register tabs -->
		<SubNav
			tabId={tab.id}
			route={tab.route}
			base="/narrative-identity"
			default="present"
			items={registers}
			insetX="0"
			divider
			ariaLabel="Narrative"
		/>

		{#if reg === "past"}
			<!-- ══ PAST — the portrait ══ -->
			<div class="reg-sub-top">Your record and its shape.</div>

			<!-- Co-authored arc: system chapters × your fortune line -->
			<div class="arc-block">
				<div class="cap">Your life in chapters</div>
				<svg class="arc" viewBox="0 0 100 44" preserveAspectRatio="none" aria-hidden="true">
					{#each boundaries as b (b.x)}
						<line class="boundary" x1={b.x} y1="0" x2={b.x} y2="44" />
					{/each}
					<line class="detected" x1={detected.x} y1="0" x2={detected.x} y2="44" />
					<path class="fortune" d={FORTUNE_PATH} fill="none" />
					{#each boundaries as b (b.x)}
						<circle class="dot" cx={b.x} cy={b.y} r="1.6" />
					{/each}
					<circle class="dot now" cx={nowDot.x} cy={nowDot.y} r="1.6" />
				</svg>
				<div class="chapter-labels">
					{#each chapters as ch (ch.title)}
						<span class="chapter-label" style="left: {ch.mid}%">{ch.title}</span>
					{/each}
				</div>
				<div class="detect-prompt">
					<Icon icon="ri:sparkling-2-line" width="13" class="detect-icon" />
					<span>A shift the system noticed around late 2023.</span>
					<button type="button" class="detect-btn">Name this chapter →</button>
				</div>
			</div>

			<!-- Chapters, readable -->
			<div class="portrait-block">
				<div class="cap">Chapters</div>
				<div class="chapters">
					{#each chapters as ch (ch.title)}
						<div class="chapter-row">
							<div class="chapter-head">
								<span class="chapter-title">{ch.title}</span>
								<span class="chapter-dates">{ch.dates}</span>
							</div>
							<span class="chapter-summary">{ch.summary}</span>
						</div>
					{/each}
				</div>
			</div>

			<!-- People -->
			<div class="portrait-block">
				<div class="cap">People who've shaped it</div>
				<div class="people">
					{#each people as p (p)}
						<span class="person"><span class="person-dot"></span>{p}</span>
					{/each}
				</div>
			</div>

			<!-- Threads -->
			<div class="portrait-block">
				<div class="cap">Threads you return to</div>
				<div class="threads">
					{#each threads as t (t)}
						<span class="thread">{t}</span>
					{/each}
				</div>
			</div>

			<!-- Coverage — the receipts -->
			<div class="portrait-block">
				<div class="cap">Coverage</div>
				<div class="heatmap" role="img" aria-label="{daysOfData} days of data">
					{#each heat as level, i (i)}
						<span class="cell" data-level={level}></span>
					{/each}
				</div>
				<div class="coverage-note">{daysOfData} days · Journal, Calendar, Health</div>
			</div>
		{:else if reg === "present"}
			<!-- ══ PRESENT — the checkpoint (real) ══ -->
			<div class="reg-sub-top">
				The checkpoint that grounds every conversation — read before your assistant replies, never
				repeated back at you.
			</div>

			<div class="checkpoint">
				{#if content.trim()}
					<p class="checkpoint-prose">{content}</p>
				{:else}
					<p class="checkpoint-empty">You haven't written your checkpoint yet.</p>
				{/if}

				<div class="checkpoint-foot">
					{#if updatedAt}
						<span class="checkpoint-date">
							Checkpoint · {new Date(updatedAt).toLocaleDateString(undefined, {
								month: "short",
								day: "numeric",
								year: "numeric",
							})}
						</span>
					{:else}
						<span></span>
					{/if}
					<button type="button" class="edit-btn" onclick={() => (editing = true)}>
						<Icon icon="ri:edit-line" width="14" />
						{content.trim() ? "Edit" : "Write it"}
					</button>
				</div>
			</div>
		{:else}
			<!-- ══ FUTURE — who you're becoming ══ -->
			<div class="reg-sub-top">Who you're becoming — goals, aspirations, the bucket list.</div>

			<ul class="aspirations">
				{#each aspirations as a (a.text)}
					<li class="aspiration">
						<Icon icon={a.icon} width="16" class="asp-icon" />
						<span>{a.text}</span>
					</li>
				{/each}
			</ul>
		{/if}
	{/if}
</Page>

<!-- Focused editor — editing is a deliberate action, not the default surface. -->
<Modal open={editing} onClose={() => (editing = false)} title="Your checkpoint" width="md">
	{#snippet children()}
		<Textarea
			bind:value={content}
			placeholder="What do you believe? What are you working on in yourself?"
			rows={10}
			autoResize
			maxRows={20}
			autoSave
			onSave={save}
			warning={warningMessage()}
			delight
		/>

		<div class="mt-3 flex items-center justify-between text-xs text-foreground-subtle">
			<span class:text-warning={tooLong || tooShort}>{charCount} / {MAX_CHARS}</span>
			<span>Read before every conversation · never repeated back at you</span>
		</div>

		<div style="margin-top: 24px;">
			<button
				class="flex items-center gap-2 text-sm text-foreground-subtle hover:text-foreground-muted hover:bg-surface-elevated cursor-pointer rounded-md px-2 py-1.5 -ml-2 transition-colors"
				onclick={() => (showExamples = !showExamples)}
			>
				<Icon
					icon={showExamples ? "ri:arrow-up-s-line" : "ri:arrow-down-s-line"}
					width="16"
					height="16"
				/>
				<span>See examples</span>
			</button>

			{#if showExamples}
				<div transition:slide={{ duration: 200 }} class="mt-4 flex flex-col gap-5">
					{#each examples as example (example.label)}
						<div>
							<p class="text-sm font-medium text-foreground-muted mb-1.5">{example.label}</p>
							<p class="text-sm text-foreground-subtle leading-relaxed italic">"{example.content}"</p>
						</div>
					{/each}
				</div>
			{/if}
		</div>
	{/snippet}
</Modal>

<style>
	.reg-sub-top {
		font-size: 0.9375rem;
		color: var(--color-foreground-muted);
		max-width: 34rem;
		margin-bottom: 1.75rem;
	}

	.cap {
		font-size: 0.6875rem;
		font-weight: 500;
		text-transform: uppercase;
		letter-spacing: 0.08em;
		color: var(--color-foreground-subtle);
		margin-bottom: 0.625rem;
	}

	/* ── Arc ── */
	.arc-block {
		margin-bottom: 2.25rem;
	}

	.arc {
		width: 100%;
		height: 120px;
		display: block;
		overflow: visible;
	}

	.fortune {
		stroke: var(--color-primary);
		stroke-width: 2;
		stroke-linecap: round;
		vector-effect: non-scaling-stroke;
	}

	.boundary {
		stroke: var(--color-border-strong);
		stroke-width: 1;
		vector-effect: non-scaling-stroke;
		opacity: 0.5;
	}

	.detected {
		stroke: var(--color-primary);
		stroke-width: 1;
		stroke-dasharray: 2 2;
		vector-effect: non-scaling-stroke;
		opacity: 0.6;
	}

	.dot {
		fill: var(--color-surface);
		stroke: var(--color-primary);
		stroke-width: 2;
		vector-effect: non-scaling-stroke;
	}
	.dot.now {
		fill: var(--color-primary);
	}

	.chapter-labels {
		position: relative;
		height: 1.25rem;
		margin-top: 0.375rem;
	}
	.chapter-label {
		position: absolute;
		transform: translateX(-50%);
		font-size: 0.75rem;
		color: var(--color-foreground-muted);
		white-space: nowrap;
	}

	.detect-prompt {
		display: flex;
		align-items: center;
		gap: 0.4375rem;
		margin-top: 1rem;
		font-size: 0.8125rem;
		color: var(--color-foreground-muted);
	}
	.detect-prompt :global(.detect-icon) {
		color: var(--color-primary);
		flex-shrink: 0;
	}
	.detect-btn {
		background: transparent;
		border: none;
		color: var(--color-primary);
		font: inherit;
		font-size: 0.8125rem;
		cursor: pointer;
		padding: 0;
	}
	.detect-btn:hover {
		text-decoration: underline;
	}

	/* ── Portrait blocks ── */
	.portrait-block {
		padding: 1.5rem 0;
		border-top: 1px solid var(--color-border-subtle);
	}

	.chapters {
		display: flex;
		flex-direction: column;
		gap: 1rem;
	}
	.chapter-row {
		display: flex;
		flex-direction: column;
		gap: 0.125rem;
	}
	.chapter-head {
		display: flex;
		align-items: baseline;
		gap: 0.625rem;
	}
	.chapter-title {
		font-family: var(--font-serif);
		font-size: 1.0625rem;
		color: var(--color-foreground);
	}
	.chapter-dates {
		font-size: 0.75rem;
		color: var(--color-foreground-subtle);
		font-variant-numeric: tabular-nums;
	}
	.chapter-summary {
		font-size: 0.9375rem;
		color: var(--color-foreground-muted);
	}

	.people {
		display: flex;
		flex-wrap: wrap;
		gap: 0.75rem 1.25rem;
	}
	.person {
		display: inline-flex;
		align-items: center;
		gap: 0.4375rem;
		font-size: 0.9375rem;
		color: var(--color-foreground);
	}
	.person-dot {
		width: 6px;
		height: 6px;
		border-radius: 50%;
		background: var(--color-primary);
	}

	.threads {
		display: flex;
		flex-wrap: wrap;
		gap: 0.5rem;
	}
	.thread {
		padding: 0.25rem 0.625rem;
		border-radius: 999px;
		border: 1px solid var(--color-border-subtle);
		font-size: 0.8125rem;
		color: var(--color-foreground-muted);
	}

	.heatmap {
		display: grid;
		grid-template-rows: repeat(7, 8px);
		grid-auto-flow: column;
		grid-auto-columns: 8px;
		gap: 2px;
		overflow-x: auto;
		padding-bottom: 4px;
	}
	.cell {
		width: 8px;
		height: 8px;
		border-radius: 2px;
		background: color-mix(in srgb, var(--color-foreground) 5%, transparent);
	}
	.cell[data-level="1"] {
		background: color-mix(in srgb, var(--color-primary) 22%, transparent);
	}
	.cell[data-level="2"] {
		background: color-mix(in srgb, var(--color-primary) 42%, transparent);
	}
	.cell[data-level="3"] {
		background: color-mix(in srgb, var(--color-primary) 66%, transparent);
	}
	.cell[data-level="4"] {
		background: var(--color-primary);
	}
	.coverage-note {
		margin-top: 0.625rem;
		font-size: 0.75rem;
		color: var(--color-foreground-subtle);
	}

	/* ── Checkpoint (present) ── */
	.checkpoint-prose {
		font-family: var(--font-serif);
		font-size: 1.0625rem;
		line-height: 1.7;
		color: var(--color-foreground);
		white-space: pre-wrap;
	}
	.checkpoint-empty {
		font-family: var(--font-serif);
		font-size: 1.0625rem;
		line-height: 1.7;
		color: var(--color-foreground-subtle);
		font-style: italic;
	}
	.checkpoint-foot {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 1rem;
		margin-top: 1.25rem;
	}
	.checkpoint-date {
		font-size: 0.75rem;
		color: var(--color-foreground-subtle);
	}
	.edit-btn {
		display: inline-flex;
		align-items: center;
		gap: 0.375rem;
		padding: 0.375rem 0.75rem;
		border: 1px solid var(--color-border);
		border-radius: 0.5rem;
		background: transparent;
		color: var(--color-foreground-muted);
		font-size: 0.8125rem;
		cursor: pointer;
		transition:
			background-color 0.15s ease,
			color 0.15s ease,
			border-color 0.15s ease;
	}
	.edit-btn:hover {
		background: var(--color-surface-elevated);
		color: var(--color-foreground);
		border-color: var(--color-border-strong);
	}

	/* ── Aspirations (future) ── */
	.aspirations {
		display: flex;
		flex-direction: column;
		gap: 0.5rem;
	}
	.aspiration {
		display: flex;
		align-items: center;
		gap: 0.625rem;
		padding: 0.625rem 0.75rem;
		border: 1px solid var(--color-border-subtle);
		border-radius: 0.625rem;
		background: var(--color-surface);
		font-size: 0.9375rem;
		color: var(--color-foreground);
	}
	.aspiration :global(.asp-icon) {
		color: var(--color-foreground-subtle);
		flex-shrink: 0;
	}
</style>
