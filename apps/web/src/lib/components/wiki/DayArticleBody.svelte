<!--
	DayArticleBody.svelte

	The day article's body with its margin.

	Every sentence the writer tagged with evidence opens that evidence when
	clicked: the record's own words in a card beside it (DayEvidence), with a
	way on to the record in Data, and a way to write in the margin. A table or
	a photo opens its evidence the same way, as a whole.

	The card opens in the margin beside its sentence when the page has a
	margin, as a popover when it doesn't, and as a sheet on a phone. While it
	is open, ← and → step to the previous and next sentence (or figure) with a
	source.

	The margin holds the section times, as quiet text; what the record
	computes about the passage beside it, a stretch with nothing recorded and
	the first time a place or a person is in your record (`placeMarks`); and
	your notes, in your hand (DayHand). A note written on a sentence sits
	beside it with a bracket over exactly that sentence; a note with no
	passage, or whose words are no longer on the page, sits beside the lead
	(the Abstract and your numbers), saying what it was about. Where notes go
	is `placeNotes` (lib/wiki/dayNotes.ts).

	Paragraphs are drawn sentence by sentence (DayInline) so each can be its
	own element; a figure is DayFigure; headings, tables and photos go through
	Markdown. On a narrow screen the margin drops under its block. The markdown
	and its footnotes come from `parseDayArticle` (lib/wiki/dayArticle.ts).
