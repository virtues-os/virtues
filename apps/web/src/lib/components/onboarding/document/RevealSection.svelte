<!--
  The reveal — the last screen of onboarding.

  IT MUST SHOW SOMETHING THEY DID NOT TELL IT. The previous version opened with
  the portrait: a paragraph of machine-written prose about a person who had just
  spent an hour writing prose about themselves. Everything on the screen was
  something they had said, so it proved nothing. A reveal earns its name only by
  showing what the box FOUND.

  THREE MOVEMENTS:

    ① the census   hard counts from the record, and the date of the oldest
                   thing on the box. Verifiable, impossible to fake, and
                   literally what they paid for.
    ② the portrait the prose — now READING the numbers above it rather than
                   arriving out of nowhere.
    ③ the door

  THE OLDEST DATE DOES THE MOST WORK. Most people have no idea their Mac has
  been keeping messages since 2015, and a specific date is the moment an
  appliance stops being an abstraction.

  THE EMPTY BOX IS ITS OWN SCREEN, not a degenerate case of this one. Someone
  who skipped sources reaches here in one click, and a portrait spun from an
  interview alone would imply the box had learned something it had not. It says
  so plainly instead, and offers the way back.
-->
<script lang="ts">
	import { onMount } from "svelte";
	import { fly } from "svelte/transition";
	import { expoOut } from "svelte/easing";
	import Icon from "$lib/components/Icon.svelte";
	import Markdown from "$lib/components/Markdown.svelte";
	import {
		triggerApplet,
		getNarrativeIdentity,
		updateNarrativeIdentity,
		getCensus,
		type Census,
	} from "$lib/api/client";
	import { formatDate } from "$lib/utils/dateUtils";

	interface Props {
		/** A non-empty portrait exists (from /api/setup/state). */
		ready: boolean;
		/** A draft run is currently in flight. */
		generating?: boolean;
		reduced?: boolean;
		onEnter: () => void;
		/** Back to the sources screen, for a box with nothing on it. */
		onConnect?: () => void;
	}

	let { ready, generating = false, reduced = false, onEnter, onConnect }: Props = $props();

	let content = $state("");
	let census = $state<Census | null>(null);
	let censusFailed = $state(false);
	let triggered = false;
	let triggerFailed = $state(false);

	/** Nothing connected, and we know it rather than are still waiting. */
	const empty = $derived(census !== null && census.lines.length === 0);

	const today = formatDate(new Date(), { day: "numeric", month: "long", year: "numeric" });
	const earliest = $derived(
		census?.earliest
			? formatDate(new Date(census.earliest), { month: "long", year: "numeric" })
			: null,
	);

	/**
	 * Counting up to each number, once.
	 *
	 * The only animation on the screen, and it is doing a job rather than
	 * decorating: a number that lands instantly is read as a label, and a number
	 * that climbs is read as an accumulation — which is what it is. Eased out so
	 * it settles rather than stops.
	 */
	let shown = $state<Record<string, number>>({});
	function countUp(id: string, to: number) {
		if (reduced) {
			shown[id] = to;
			return;
		}
		const dur = 900;
		const t0 = performance.now();
		const tick = (now: number) => {
			const p = Math.min(1, (now - t0) / dur);
			shown[id] = Math.round(to * (1 - Math.pow(1 - p, 3)));
			if (p < 1) requestAnimationFrame(tick);
		};
		requestAnimationFrame(tick);
	}

	async function triggerDraft() {
		if (triggered) return;
		triggered = true;
		triggerFailed = false;
		try {
			await triggerApplet("applet_narrative_identity_draft");
		} catch {
			triggerFailed = true;
		}
	}

	async function loadContent() {
		try {
			const d = await getNarrativeIdentity<{ content?: string }>();
			content = (d.content ?? "").trim();
		} catch {
			/* keep waiting */
		}
	}

	onMount(async () => {
		// The census first: it is the fast, certain half, and it is what the
		// screen is for. The portrait can take a minute and may never come.
		try {
			census = await getCensus();
			census.lines.forEach((l) => countUp(l.id, l.count));
		} catch {
			censusFailed = true;
		}
		if (!ready) void triggerDraft();
	});

	$effect(() => {
		if (ready && !content) void loadContent();
	});

	/**
	 * EDIT, NOT REDRAFT.
	 *
	 * There was a "Redraft" button here. It re-ran the generator on unchanged
	 * inputs, which is a slot machine: no reason to expect the second answer to
	 * be better than the first, and no way to say what was wrong with it. The
	 * only honest version of that button needs a box to type WHY into — and once
	 * there is a box, the shortest path from "this sentence is wrong" to a
	 * portrait that is right is to let someone fix the sentence.
	 *
	 * Which is also the doctrine the draft screen already set: their words, an
	 * edit box, not a wall of finished prose with an OK button. Regeneration
	 * belongs where the answers it came from are visible, not here.
	 */
	let editing = $state(false);
	let buffer = $state("");
	let saving = $state(false);
	let saveError = $state<string | null>(null);

	function startEdit() {
		buffer = content;
		saveError = null;
		editing = true;
	}

	async function save() {
		saving = true;
		saveError = null;
		try {
			await updateNarrativeIdentity({ content: buffer.trim() });
			content = buffer.trim();
			editing = false;
		} catch (e) {
			// Never swallow it — this is the one paragraph they may have just
			// rewritten by hand.
			saveError = e instanceof Error ? e.message : String(e);
		}
		saving = false;
	}

	/** 41_284 → "41,284". Grouped, because these numbers are meant to be felt. */
	const fmt = (n: number) => n.toLocaleString();
