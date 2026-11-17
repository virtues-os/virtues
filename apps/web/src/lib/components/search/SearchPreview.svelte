<script lang="ts">
	/**
	 * The right side of search on the desktop: what the selected row is,
	 * read before you open it. A day shows its opening paragraph and a dot
	 * for each part of it; a person, their line and the start of their
	 * article; a page, its opening text with your words marked; a chat, your
	 * question and the start of the answer; a thread, a dot per record filed
	 * in each of the last twelve weeks and its latest records.
	 *
	 * What is on the device draws at once. What needs the server (a page's
	 * text, a day's article) is asked for after the selection rests for a
	 * moment, and each answer is kept for as long as search is open, so
	 * moving back up the list redraws without asking again.
	 */
	import Icon from "$lib/components/Icon.svelte";
	import { getChat, getPageMarkdown, getSkyDay } from "$lib/api/client";
	import { getPersonById, getPlaceById, listDays } from "$lib/wiki/api";
	import { markdownWords } from "$lib/wiki/inlineMarkdown";
	import { projectStore } from "$lib/stores/project.svelte";
	import { recentRows } from "$lib/sidebar/recents.svelte";
	import { formatDate } from "$lib/utils/dateUtils";
	import { contentGlyph, previewOf, projectOfRow, rowNoun, type SearchRow } from "$lib/search/results";

	interface Props {
		row: SearchRow | null;
		/** The words to mark in the text, from the field. */
		query: string;
	}

	let { row, query }: Props = $props();

	type Line = { date: string; noun: string; title: string };
	type Preview =
		| { kind: "day"; title: string; hint: string | null; lede: string | null; dots: { x: number; y: number }[]; places: string[] }
		| { kind: "entity"; title: string; line: string | null; about: string | null; meta: string | null }
		| { kind: "page"; title: string; meta: string | null; text: string | null }
		| { kind: "chat"; title: string; meta: string | null; question: string | null; answer: string | null }
		| { kind: "thread"; title: string; meta: string | null; weeks: number[]; lines: Line[] }
		| { kind: "plain"; title: string; meta: string | null; text: string | null }
		| { kind: "ask"; query: string }
		| { kind: "open"; url: string }
		| { kind: "command"; label: string; shortcut: string | null };

	let preview = $state<Preview | null>(null);
	const kept = new Map<string, Preview>();

	const WEEK_MS = 7 * 24 * 3600 * 1000;

	function keyOf(r: SearchRow): string {
		switch (r.kind) {
			case "open":
				return `open:${r.url}`;
			case "ask":
				return `ask:${r.query}`;
			case "command":
				return `command:${r.command.id}`;
			case "record":
				return r.record.url;
			case "day":
				return `day:${r.day.date}`;
			case "entity":
				return r.entity.url;
			case "content":
				return `content:${r.hit.ontology}:${r.hit.record_id}`;
		}
	}

	const dateOf = (iso: string | number | null | undefined) => formatDate(iso, { month: "short", day: "numeric" });

	/** The first paragraphs of markdown as plain text, headings left out. */
	function opening(md: string | null | undefined, paragraphs = 2, max = 700): string | null {
		if (!md) return null;
		const out: string[] = [];
		for (const block of md.replace(/^---[\s\S]*?---\s*/, "").split(/\n\s*\n/)) {
			const b = block.trim();
			if (!b || /^#{1,6}\s/.test(b) || /^(```|<|\|)/.test(b)) continue;
			out.push(markdownWords(b).replace(/\s+/g, " ").trim());
			if (out.length >= paragraphs) break;
		}
		const text = out.join("\n\n");
		return text ? (text.length > max ? `${text.slice(0, max).replace(/\s+\S*$/, "")}…` : text) : null;
	}

	/** A dot per part of the day at its hour, stacked where two would touch. */
	function dayDots(hours: number[]): { x: number; y: number }[] {
		const lanes: number[] = [];
		return hours
			.sort((a, b) => a - b)
			.map((h) => {
				let lane = lanes.findIndex((last) => h - last >= 0.4);
				if (lane < 0) lane = lanes.length < 3 ? lanes.length : 2;
				lanes[lane] = h;
				return { x: (h / 24) * 100, y: lane * 7 };
			});
	}

	/** A thread's filed records by week, oldest week first, and its latest few. */
	function threadPreview(id: string, title: string): Preview {
		const members = recentRows({ pinned: true }).filter((r) => projectOfRow(r) === id);
		const now = Date.now();
		const weeks = Array.from({ length: 12 }, () => 0);
		for (const m of members) {
			const back = Math.floor((now - m.ts) / WEEK_MS);
			if (back >= 0 && back < 12) weeks[11 - back] = Math.min(5, weeks[11 - back] + 1);
		}
		const project = projectStore.projects.find((p) => p.id === id);
		const meta = project ? (project.archived_at ? `Ended ${dateOf(project.archived_at)}` : `Started ${dateOf(project.created_at)}`) : null;
		return {
			kind: "thread",
			title,
			meta,
			weeks,
			lines: members.slice(0, 4).map((m) => ({ date: dateOf(m.ts), noun: rowNoun({ kind: "record", record: m }) ?? "", title: m.title })),
		};
	}

	/** What can be drawn now, from what the device has. */
	function now(r: SearchRow): Preview {
		switch (r.kind) {
			case "open":
				return { kind: "open", url: r.url };
			case "ask":
				return { kind: "ask", query: r.query };
			case "command":
				return { kind: "command", label: r.command.label, shortcut: r.command.shortcut ?? null };
			case "day":
				return { kind: "day", title: r.day.title, hint: r.day.hint, lede: null, dots: [], places: [] };
			case "entity":
				return { kind: "entity", title: r.entity.name, line: null, about: null, meta: null };
			case "record": {
				const rec = r.record;
				if (rec.kind === "chat") return { kind: "chat", title: rec.title, meta: `Last active ${dateOf(rec.ts)}`, question: null, answer: null };
				if (rec.kind === "page") {
					// A server older than the field sends no `threads`.
					const threads = (rec.page.threads ?? []).map((t) => t.name).join(", ");
					return { kind: "page", title: rec.title, meta: `Edited ${dateOf(rec.page.updated_at)}${threads ? `, in ${threads}` : ""}`, text: rec.page.excerpt || null };
				}
				if (rec.kind === "thread") return threadPreview(rec.project.id, rec.title);
				return { kind: "plain", title: rec.title, meta: null, text: rec.applet.description };
			}
			case "content": {
				const hit = r.hit;
				const title = hit.title || rowNoun(r) || "Untitled";
				const meta = hit.timestamp ? dateOf(hit.timestamp) : null;
				if (hit.ontology === "app_page") return { kind: "page", title, meta, text: previewOf(hit) };
				if (hit.ontology === "app_chat") return { kind: "chat", title, meta, question: null, answer: previewOf(hit) };
				return { kind: "plain", title, meta, text: previewOf(hit) };
			}
		}
	}

	/** What the server adds, or null when the device had it all. */
	function fromServer(r: SearchRow, base: Preview): Promise<Preview> | null {
		if (r.kind === "day" && base.kind === "day") {
			const date = r.day.date;
			// The day list, not the one day: asking for a day by date creates its row.
			return Promise.allSettled([listDays(date, date), getSkyDay(date)]).then(([day, sky]) => {
				const moments = sky.status === "fulfilled" ? sky.value.moments.filter((m) => !m.planned && m.start_hour >= 0 && m.start_hour < 24) : [];
				const places = [...new Set(moments.filter((m) => m.kind === "stay" && m.label).map((m) => m.label))].slice(0, 4);
				return {
					...base,
					lede: day.status === "fulfilled" ? opening(day.value[0]?.article, 1, 420) : null,
					dots: dayDots(moments.map((m) => m.start_hour)),
					places,
				};
			});
		}
		if (r.kind === "entity" && base.kind === "entity") {
			if (r.entity.kind === "person") {
				return getPersonById(r.entity.id).then((p) => ({
					...base,
					line: p?.bond ?? (p?.relationship_category ? p.relationship_category[0].toUpperCase() + p.relationship_category.slice(1) : null),
					about: opening(p?.article ?? p?.content ?? p?.notes, 1, 420),
				}));
			}
			return getPlaceById(r.entity.id).then((p) => ({
				...base,
				line: [p?.category, p?.address].filter(Boolean).join(", ") || null,
				about: opening(p?.article ?? p?.content, 1, 420),
				meta: p?.last_seen ? `Last there ${formatDate(p.last_seen)}` : null,
			}));
		}
		const pageId = r.kind === "record" && r.record.kind === "page" ? r.record.page.id : r.kind === "content" && r.hit.ontology === "app_page" ? r.hit.record_id : null;
		if (pageId && base.kind === "page") {
			return getPageMarkdown(pageId).then((res) => ({ ...base, text: opening(res.markdown, 3) ?? base.text }));
		}
		const chatId = r.kind === "record" && r.record.kind === "chat" ? r.record.session.conversation_id : r.kind === "content" && r.hit.ontology === "app_chat" ? r.hit.record_id : null;
		if (chatId && base.kind === "chat") {
			return getChat<{ messages?: { role?: string; content?: string }[] }>(chatId).then((res) => {
				const messages = res.messages ?? [];
				const asked = messages.findIndex((m) => m.role === "user" && m.content?.trim());
				const reply = messages.slice(asked + 1).find((m) => m.role === "assistant" && m.content?.trim());
				return {
					...base,
					question: asked >= 0 ? opening(messages[asked].content, 1, 280) : null,
					answer: opening(reply?.content, 2, 520) ?? base.answer,
				};
			});
		}
		return null;
	}

	$effect(() => {
		const r = row;
		if (!r) {
			preview = null;
			return;
		}
		const key = keyOf(r);
		const have = kept.get(key);
		if (have) {
			preview = have;
			return;
		}
		// A preview is never worth the page: a row whose data has a shape this
		// code does not expect draws its title alone, and search keeps working.
		let base: Preview;
		try {
			base = now(r);
		} catch (err) {
			console.warn("[search] preview could not be drawn", err);
			base = { kind: "plain", title: rowNoun(r) ?? "", meta: null, text: null };
		}
		preview = base;
		let live = true;
		// Asked once the selection rests, so arrowing down the list does not
		// send a request per row passed.
		const timer = setTimeout(() => {
			const pending = fromServer(r, base);
			if (!pending) {
				kept.set(key, base);
				return;
			}
			pending
				.then((full) => {
					kept.set(key, full);
					if (live) preview = full;
				})
				.catch(() => {
					// The preview stays as the device drew it; opening the row still works.
				});
		}, 90);
		return () => {
			live = false;
			clearTimeout(timer);
		};
	});

	/** `text` cut into runs, with the query's words marked. */
	function marked(text: string): { text: string; hit: boolean }[] {
		const words = query
			.replace(/^#\w+\s*/, "")
			.split(/\s+/)
			.filter((w) => w.length >= 2)
			.map((w) => w.replace(/[.*+?^${}()|[\]\\]/g, "\\$&"));
		if (!words.length) return [{ text, hit: false }];
		// Split on a capturing group: the matches land at the odd indexes.
		return text
			.split(new RegExp(`(${words.join("|")})`, "i"))
			.map((part, i) => ({ text: part, hit: i % 2 === 1 }))
			.filter((run) => run.text);
	}

	const glyph = $derived.by(() => {
		if (!row) return null;
		switch (row.kind) {
			case "day":
				return "atlas:calendar";
			case "entity":
				return row.entity.kind === "person" ? "mynaui:user" : "mynaui:map-pin";
			case "record":
				return { chat: "atlas:chats", page: "atlas:pages", thread: "atlas:projects", applet: "atlas:applets" }[row.record.kind];
			case "content":
				return contentGlyph(row.hit);
			default:
				return null;
		}
	});
	const noun = $derived(row ? rowNoun(row) : null);
</script>

{#snippet text(body: string)}
	{#each body.split("\n\n") as para}
		<p class="reading">
			{#each marked(para) as run}{#if run.hit}<mark>{run.text}</mark>{:else}{run.text}{/if}{/each}
		</p>
	{/each}
{/snippet}

{#snippet dots(weeks: number[], label: string)}
	<div class="label">{label}</div>
	<div class="weeks" aria-hidden="true">
		{#each weeks as n}
			<span class="week">{#each { length: n } as _}<span class="dot"></span>{/each}</span>
		{/each}
	</div>
{/snippet}

{#if preview}
	<div class="preview">
		{#if noun}
			<div class="kind">
				{#if glyph}<Icon icon={glyph} width="14" />{/if}
				{noun}
			</div>
		{/if}

		{#if preview.kind === "day"}
			<h2>{preview.title}</h2>
			{#if preview.hint}<div class="meta">{preview.hint[0].toUpperCase() + preview.hint.slice(1)}</div>{/if}
			{#if preview.lede}{@render text(preview.lede)}{/if}
			{#if preview.dots.length}
				<div class="label">The day, a dot for each part of it</div>
				<div class="strip" aria-hidden="true">
					<span class="axis"></span>
					{#each preview.dots as d}<span class="dot" style:left="{d.x}%" style:bottom="{8 + d.y}px"></span>{/each}
				</div>
				<div class="hours" aria-hidden="true"><span style:left="25%">6am</span><span style:left="50%">noon</span><span style:left="75%">6pm</span></div>
			{/if}
			{#if preview.places.length}
				<dl class="facts"><dt>Places</dt><dd>{preview.places.join(", ")}</dd></dl>
			{/if}
		{:else if preview.kind === "entity"}
			<h2>{preview.title}</h2>
			{#if preview.line}<div class="meta">{preview.line}</div>{/if}
			{#if preview.about}{@render text(preview.about)}{/if}
			{#if preview.meta}<div class="meta below">{preview.meta}</div>{/if}
		{:else if preview.kind === "page"}
			<h2>{preview.title}</h2>
			{#if preview.meta}<div class="meta">{preview.meta}</div>{/if}
			{#if preview.text}{@render text(preview.text)}{/if}
		{:else if preview.kind === "chat"}
			<h2>{preview.title}</h2>
			{#if preview.meta}<div class="meta">{preview.meta}</div>{/if}
			{#if preview.question}<div class="question">{preview.question}</div>{/if}
			{#if preview.answer}{@render text(preview.answer)}{/if}
		{:else if preview.kind === "thread"}
			<h2>{preview.title}</h2>
			{#if preview.meta}<div class="meta">{preview.meta}</div>{/if}
			{#if preview.weeks.some((n) => n > 0)}{@render dots(preview.weeks, "Filed here, the last twelve weeks")}{/if}
			{#if preview.lines.length}
				<div class="lines">
					{#each preview.lines as line}
						<div class="line"><span class="sub">{line.date}</span><span class="sub">{line.noun}</span><span class="title">{line.title}</span></div>
					{/each}
				</div>
			{/if}
		{:else if preview.kind === "plain"}
			<h2>{preview.title}</h2>
			{#if preview.meta}<div class="meta">{preview.meta}</div>{/if}
			{#if preview.text}{@render text(preview.text)}{/if}
		{:else if preview.kind === "ask"}
			<div class="kind"><Icon icon="atlas:new-chat" width="14" />Ask</div>
			<h2>Ask Virtues</h2>
			<p class="reading">Virtues answers “{preview.query}” in a new chat, from your days, pages, chats and sources.</p>
		{:else if preview.kind === "open"}
			<div class="kind"><Icon icon="mynaui:globe" width="14" />Address</div>
			<h2 class="url">{preview.url.replace(/^https?:\/\//i, "")}</h2>
		{:else if preview.kind === "command"}
			<div class="kind"><Icon icon="atlas:plus" width="14" />Command</div>
			<h2>{preview.label}</h2>
			{#if preview.shortcut}<div class="meta below"><kbd>{preview.shortcut}</kbd></div>{/if}
		{/if}
	</div>
{/if}

<style>
	.preview {
		padding: 20px 24px 24px;
		color: var(--color-foreground);
	}

	.kind {
		display: flex;
		align-items: center;
		gap: 6px;
		font-size: 12px;
		color: var(--color-foreground-subtle);
	}

	h2 {
		margin: 8px 0 0;
		font-family: var(--font-serif);
		font-size: 26px;
		font-weight: 400;
		line-height: 1.15;
		overflow-wrap: anywhere;
	}

	h2.url {
		font-family: inherit;
		font-size: 17px;
	}

	.meta {
		margin-top: 6px;
		font-size: 12px;
		color: var(--color-foreground-subtle);
	}

	.meta.below {
		margin-top: 14px;
		display: flex;
		align-items: center;
		gap: 6px;
	}

	.reading {
		margin: 12px 0 0;
		font-family: var(--font-serif);
		font-size: 17px;
		line-height: 1.5;
		display: -webkit-box;
		-webkit-line-clamp: 6;
		line-clamp: 6;
		-webkit-box-orient: vertical;
		overflow: hidden;
	}

	mark {
		background: color-mix(in srgb, var(--color-primary) 13%, transparent);
		color: inherit;
		border-radius: var(--radius-sm);
		padding: 0 1px;
	}

	.question {
		margin-top: 14px;
		padding: 9px 12px;
		border-radius: var(--radius-md);
		background: var(--hover-bg);
		font-size: 13px;
		line-height: 1.45;
	}

	.label {
		margin-top: 20px;
		font-size: 11px;
		font-weight: 500;
		color: var(--color-foreground-subtle);
	}

	.dot {
		width: 6px;
		height: 6px;
		border-radius: 50%;
		background: color-mix(in srgb, var(--color-foreground) 72%, transparent);
	}

	.strip {
		position: relative;
		height: 30px;
		margin-top: 6px;
	}

	.axis {
		position: absolute;
		left: 0;
		right: 0;
		bottom: 4px;
		height: 1px;
		background: var(--color-border);
	}

	.strip .dot {
		position: absolute;
		transform: translateX(-3px);
	}

	.hours {
		position: relative;
		height: 14px;
		margin-top: 2px;
		font-size: 11px;
		color: var(--color-foreground-subtle);
	}

	.hours span {
		position: absolute;
		transform: translateX(-50%);
	}

	.weeks {
		display: flex;
		align-items: flex-end;
		gap: 10px;
		height: 42px;
		margin-top: 8px;
	}

	.week {
		display: flex;
		flex-direction: column-reverse;
		gap: 3px;
		min-width: 6px;
	}

	.facts {
		display: grid;
		grid-template-columns: 64px minmax(0, 1fr);
		margin: 16px 0 0;
		font-size: 13px;
	}

	.facts dt {
		color: var(--color-foreground-subtle);
	}

	.facts dd {
		margin: 0;
	}

	.lines {
		display: flex;
		flex-direction: column;
		gap: 7px;
		margin-top: 18px;
	}

	.line {
		display: grid;
		grid-template-columns: 64px 60px minmax(0, 1fr);
		font-size: 13px;
	}

	.sub {
		color: var(--color-foreground-subtle);
	}

	.title {
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}

	kbd {
		font-family: var(--font-sans);
		font-size: 11px;
		line-height: 1;
		padding: 3px 5px;
		border-radius: var(--radius-sm);
		background: var(--hover-bg);
		color: var(--color-foreground-subtle);
	}
</style>