-->
<script lang="ts">
	import type { Snippet } from "svelte";
	import { tick } from "svelte";
	import Markdown from "$lib/components/Markdown.svelte";
	import {
		clockMinutes,
		dayStartIn,
		figureRefs,
		placeMarks,
		recordingGaps,
		timeRange,
		veilMarks,
		type ArticleBlock,
		type FirstMark,
		type MarginNote,
	} from "$lib/wiki/dayArticle";
	import { placeNotes, plainSentence, type NoteAnchor } from "$lib/wiki/dayNotes";
	import { getDayFirsts, type DayFirstsApi, type WikiNote } from "$lib/wiki/api";
	import { stepDay } from "$lib/timeline/day";
	import { veiled } from "$lib/actions/veil";
	import { veil } from "$lib/stores/veil.svelte";
	import DayInline from "./DayInline.svelte";
	import DayEvidence, { type CardMode } from "./DayEvidence.svelte";
	import DayFigure from "./DayFigure.svelte";
	import DayHand from "./DayHand.svelte";

	interface Props {
		blocks: ArticleBlock[];
		/** The page's day, `YYYY-MM-DD`. */
		date: string;
		/** The zone the day was windowed in; null reads it in this device's zone. */
		timezone: string | null;
		/** The day's recorded stretches as [start, end] instants, for the margin's gaps. */
		coverage: [string, string][] | null;
		/** What leads the page (the Abstract, your numbers); its margin holds notes about the whole day. */
		lead?: Snippet;
		/** Your open notes on this day. */
		notes?: WikiNote[];
		/** Open Data on a cited item (`table:id`), from the sentence at `at`. */
		oncite?: (ref: string, at: { block: number; sentence: number } | null) => void;
		/** Draws a person link inside a sentence (the day's gloss card). */
		person?: Snippet<[{ name: string; url: string }]>;
		/** Keep a note you wrote; resolves to the saved note. */
		onwrite?: (body: string, anchor: NoteAnchor | null) => Promise<WikiNote>;
		onremove?: (id: number) => void;
		onundo?: (id: number) => void;
		/** Notes removed and waiting out their Undo. */
		removing?: number[];
		/** Notes whose removal just failed. */
		removeFailed?: number[];
	}

	let {
		blocks,
		date,
		timezone,
		coverage,
		lead,
		notes = [],
		oncite,
		person,
		onwrite,
		onremove,
		onundo,
		removing = [],
		removeFailed = [],
	}: Props = $props();

	const context = (b: ArticleBlock) => b.notes.filter((n) => n.kind === "cx");
	const blockEvidence = (b: ArticleBlock) => b.notes.filter((n) => n.kind === "ev" && n.ref);

	// ── Evidence ────────────────────────────────────────────────────────────
	/** What a card shows, and where a note written from it would go. */
	type Source = {
		evidence: MarginNote[];
		sentence: string;
		at: { block: number; sentence: number } | null;
		label: string;
	};

	/** Everything on the page with a source, in reading order: what ← and → step through. */
	type Stop = { id: string; source: Source };

	/** Every citation on the page by its record, so a figure's card can use the writer's label. */
	const cited = $derived(new Map(blocks.flatMap((b) => b.notes.filter((n) => n.kind === "ev" && n.ref).map((n) => [n.ref, n] as const))));
	const noteFor = (ref: string): MarginNote => cited.get(ref) ?? { kind: "ev", label: "", ref };

	function figureSource(fields: Record<string, string>, refs = figureRefs(fields)): Source {
		return { evidence: refs.map(noteFor), sentence: fields.text ?? "", at: null, label: "The record behind this figure" };
	}

	const stops = $derived<Stop[]>(
		blocks.flatMap((b, i): Stop[] => {
			if (b.kind === "paragraph") {
				return b.sentences.flatMap((s, j) =>
					s.evidence.length
						? [{ id: `${i}.${j}`, source: { evidence: s.evidence, sentence: s.markdown, at: { block: i, sentence: j }, label: "The record behind this sentence" } }]
						: [],
				);
			}
			if (b.kind === "figure") return b.fields && figureRefs(b.fields).length ? [{ id: `${i}`, source: figureSource(b.fields) }] : [];
			const ev = blockEvidence(b);
			return ev.length ? [{ id: `${i}`, source: { evidence: ev, sentence: b.markdown, at: null, label: "The record behind this passage" } }] : [];
		}),
	);

	/** The open card: the element it belongs to, what it shows, and where it sits. */
	type Open = Source & {
		anchor: HTMLElement;
		/** The keyboard control that opened it, which takes focus back on Escape. */
		trigger: HTMLElement | null;
		/** Its place in `stops`. */
		stop: string;
		mode: CardMode;
		place: { top: number; left: number; width: number } | null;
		/** Opened or stepped to from the keyboard: the card takes focus. */
		focus: boolean;
	};
	let open = $state<Open | null>(null);

	const sourceOf = $derived(new Map(stops.map((s) => [s.id, s.source])));

	const position = $derived.by(() => {
		const n = open ? stops.findIndex((s) => s.id === open!.stop) + 1 : 0;
		return n ? { n, total: stops.length } : null;
	});

	/** The nearest ancestor that scrolls: the page's right edge, for the card's width. */
	function scrollParent(el: HTMLElement): HTMLElement | null {
		for (let p = el.parentElement; p; p = p.parentElement) {
			if (/(auto|scroll)/.test(getComputedStyle(p).overflowY)) return p;
		}
		return null;
	}

	/**
	 * In the margin when the row has one: level with the anchor's first line,
	 * as wide as the margin and on into the page's right gutter, up to 20rem.
	 * Without a margin, a popover; on a phone, a sheet.
	 */
	function placeFor(anchor: HTMLElement): Pick<Open, "mode" | "place"> {
		const row = anchor.closest<HTMLElement>(".row");
		const style = row ? getComputedStyle(row) : null;
		const cols = style?.gridTemplateColumns.split(" ").map(parseFloat) ?? [];
		if (!root || !row || !style || cols.length < 2) {
			return { mode: matchMedia("(max-width: 40rem)").matches ? "sheet" : "popover", place: null };
		}
		const box = root.getBoundingClientRect();
		const rowBox = row.getBoundingClientRect();
		const left = rowBox.left - box.left + cols[0] + (parseFloat(style.columnGap) || 0);
		const view = scrollParent(root);
		const right = view ? view.getBoundingClientRect().left + view.clientWidth : document.documentElement.clientWidth;
		const rem = parseFloat(getComputedStyle(document.documentElement).fontSize) || 16;
		const width = Math.min(20 * rem, Math.max(cols[1], right - (box.left + left) - rem));
		const top = (anchor.getClientRects()[0] ?? anchor.getBoundingClientRect()).top - box.top - 4;
		return { mode: "margin", place: { top, left, width } };
	}

	function show(o: Omit<Open, "mode" | "place">) {
		close();
		o.anchor.classList.add("active");
		o.trigger?.setAttribute("aria-expanded", "true");
		open = { ...o, ...placeFor(o.anchor) };
	}

	function close(returnFocus = false) {
		open?.anchor.classList.remove("active");
		open?.trigger?.setAttribute("aria-expanded", "false");
		if (returnFocus) open?.trigger?.focus({ preventScroll: true });
		open = null;
	}

	/** The card follows its sentence when the page reflows, and changes form when the margin comes or goes. */
	function reposition() {
		if (!open) return;
		const next = placeFor(open.anchor);
		const was = open.place;
		const moved = !was || !next.place || was.top !== next.place.top || was.left !== next.place.left || was.width !== next.place.width;
		if (next.mode !== open.mode || moved) open = { ...open, ...next };
	}

	const refsOf = (evidence: MarginNote[]) => evidence.map((n) => n.ref).filter(Boolean).join(" ");

	/** A click anywhere on the sentence (or table, or photo) opens its source; a second click closes it. */
	function onClick(e: MouseEvent, stop: string) {
		// A link inside the sentence is its own target, and a reader selecting
		// words to copy them isn't asking for the source.
		if ((e.target as HTMLElement).closest("button, a")) return;
		if (String(window.getSelection() ?? "").trim()) return;
		const anchor = e.currentTarget as HTMLElement;
		const source = sourceOf.get(stop);
		if (!source) return;
		if (open?.anchor === anchor) return close();
		show({ ...source, anchor, trigger: anchor.nextElementSibling as HTMLElement | null, stop, focus: false });
	}

	/** The same, from the "source" control after it, for the keyboard and screen readers. */
	function onTrigger(e: MouseEvent, stop: string) {
		const trigger = e.currentTarget as HTMLElement;
		const anchor = trigger.previousElementSibling as HTMLElement | null;
		const source = sourceOf.get(stop);
		if (!anchor || !source) return;
		if (open?.anchor === anchor) return close();
		show({ ...source, anchor, trigger, stop, focus: true });
	}

	/** A figure's own records, from the figure. Opened from the keyboard, the
	 *  figure is the control the card hands focus back to. */
	function citeFigure(block: number, refs: string[], anchor: HTMLElement, focus: boolean) {
		const fields = blocks[block]?.fields;
		if (!fields || !refs.length) return;
		if (open?.anchor === anchor) return close();
		show({ ...figureSource(fields, refs), anchor, trigger: focus ? anchor : null, stop: `${block}`, focus });
	}

	/** ← and →: the previous or next source on the page, scrolled into view. */
	function step(by: -1 | 1) {
		if (!open || !root) return;
		const next = stops[stops.findIndex((s) => s.id === open!.stop) + by];
		const el = next && root.querySelector<HTMLElement>(`[data-stop="${next.id}"]`);
		if (!next || !el) return;
		const focus = !!document.activeElement?.closest(".evidence-card");
		const still = matchMedia("(prefers-reduced-motion: reduce)").matches;
		el.scrollIntoView({ block: "center", behavior: still ? "auto" : "smooth" });
		const trigger = el.nextElementSibling?.classList.contains("ev-trigger") ? (el.nextElementSibling as HTMLElement) : null;
		show({ ...next.source, anchor: el, trigger, stop: next.id, focus });
	}

	$effect(() => {
		if (!open) return;
		const onKey = (e: KeyboardEvent) => {
			if ((e.key !== "ArrowLeft" && e.key !== "ArrowRight") || e.altKey || e.ctrlKey || e.metaKey || e.shiftKey) return;
			if ((e.target as HTMLElement | null)?.closest("input, textarea, select, [contenteditable]")) return;
			e.preventDefault();
			step(e.key === "ArrowRight" ? 1 : -1);
		};
		document.addEventListener("keydown", onKey);
		return () => document.removeEventListener("keydown", onKey);
	});

	function cite(ref: string) {
		const at = open?.at ?? null;
		close();
		oncite?.(ref, at);
	}

	// ── The margin's computed marks ─────────────────────────────────────────
	const zone = $derived(timezone ?? Intl.DateTimeFormat().resolvedOptions().timeZone);
	const dayEnd = $derived(dayStartIn(stepDay(date, 1), zone));

	let firsts = $state<DayFirstsApi | null>(null);
	$effect(() => {
		const day = date;
		firsts = null;
		if (!blocks.some((b) => b.kind === "paragraph")) return;
		getDayFirsts(day)
			.then((f) => {
				if (date === day) firsts = f;
			})
			// The "new" marks add to the margin; without them the page is
			// still whole, so a failed fetch leaves them off.
			.catch(() => {});
	});

	const marks = $derived.by(() => {
		const at = (iso: string) => clockMinutes(Date.parse(iso), zone, dayEnd);
		const gaps = recordingGaps(coverage, dayStartIn(date, zone), dayEnd).map((g) => ({
			end: clockMinutes(g.end, zone, dayEnd),
			label: timeRange(g.start, g.end, zone),
		}));
		// People and named places before places with no name: a page can
		// show only a few, and "First visit in your record" alone says least.
		const news: FirstMark[] = [
			...(firsts?.people ?? []).map((p) => ({ at: at(p.at), title: null, line: `First message from ${p.name}`, veil: [p.name], rank: 0 })),
			...(firsts?.places ?? []).map((p) => ({
				at: at(p.at),
				title: p.name,
				line: "First visit in your record",
				veil: p.name ? [p.name] : [],
				rank: p.name ? 0 : 1,
			})),
		];
		return placeMarks(blocks, gaps, news);
	});

	// ── Notes ───────────────────────────────────────────────────────────────
	const placed = $derived(placeNotes(blocks, notes));
	const notesAt = (block: number) =>
		placed.filter((p) => p.at?.block === block).sort((a, b) => (a.at?.sentence ?? 0) - (b.at?.sentence ?? 0));
	const leadNotes = $derived(placed.filter((p) => !p.at));

	/** Where you're writing: a sentence, or the lead (null) for the whole day,
	 *  and where focus goes back to when you're done. */
	let writing = $state<{
		at: { block: number; sentence: number } | null;
		quote: string | null;
		returnTo: HTMLElement | null;
	} | null>(null);
	let draft = $state("");
	let saving = $state(false);
	let failed = $state(false);
	/** The note you just wrote, which draws itself once. */
	let drawn = $state<number | null>(null);

	/** Open the margin editor for the whole day, beside the lead. */
	export function writeAboutDay() {
		close();
		writing = { at: null, quote: null, returnTo: document.activeElement as HTMLElement | null };
		draft = "";
		failed = false;
		void focusEditor();
	}

	function writeBeside(o: Open) {
		const quote = o.at ? plainSentence(blocks[o.at.block]?.sentences[o.at.sentence]?.markdown ?? o.sentence) : null;
		// From the keyboard, focus comes back to the sentence's source control.
		const fromKeyboard = (document.activeElement as HTMLElement | null)?.matches(":focus-visible") ?? false;
		close();
		writing = { at: o.at, quote, returnTo: fromKeyboard ? o.trigger : null };
		draft = "";
		failed = false;
		void focusEditor();
	}

	function stopWriting() {
		const back = writing?.returnTo;
		writing = null;
		draft = "";
		back?.focus({ preventScroll: true });
	}

	async function focusEditor() {
		await tick();
		root?.querySelector<HTMLTextAreaElement>(".hand-editor textarea")?.focus({ preventScroll: true });
		layout();
	}

	async function keep() {
		const body = draft.trim();
		if (!body || !writing || !onwrite || saving) return;
		saving = true;
		failed = false;
		const anchor = writing.quote ? { quote: writing.quote, sentence: writing.at?.sentence } : null;
		try {
			const n = await onwrite(body, anchor);
			drawn = n.id;
			stopWriting();
		} catch {
			failed = true;
		} finally {
			saving = false;
		}
	}

	function onEditorKey(e: KeyboardEvent) {
		if (e.isComposing) return;
		if (e.key === "Escape") {
			e.stopPropagation();
			stopWriting();
		} else if (e.key === "Enter" && !e.shiftKey) {
			e.preventDefault();
			void keep();
		}
	}

	const written = (iso: string) => new Date(iso).toLocaleDateString("en-US", { month: "short", day: "numeric" });
	const uid = $props.id();
	const hintId = `margin-hint-${uid}`;

	// ── Margin layout ───────────────────────────────────────────────────────
	// Top to bottom: a note sits level with its sentence, or below whatever in
	// the margin is above it, and its row grows to hold it, so a note never
	// runs into the next section's time or past the article. A bracket spans
	// its sentence's lines.
	let root: HTMLElement | null = $state(null);

	function layout() {
		if (!root || matchMedia("(max-width: 56rem)").matches) return;
		let floor = -Infinity;
		for (const row of root.querySelectorAll<HTMLElement>(".row")) {
			const side = row.querySelector<HTMLElement>(":scope > .margin");
			const text = row.querySelector<HTMLElement>(":scope > .text");
			if (!side || !text) continue;
			side.style.minHeight = "";
			if (!row.hasAttribute("data-block")) {
				// Time labels and the lead's notes sit where they are.
				for (const n of side.querySelectorAll<HTMLElement>(".note, :scope > *")) floor = Math.max(floor, n.getBoundingClientRect().bottom + 12);
				continue;
			}
			const sideTop = side.getBoundingClientRect().top;
			const textBox = text.getBoundingClientRect();
			const at = (el: HTMLElement) =>
				(text.querySelector<HTMLElement>(`[data-s="${el.dataset.at}"]`) ?? text).getBoundingClientRect();
			for (const b of text.querySelectorAll<HTMLElement>(".bracket")) {
				const r = at(b);
				b.style.top = `${r.top - textBox.top + 2}px`;
				b.style.height = `${Math.max(8, r.height - 4)}px`;
			}
			for (const n of side.querySelectorAll<HTMLElement>(":scope > .note")) floor = Math.max(floor, n.getBoundingClientRect().bottom + 12);
			const placedEls = [...side.querySelectorAll<HTMLElement>(":scope > .placed")]
				.map((el) => ({ el, top: at(el).top - 2 }))
				.sort((x, y) => x.top - y.top);
			let bottom = sideTop;
			for (const { el, top } of placedEls) {
				el.style.top = `${Math.max(top, floor) - sideTop}px`;
				const r = el.getBoundingClientRect();
				floor = r.bottom + 12;
				bottom = Math.max(bottom, r.bottom);
			}
			if (placedEls.length) side.style.minHeight = `${bottom - sideTop + 8}px`;
		}
	}

	$effect(() => {
		// Re-place whenever what's in the margin changes.
		void placed;
		void writing;
		void blocks;
		void marks;
		tick().then(() => requestAnimationFrame(layout));
	});

	$effect(() => {
		if (!root) return;
		let frame = 0;
		const ro = new ResizeObserver(() => {
			cancelAnimationFrame(frame);
			frame = requestAnimationFrame(() => {
				layout();
				reposition();
			});
		});
		ro.observe(root);
		document.fonts?.ready.then(layout);
		return () => {
			ro.disconnect();
			cancelAnimationFrame(frame);
		};
	});
