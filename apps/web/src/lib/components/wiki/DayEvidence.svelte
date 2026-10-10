<!--
	DayEvidence.svelte

	The record behind one sentence of a day article (or a table, a photo, a
	figure), opened by clicking it. Each source the writer cited shows its own
	words: a message as it was sent, a recording as the few turns nearest the
	sentence, with the words the sentence shares with it marked (lib/wiki/
	recordWords.ts). Nothing is summarized. With the veil on, no words and no
	names show.

	Where it sits is the page's call (`mode`): in the margin beside the
	sentence when the page has a margin, a popover when it doesn't, a sheet
	from the bottom on a phone. Its anatomy is the person card's (DayGloss): a
	kicker line, the body, one row of actions.
-->
<script lang="ts" module>
	import type { OntologyRecord } from "$lib/api/client";

	export type CardMode = "margin" | "popover" | "sheet";

	/** Records already fetched this session, by `table:id`. */
	const cache = new Map<string, OntologyRecord>();
</script>

<script lang="ts">
	import { FloatingContent, useClickOutside, useEscapeKey } from "$lib/floating";
	import { portal } from "$lib/actions/portal";
	import { getRecord } from "$lib/api/client";
	import type { MarginNote } from "$lib/wiki/dayArticle";
	import { contentWords, markRanges, messageBody, messageSender, nearTurns, sharedRanges } from "$lib/wiki/recordWords";
	import { veil } from "$lib/stores/veil.svelte";

	interface Props {
		anchor: HTMLElement;
		evidence: MarginNote[];
		/** The sentence's markdown, veil marks kept: it places the recording's window and marks the shared words. */
		sentence: string;
		/** What the card is about, for assistive tech: a sentence, or a table, photo or figure. */
		label?: string;
		mode: CardMode;
		/** In the margin: where the card sits inside the page body, in px. */
		place?: { top: number; left: number; width: number } | null;
		/** The day's zone, for a time the citation doesn't carry. */
		timezone: string | null;
		/** Where this source falls among the page's: "3 of 21". */
		position?: { n: number; total: number } | null;
		/** Take focus on opening: it was opened, or stepped to, from the keyboard. */
		focus?: boolean;
		oncite: (ref: string) => void;
		/** Write in the margin beside this sentence. */
		onnote?: () => void;
		/** Go to the previous (-1) or next (1) source on the page. */
		onstep?: (by: -1 | 1) => void;
		/** Close the card; `returnFocus` when the card was reached from the keyboard. */
		onclose: (returnFocus?: boolean) => void;
	}

	let {
		anchor,
		evidence,
		sentence,
		label = "The record behind this sentence",
		mode,
		place = null,
		timezone,
		position = null,
		focus = false,
		oncite,
		onnote,
		onstep,
		onclose,
	}: Props = $props();

	let card: HTMLElement | null = $state(null);

	useClickOutside(() => [card, anchor], () => onclose());
	// Focus goes back only where it came from the keyboard, or is now inside
	// the card (it would otherwise fall to the page when the card goes).
	useEscapeKey(() => onclose(focus || !!card?.contains(document.activeElement)));
	$effect(() => {
		if (card && focus) card.focus({ preventScroll: true });
	});

	type Loaded = { note: MarginNote; record: OntologyRecord | null; failed: boolean };
	let sources = $state<Loaded[]>([]);

	$effect(() => {
		// One source per record: a sentence, or a table's lines, can cite the
		// same moment twice. A citation with no record (a chat) still shows.
		const notes = evidence.filter((n, i, all) => all.findIndex((m) => (m.ref ?? m.label) === (n.ref ?? n.label)) === i);
		sources = notes.map((note) => ({ note, record: note.ref ? (cache.get(note.ref) ?? null) : null, failed: false }));
		notes.forEach((note, i) => {
			const ref = note.ref;
			if (!ref || cache.has(ref)) return;
			const [table, ...rest] = ref.split(":");
			getRecord(table, rest.join(":"))
				.then((r) => {
					cache.set(ref, r);
					if (sources[i]?.note === note) sources[i] = { note, record: r, failed: false };
				})
				.catch(() => {
					if (sources[i]?.note === note) sources[i] = { note, record: null, failed: true };
				});
		});
	});

	const MESSAGE = "data_communication_message";
	const RECORDING = "data_communication_transcription";
	const tableOf = (n: MarginNote) => (n.ref ?? "").split(":")[0];
	const refs = $derived(sources.flatMap((s) => (s.note.ref ? [s.note.ref] : [])));

	/** The kicker: what the record is, when, and who. The citation's own
	 *  label says the first two; a figure's citation may carry neither. */
	function kicker(s: Loaded): string[] {
		const [kind, time] = s.note.label.replace(/\s*·\s*$/, "").split(" · ");
		const table = tableOf(s.note);
		const at = s.record ? String(s.record.row[s.record.timestamp_column] ?? "") : "";
		const parts = [
			kind || (table === MESSAGE ? "Message" : table === RECORDING ? "Recording" : (s.record?.display_name ?? "Record")),
			time ||
				(at
					? new Date(at).toLocaleTimeString("en-US", { hour: "numeric", minute: "2-digit", timeZone: timezone ?? undefined })
					: ""),
		];
		if (table === RECORDING) parts.push("Voices not identified");
		else if (table === MESSAGE && s.record && !veil.hiding) parts.push(messageSender(s.record.row));
		return parts.filter(Boolean);
	}

	const want = $derived(contentWords(sentence));
	type Turn = { pieces: { text: string; marked: boolean }[]; near: boolean };

	/** The record's own words, cut where they share the sentence's. */
	function turnsOf(s: Loaded): Turn[] | null {
		if (!s.record) return null;
		const table = tableOf(s.note);
		const marked = (text: string) => markRanges(text, sharedRanges(text, want));
		if (table === MESSAGE) {
			const body = messageBody(s.record.row);
			return [{ pieces: body ? marked(body) : [{ text: "(no text)", marked: false }], near: true }];
		}
		if (table === RECORDING) {
			return nearTurns(String(s.record.row.text ?? ""), sentence).turns.map((t) => ({ pieces: marked(t.text), near: t.near }));
		}
		return null;
	}

	const bodies = $derived(sources.map((s) => turnsOf(s)));

	/** Said only once every record has loaded and none shares a word. */
	const sharesNothing = $derived.by(() => {
		if (veil.hiding || !want.size) return false;
		const cited = sources.map((s, i) => ({ s, body: bodies[i] })).filter(({ s }) => s.note.ref);
		if (!cited.length || cited.some(({ s, body }) => !s.record || !body)) return false;
		return cited.every(({ body }) => body!.every((t) => t.pieces.every((p) => !p.marked)));
	});