</script>

{#if empty}
	<!-- ① replaced: there is nothing to count. Say so, and offer the way back
	     rather than drafting a portrait out of an interview and calling it what
	     the box learned. -->
	<div in:fly={{ y: reduced ? 0 : 14, duration: reduced ? 0 : 420, easing: expoOut }}>
		<p class="hollow">
			Your box knows what you told it, and nothing else yet. Connect something and it will
			start keeping the record — the page waiting for you tomorrow is built from that.
		</p>
		<div class="cta">
			{#if onConnect}
				<button class="ob-btn" onclick={onConnect}>
					Connect a source
					<Icon icon="ri:arrow-right-line" width="16" />
				</button>
			{/if}
			<button class="ob-ghost" onclick={onEnter}>Enter anyway →</button>
		</div>
	</div>
{:else}
	<div in:fly={{ y: reduced ? 0 : 14, duration: reduced ? 0 : 520, easing: expoOut }}>
		<!-- ① THE CENSUS -->
		{#if census && census.lines.length}
			<p class="ob-label">On your box, as of {today}</p>

			<dl class="census">
				{#each census.lines as l (l.id)}
					<div class="line">
						<dt>{fmt(shown[l.id] ?? 0)}</dt>
						<dd>{l.label}</dd>
					</div>
				{/each}
			</dl>

			{#if earliest}
				<!-- The sleeper. A decade of messages nobody remembers keeping. -->
				<p class="span">
					The oldest thing it found is from <strong>{earliest}</strong>{#if census.span_days > 365}, spanning
						{Math.floor(census.span_days / 365)} years{/if}.
				</p>
			{/if}
		{:else if censusFailed}
			<p class="ob-note">Couldn't count what's on the box just now.</p>
		{/if}

		<!-- ② THE PORTRAIT -->
		{#if ready && content}
			<div class="portrait-block">
				<p class="ob-label">And what it makes of you</p>

				{#if editing}
					<textarea bind:value={buffer} rows="9" aria-label="Your portrait"></textarea>
					{#if saveError}
						<p class="ob-err"><Icon icon="ri:error-warning-line" width="14" /> {saveError}</p>
					{/if}
					<div class="edit-row">
						<button class="ob-btn tight" onclick={save} disabled={saving}>
							{saving ? "Saving…" : "Save"}
						</button>
						<button class="ob-ghost" onclick={() => (editing = false)}>Cancel</button>
					</div>
				{:else}
					<div class="portrait">
						<Markdown {content} isStreaming={!reduced} />
					</div>
					<p class="colophon">
						Written on your box, from the above and from what you wrote. Stored only here.
						It will be wrong in places — it is a first draft from limited data, and it is
						yours.
						<button class="inline-edit" onclick={startEdit}>Edit it</button>
					</p>
				{/if}
			</div>
		{:else}
			<div class="pending">
				<Icon icon="ri:quill-pen-line" width="16" class={generating || !triggerFailed ? "pen" : ""} />
				{#if triggerFailed}
					<span>The portrait will be written once your box has caught up.</span>
				{:else}
					<span>Writing the portrait from all of this…</span>
				{/if}
			</div>
		{/if}

		<!-- ③ THE DOOR -->
		<div class="cta">
			<button class="ob-btn" onclick={onEnter}>
				Enter Virtues
				<Icon icon="ri:arrow-right-line" width="16" />
			</button>

		</div>
	</div>
{/if}

<style>
	/* ── the census ────────────────────────────────────────────────────── */

	.census {
		margin: 1.25rem 0 0;
		display: grid;
		grid-template-columns: repeat(auto-fit, minmax(7.5rem, 1fr));
		gap: 1.5rem 1.25rem;
	}

	.line {
		display: flex;
		flex-direction: column;
		gap: 0.15rem;
	}

	/* The numbers are the point, so they get the display serif and the ink.
	   `tabular-nums` keeps them from jittering sideways as they count up — a
	   proportional 1 is narrower than a 0, so without it every digit shuffles on
	   every frame. */
	dt {
		font-family: var(--font-serif, Georgia, serif);
		font-size: clamp(1.6rem, 3vw, 2rem);
		line-height: 1;
		font-variant-numeric: tabular-nums;
		color: var(--color-foreground);
	}

	dd {
		margin: 0;
		font-size: 13px;
		line-height: 1.35;
		color: var(--color-foreground-subtle);
	}

	.span {
		margin: 1.75rem 0 0;
		font-family: var(--font-serif, Georgia, serif);
		font-size: 1.0625rem;
		line-height: 1.7;
		color: var(--color-foreground-muted);
	}

	.span strong {
		font-weight: 400;
		color: var(--color-foreground);
	}

	/* ── the portrait ──────────────────────────────────────────────────── */

	.portrait-block {
		margin-top: 2.5rem;
		padding-top: 2rem;
		border-top: 1px solid var(--color-border);
	}

	.portrait {
		margin-top: 1rem;
		font-family: var(--font-serif, Georgia, serif);
		font-size: 1.0625rem;
		line-height: 1.7;
		color: var(--color-foreground);
	}

	/* Keep the portrait prose calm — override the .markdown heading/spacing chrome. */
	.portrait :global(.markdown p) {
		margin-bottom: 0;
		font-size: inherit;
		line-height: inherit;
	}

	.colophon {
		margin: 1.25rem 0 0;
		font-size: 13px;
		line-height: 1.6;
		color: var(--color-foreground-subtle);
	}

	.hollow {
		font-family: var(--font-serif, Georgia, serif);
		font-size: 1.0625rem;
		line-height: 1.7;
		color: var(--color-foreground-muted);
	}

	.pending {
		margin-top: 2.5rem;
		padding-top: 2rem;
		border-top: 1px solid var(--color-border);
		display: flex;
		align-items: center;
		gap: 0.6rem;
		color: var(--color-foreground-muted);
		font-size: 14px;
	}

	.cta {
		display: flex;
		align-items: center;
		gap: 1.25rem;
	}

	/* ── editing the portrait ──────────────────────────────────────────── */

	/* The edit surface is set in the SAME serif at the same size as the prose it
	   replaces, so correcting a sentence feels like writing in the document
	   rather than filling in a form about it. */
	textarea {
		margin-top: 1rem;
		width: 100%;
		font-family: var(--font-serif, Georgia, serif);
		font-size: 1.0625rem;
		line-height: 1.7;
		color: var(--color-foreground);
		padding: 1rem 1.1rem;
		border: 1px solid var(--color-border);
		border-radius: 12px;
		background: color-mix(in srgb, var(--color-foreground) 3%, transparent);
		resize: vertical;
	}

	textarea:focus {
		outline: none;
		border-color: color-mix(in srgb, var(--color-primary) 60%, transparent);
	}

	.edit-row {
		margin-top: 1rem;
		display: flex;
		align-items: center;
		gap: 1.25rem;
	}

	/* Sits inside the colophon rather than below it: this is a correction to a
	   document, not a mode you enter. */
	.tight {
		margin-top: 0;
	}

	.inline-edit {
		margin-left: 0.35rem;
		font: inherit;
		color: var(--color-foreground-muted);
		background: none;
		border: none;
		border-bottom: 1px solid var(--color-border);
		padding: 0 0 1px;
		cursor: pointer;
		transition: color 0.15s ease;
	}

	.inline-edit:hover {
		color: var(--color-foreground);
		border-bottom-color: var(--color-foreground-subtle);
	}

	:global(.pen) {
		animation: pen-pulse 1.8s ease-in-out infinite;
	}

	@keyframes pen-pulse {
		0%,
		100% {
			opacity: 0.45;
		}
		50% {
			opacity: 1;
		}
	}

	@media (prefers-reduced-motion: reduce) {
		:global(.pen) {
			animation: none;
		}
	}
</style>