</script>

{#snippet editor()}
	<div class="hand-editor" class:placed={!!writing?.at} data-at={writing?.at?.sentence ?? null}>
		<textarea
			aria-label={writing?.quote ? `Your note beside: ${writing.quote}` : "Your note about the day"}
			aria-describedby={hintId}
			placeholder="Write in the margin…"
			bind:value={draft}
			onkeydown={onEditorKey}
			onblur={() => {
				// Only the editor that lost focus, and only if it's still empty.
				const w = writing;
				setTimeout(() => {
					if (writing === w && !draft.trim() && !saving) writing = null;
				}, 150);
			}}
		></textarea>
		<span class="hint" id={hintId}>
			{#if failed}<span role="status">Your server couldn't keep that note. Press Enter to try again.</span>{:else}Enter keeps it.{/if}
			<button type="button" class="discard" onmousedown={(e) => e.preventDefault()} onclick={stopWriting}>Discard note</button>
		</span>
	</div>
{/snippet}

<div class="day-body" bind:this={root} class:veiled-notes={veil.hiding}>
	{#if lead}
		<div class="row row-lead">
			<div class="text">{@render lead()}</div>
			<aside class="margin" aria-label="Your notes on this day">
				{#each leadNotes as p (p.note.id)}
					<DayHand
						text={p.note.body}
						written={written(p.note.created_at)}
						lost={p.lost}
						drawing={drawn === p.note.id}
						removing={removing.includes(p.note.id)}
						failed={removeFailed.includes(p.note.id)}
						onremove={onremove ? () => onremove(p.note.id) : undefined}
						onundo={onundo ? () => onundo(p.note.id) : undefined}
					/>
				{/each}
				{#if writing && !writing.at}{@render editor()}{/if}
			</aside>
		</div>
	{/if}
	{#each blocks as block, i (i)}
		{@const cx = context(block)}
		{@const ev = blockEvidence(block)}
		{@const marked = veilMarks(block.markdown)}
		{@const here = notesAt(i)}
		{@const computed = marks.get(i) ?? []}
		<div
			class="row"
			class:row-heading={block.kind === "heading"}
			class:row-table={block.kind === "table"}
			class:row-figure={block.markdown.startsWith("![")}
			data-block={block.kind === "paragraph" ? i : null}
		>
			{#if block.kind === "figure" && block.fields}
				<!-- The figure veils its own words, as the card does. -->
				{@const fields = block.fields}
				<div class="text" data-stop={figureRefs(fields).length ? `${i}` : null}>
					<DayFigure {fields} {date} {timezone} oncite={(refs, anchor, focus) => citeFigure(i, refs, anchor, focus)} />
				</div>
			{:else}
				<div class="text" use:veiled={{ hiding: veil.hiding, phrases: marked.phrases }}>
					{#if block.kind === "paragraph"}
						<div class="markdown markdown--article">
							<p>
								{#each block.sentences as s, j (j)}{#if j > 0 && s.space}{" "}{/if}{#if s.evidence.length}<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions --><span
											class="s"
											data-s={j}
											data-stop="{i}.{j}"
											data-refs={refsOf(s.evidence)}
											onclick={(e) => onClick(e, `${i}.${j}`)}
										><DayInline markdown={veilMarks(s.markdown).markdown} {person} /></span
									><button type="button" class="ev-trigger" data-for="evidence" aria-haspopup="dialog" aria-expanded="false" onclick={(e) => onTrigger(e, `${i}.${j}`)}>Source</button>{:else}<span data-s={j}><DayInline markdown={veilMarks(s.markdown).markdown} {person} /></span>{/if}{/each}
							</p>
						</div>
						{#each here as p (p.note.id)}<span class="bracket" data-at={p.at?.sentence} class:drawing={drawn === p.note.id} aria-hidden="true"></span>{/each}
					{:else if ev.length}
						<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
						<div class="s s-block" data-stop={i} data-refs={refsOf(ev)} onclick={(e) => onClick(e, `${i}`)}>
							<Markdown content={marked.markdown} refVariant="quiet" variant="article" />
						</div><button type="button" class="ev-trigger" data-for="evidence" aria-haspopup="dialog" aria-expanded="false" onclick={(e) => onTrigger(e, `${i}`)}>Source</button>
					{:else}
						<Markdown content={marked.markdown} refVariant="quiet" variant="article" />
					{/if}
				</div>
			{/if}
			{#if cx.length || computed.length || here.length || writing?.at?.block === i}
				<aside class="margin" aria-label={here.length || writing?.at?.block === i ? "Your notes beside this passage" : "Beside this passage"}>
					{#each cx as n (n.label)}<span class="note">{n.label}</span>{/each}
					{#each computed as mark, k (k)}
						{#if mark.kind === "gap"}
							<span class="note mark gap"><span aria-hidden="true" class="rule"></span><span>Nothing recorded{#each mark.spans as span (span)}<br />{span}{/each}</span></span>
						{:else}
							<span class="note mark first" use:veiled={{ hiding: veil.hiding, phrases: mark.veil }}
								><span class="new">new</span><span>{#if mark.title}<span class="what">{mark.title}</span><br />{/if}{mark.line}</span></span
							>
						{/if}
					{/each}
					{#each here as p (p.note.id)}
						<div class="placed" data-at={p.at?.sentence}>
							<DayHand
								text={p.note.body}
								written={written(p.note.created_at)}
								about={p.note.anchor?.quote ?? null}
								drawing={drawn === p.note.id}
								removing={removing.includes(p.note.id)}
								failed={removeFailed.includes(p.note.id)}
								onremove={onremove ? () => onremove(p.note.id) : undefined}
								onundo={onundo ? () => onundo(p.note.id) : undefined}
							/>
						</div>
					{/each}
					{#if writing?.at?.block === i}{@render editor()}{/if}
				</aside>
			{/if}
		</div>
	{/each}

	{#if open}
		{#key open.anchor}
			<DayEvidence
				anchor={open.anchor}
				evidence={open.evidence}
				sentence={open.sentence}
				label={open.label}
				mode={open.mode}
				place={open.place}
				{timezone}
				{position}
				focus={open.focus}
				oncite={cite}
				onnote={onwrite && open.at ? () => open && writeBeside(open) : undefined}
				onstep={step}
				onclose={close}
			/>
		{/key}
	{/if}
</div>

<style>
	/* The evidence card sits in here when it opens in the margin. */
	.day-body {
		position: relative;
		display: flex;
		flex-direction: column;
	}

	.row {
		display: grid;
		grid-template-columns: minmax(0, 40rem) 11rem;
		column-gap: 2.25rem;
		align-items: start;
	}

	/* The article's own spacing lives in Markdown's article register; a block
	   here is one paragraph, so its first/last margins are the rhythm. */
	.text :global(.markdown > :first-child) {
		margin-top: 0;
	}

	.row-heading .text :global(h2) {
		margin: 2.25rem 0 0.625rem;
	}

	.row-heading:first-child .text :global(h2) {
		margin-top: 0;
	}

	.row-heading:first-child .margin {
		padding-top: 0.6rem;
	}

	/* A sentence with evidence carries no mark until it is touched: a faint
	   wash on hover (after a beat, so reading past it doesn't flicker), the
	   highlight while its card is open. */
	.s {
		cursor: pointer;
		border-radius: 6px;
		box-decoration-break: clone;
		-webkit-box-decoration-break: clone;
		transition: background-color 0.12s ease;
	}

	.s:hover {
		background-color: color-mix(in srgb, var(--color-highlight) 45%, transparent);
		transition-delay: 0.2s;
	}

	/* The keyboard's way to a sentence's source: out of sight until focused,
	   then a small tag after the sentence. */
	.ev-trigger {
		-webkit-user-select: none;
		user-select: none;
		position: absolute;
		width: 1px;
		height: 1px;
		overflow: hidden;
		clip-path: inset(50%);
		white-space: nowrap;
		border: 0;
		padding: 0;
	}

	.ev-trigger:focus-visible {
		position: static;
		width: auto;
		height: auto;
		overflow: visible;
		clip-path: none;
		margin-left: 0.25rem;
		padding: 0 0.375rem;
		border-radius: 6px;
		background: var(--color-highlight);
		font-family: var(--font-sans);
		font-size: 0.75rem;
		color: var(--color-foreground);
		outline: 2px solid var(--color-border-focus);
		outline-offset: 1px;
	}

	.s:global(.active) {
		background-color: var(--color-highlight);
		transition-delay: 0s;
	}

	/* Back from Data: the sentence the citation came from, lit and let go. */
	.s:global(.flash) {
		animation: sentence-flash 1.6s ease;
	}

	@keyframes sentence-flash {
		0%,
		40% {
			background-color: var(--color-highlight);
		}
		100% {
			background-color: transparent;
		}
	}

	.s-block {
		display: block;
		border-radius: 6px;
	}

	.s-block:hover {
		background-color: transparent;
	}

	.row-table .s-block :global(tr:hover td) {
		background-color: color-mix(in srgb, var(--color-primary) 5%, transparent);
	}

	.margin {
		display: flex;
		flex-direction: column;
		gap: 0.375rem;
		padding-top: 0.3rem;
		font-family: var(--font-sans);
		font-size: 0.75rem;
		line-height: 1.35;
		color: var(--color-foreground-subtle);
		font-variant-numeric: tabular-nums;
	}

	/* A heading's notes hang beside it without making its row taller, so a
	   second note never pushes the paragraph below away from its heading. */
	.row-heading .margin {
		padding-top: 2.6rem;
		height: 0;
		overflow: visible;
	}

	/* A photo with its caption on the line under it: the caption reads as one,
	   smaller and quieter than the prose around it. */
	.row-figure .text :global(img) {
		display: block;
		width: 100%;
		margin: 0.375rem 0 0.625rem;
		border-radius: 6px;
	}

	.row-figure .text :global(p) {
		font-size: 0.9375rem;
		line-height: 1.45;
		color: var(--color-foreground-muted);
		margin-bottom: 1.75rem;
	}

	.note {
		display: block;
	}

	/* What the record says beside the passage: a short dashed rule before a
	   silence, a small claret "new" before a first. */
	.mark {
		display: grid;
		grid-template-columns: 1.4rem 1fr;
		column-gap: 0.4rem;
		line-height: 1.4;
	}

	.mark .rule {
		border-top: 1.5px dashed var(--color-border-strong);
		margin-top: 0.5rem;
	}

	.mark .new {
		font-size: 0.625rem;
		letter-spacing: 0.04em;
		color: var(--color-secondary);
		padding-top: 0.08rem;
	}

	.mark .what {
		color: var(--color-foreground-muted);
	}

	/* ── Your notes ── */
	.row-lead {
		margin-bottom: 0;
	}

	.row-lead .margin {
		gap: 1rem;
		padding-top: 0.5rem;
	}

	.row[data-block] .text {
		position: relative;
	}

	.row[data-block] .margin {
		position: relative;
		align-self: stretch;
	}

	/* Level with its sentence; the layout pass sets `top`. */
	.row[data-block] .placed {
		position: absolute;
		left: 0;
		right: 0;
	}

	/* A pen stroke down the right edge of the passage the note is about. */
	.bracket {
		position: absolute;
		left: calc(100% + 0.875rem);
		width: 8px;
		border: 1.5px solid var(--color-foreground-muted);
		border-left: 0;
		border-radius: 0 3px 3px 0;
		opacity: 0.75;
		transform: rotate(0.6deg);
		transform-origin: top;
		pointer-events: none;
	}

	.bracket.drawing {
		animation: bracket-draw 0.45s ease-out both;
	}

	@keyframes bracket-draw {
		from {
			clip-path: inset(0 0 100% 0);
		}
		to {
			clip-path: inset(0 0 0 0);
		}
	}

	.hand-editor {
		display: flex;
		flex-direction: column;
		gap: 0.3rem;
	}

	.hand-editor textarea {
		width: 100%;
		min-height: 4.5rem;
		resize: none;
		border: 0;
		border-bottom: 1px dashed var(--color-border-strong);
		outline: 0;
		background: transparent;
		padding: 0 0 0.25rem;
		font-family: var(--font-hand);
		font-size: 1.125rem;
		line-height: 1.3;
		color: var(--color-foreground-muted);
	}

	.hand-editor textarea::placeholder {
		font-family: var(--font-sans);
		font-size: 0.8125rem;
		color: var(--color-foreground-subtle);
	}

	.hint {
		display: flex;
		flex-wrap: wrap;
		gap: 0.25rem 0.75rem;
		font-size: 0.75rem;
		line-height: 1.35;
		color: var(--color-foreground-subtle);
	}

	.discard {
		font: inherit;
		color: inherit;
		background: none;
		border: none;
		padding: 0;
		cursor: pointer;
		text-decoration: underline;
		text-underline-offset: 2px;
	}

	/* With the veil on, your handwriting is as private as the names. */
	.veiled-notes .margin :global(.hand .words),
	.veiled-notes .margin :global(.hand .about) {
		filter: blur(4px);
		user-select: none;
	}

	@media (max-width: 56rem) {
		.row {
			grid-template-columns: minmax(0, 1fr);
		}

		/* Under the passage, in reading order, with no bracket to point. */
		.row[data-block] .placed {
			position: static;
			margin: 0 0 0.75rem 1rem;
		}

		.bracket {
			display: none;
		}

		.margin {
			padding-top: 0;
			margin: -0.5rem 0 1rem;
		}

		.row-heading .margin {
			padding-top: 0;
			height: auto;
			margin: -0.375rem 0 0.75rem;
		}
	}

	@media (prefers-reduced-motion: reduce) {
		.s {
			transition: none;
		}

		/* Marked without the fade; the page lifts the mark after a moment. */
		.s:global(.flash) {
			animation: none;
			background-color: var(--color-highlight);
		}

		.bracket.drawing {
			animation: none;
		}
	}
</style>