</script>

{#snippet content()}
	<div
		class="evidence-card"
		class:in-margin={mode === "margin"}
		class:sheet={mode === "sheet"}
		style={mode === "margin" && place ? `top: ${place.top}px; left: ${place.left}px; width: ${place.width}px` : undefined}
		bind:this={card}
		role="dialog"
		aria-label={label}
		tabindex="-1"
	>
		{#each sources as s, i (s.note.ref ?? s.note.label)}
			{@const table = tableOf(s.note)}
			{@const body = bodies[i]}
			<section class="source">
				<p class="kick">
					{#each kicker(s) as part, k (k)}<span class:kind={k === 0}>{k ? `· ${part}` : part}</span>{/each}
					{#if refs.length > 1 && s.note.ref}
						<button type="button" class="act own" onclick={() => oncite(s.note.ref as string)}>Open in Data</button>
					{/if}
				</p>
				{#if !s.note.ref}
					<p class="quiet">A question you asked Virtues that day. You'll find the day's chats in Data.</p>
				{:else if veil.hiding}
					<p class="quiet">Veiled. Hold V to read the record.</p>
				{:else if s.failed}
					<p class="quiet">Couldn't load this record. Check that your server is running, then open it again.</p>
				{:else if !s.record}
					<p class="quiet">Loading…</p>
				{:else if body}
					<blockquote class:message={table === MESSAGE}>
						{#each body as turn, t (t)}
							<p class:near={turn.near}>{#each turn.pieces as piece, p (p)}{#if piece.marked}<mark>{piece.text}</mark>{:else}{piece.text}{/if}{/each}</p>
						{/each}
					</blockquote>
				{:else}
					<p class="quiet">{s.record.display_name}</p>
				{/if}
			</section>
		{/each}
		{#if sharesNothing}
			<p class="quiet">This sentence shares no words with its source.</p>
		{/if}
		<div class="acts">
			<span class="slot">
				{#if refs.length === 1}<button type="button" class="act" onclick={() => oncite(refs[0])}>Open in Data</button>{/if}
			</span>
			{#if position}
				<span class="steps">
					<button type="button" class="step" aria-label="Previous source" disabled={position.n <= 1} onclick={() => onstep?.(-1)}>←</button>
					<span>{position.n} of {position.total}</span>
					<button type="button" class="step" aria-label="Next source" disabled={position.n >= position.total} onclick={() => onstep?.(1)}>→</button>
				</span>
			{/if}
			<span class="slot end">
				{#if onnote}<button type="button" class="act" onclick={() => onnote?.()}>Write a note</button>{/if}
			</span>
		</div>
	</div>
{/snippet}

{#if mode === "margin"}
	{@render content()}
{:else if mode === "sheet"}
	<div use:portal>{@render content()}</div>
{:else}
	<div use:portal>
		<FloatingContent {anchor} options={{ placement: "bottom-start", offset: 8, flip: true, shift: true, padding: 12, strategy: "fixed" }}>
			{@render content()}
		</FloatingContent>
	</div>
{/if}

<style>
	.evidence-card {
		width: min(20rem, calc(100vw - 32px));
		max-height: min(26rem, 60vh);
		overflow-y: auto;
		padding: 0.8rem 0.95rem 0.7rem;
		font-family: var(--font-sans);
		font-size: 0.8125rem;
		line-height: 1.45;
		color: var(--color-foreground-muted);
	}

	.evidence-card:focus {
		outline: none;
	}

	/* In the margin: the card lifts off the page beside its sentence, and
	   scrolls with it. */
	.evidence-card.in-margin {
		position: absolute;
		z-index: 5;
		max-height: none;
		overflow: visible;
		background: var(--color-surface);
		border: 1px solid var(--color-border);
		border-radius: 12px;
		animation: rise 0.14s ease-out;
	}

	/* On a phone: a sheet from the bottom of the screen. */
	.evidence-card.sheet {
		position: fixed;
		z-index: var(--z-modal);
		left: 0;
		right: 0;
		bottom: 0;
		width: auto;
		max-height: 60vh;
		padding-bottom: calc(1.25rem + env(safe-area-inset-bottom, 0px));
		background: var(--color-surface);
		border-top: 1px solid var(--color-border);
		border-radius: 12px 12px 0 0;
		animation: sheet-up 0.18s ease-out;
	}

	@keyframes rise {
		from {
			opacity: 0;
			transform: translateY(3px);
		}
		to {
			opacity: 1;
			transform: none;
		}
	}

	@keyframes sheet-up {
		from {
			transform: translateY(100%);
		}
		to {
			transform: none;
		}
	}

	.source + .source {
		border-top: 1px solid var(--color-border-subtle);
		margin-top: 0.625rem;
		padding-top: 0.625rem;
	}

	.kick {
		display: flex;
		flex-wrap: wrap;
		align-items: baseline;
		gap: 0 0.35rem;
		margin: 0 0 0.45rem;
		font-size: 0.75rem;
		color: var(--color-foreground-subtle);
	}

	.kick .kind {
		font-weight: 500;
		color: var(--color-foreground-muted);
	}

	.kick .own {
		margin-left: auto;
	}

	.act {
		font: inherit;
		color: var(--color-primary);
		background: none;
		border: none;
		padding: 0;
		cursor: pointer;
	}

	.act:hover {
		text-decoration: underline;
		text-underline-offset: 2px;
	}

	blockquote {
		margin: 0;
		font-family: var(--font-serif);
		font-size: 0.98rem;
		line-height: 1.45;
		color: var(--color-foreground-subtle);
	}

	blockquote p {
		margin: 0;
	}

	blockquote p + p {
		margin-top: 0.45rem;
	}

	/* The turn nearest the sentence, when one shares its words; its
	   neighbors set it in context. A message is one turn at full strength. */
	blockquote p.near {
		color: var(--color-foreground);
	}

	/* The record's words the sentence shares. */
	mark {
		background: var(--color-highlight);
		color: inherit;
		padding: 0 1px;
		box-decoration-break: clone;
		-webkit-box-decoration-break: clone;
	}

	.acts {
		display: grid;
		grid-template-columns: 1fr auto 1fr;
		align-items: baseline;
		gap: 0.75rem;
		margin-top: 0.6rem;
		padding-top: 0.55rem;
		border-top: 1px solid var(--color-border-subtle);
		font-size: 0.75rem;
	}

	.slot.end {
		justify-self: end;
	}

	.steps {
		display: inline-flex;
		align-items: baseline;
		gap: 0.4rem;
		color: var(--color-foreground-subtle);
		font-variant-numeric: tabular-nums;
	}

	.step {
		font: inherit;
		color: var(--color-foreground-muted);
		background: none;
		border: none;
		padding: 0 0.15rem;
		cursor: pointer;
	}

	.step:hover:not(:disabled) {
		color: var(--color-foreground);
	}

	.step:disabled {
		opacity: 0.35;
		cursor: default;
	}

	.quiet {
		margin: 0.375rem 0 0;
		font-size: 0.8125rem;
		line-height: 1.45;
		color: var(--color-foreground-subtle);
	}

	@media (prefers-reduced-motion: reduce) {
		.evidence-card.in-margin,
		.evidence-card.sheet {
			animation: none;
		}
	}
</style>
