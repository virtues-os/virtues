<!--
	QueryView: the rows behind a figure in a chat answer, at
	/chat/<chat>/tool/<call>. A figure computed over rows (a count, a sum, a
	trend) links to the query that computed it, and this tab shows that query
	the way the answer used it: the sentence that cites it, the SQL your
	server ran, and what came back. It reads the call from the stored chat,
	so it opens from a link anywhere and after a reload.
-->
<script lang="ts">
	import type { Tab } from "$lib/tabs/types";
	import type { QueryResult } from "$lib/types/Citation";
	import Icon from "$lib/components/Icon.svelte";
	import MarkdownCodeBlock from "$lib/components/MarkdownCodeBlock.svelte";
	import ShowNumbers from "$lib/components/chat/show/ShowNumbers.svelte";
	import ShowTable from "$lib/components/chat/show/ShowTable.svelte";
	import { formatTime } from "$lib/components/chat/show/show";
	import { deduplicateMessages, toUiMessage } from "$lib/components/chat/state/transcript";
	import { getChat } from "$lib/api/client";
	import { citingSentences, findToolCall, queryResult, type Piece, type StoredTurn } from "$lib/citations";
	import { windowShellStore } from "$lib/stores/window-shell.svelte";
	import { parseToolRef } from "$lib/utils/refRoutes";

	let { tab }: { tab: Tab } = $props();

	interface Loaded {
		result: QueryResult;
		/** When the turn that ran it started. */
		ran: Date | null;
		/** Each sentence in the chat that links this query, its link's words marked. */
		sentences: Piece[][];
		/** A `show` figure's own title. */
		title: string | null;
		chatId: string;
		chatTitle: string;
	}

	let loaded = $state<Loaded | null>(null);
	let error = $state<string | null>(null);
	let loading = $state(true);

	$effect(() => {
		const route = tab.route;
		const ref = parseToolRef(route);
		loaded = null;
		error = null;
		if (!ref) {
			error = "This link doesn't name a query.";
			loading = false;
			return;
		}
		loading = true;
		const ac = new AbortController();
		getChat<{ conversation?: { title?: string }; messages?: unknown[] }>(ref.chatId, ac.signal)
			.then((data) => {
				const turns = deduplicateMessages(data.messages ?? []).map((m) => toUiMessage(m, new Map())) as StoredTurn[];
				const call = findToolCall(turns, route);
				const result = call ? queryResult(call.output, call.input) : null;
				if (!call || !result) {
					error = "The chat this link points to doesn't have this query any more. Ask the question again to run it.";
					return;
				}
				const sentences = turns
					.filter((t) => t.role === "assistant")
					.flatMap((t) => t.parts)
					.flatMap((p) => {
						const part = p as { type?: string; text?: string };
						return part.type === "text" && part.text ? citingSentences(part.text, route) : [];
					});
				const title = (call.output as { title?: unknown }).title;
				loaded = {
					result,
					ran: call.turn.startedAt ?? call.turn.createdAt ?? null,
					sentences,
					title: typeof title === "string" && title ? title : null,
					chatId: ref.chatId,
					chatTitle: data.conversation?.title || "Untitled chat",
				};
			})
			.catch(() => {
				if (!ac.signal.aborted) error = "Couldn't load the chat this query came from. Check that your server is running and the chat still exists.";
			})
			.finally(() => {
				if (!ac.signal.aborted) loading = false;
			});
		return () => ac.abort();
	});

	// The figure as the answer wrote it, else a figure's title.
	const headline = $derived.by(() => {
		const cited = loaded?.sentences[0]?.find((p) => p.marked)?.text.trim();
		const text = cited || loaded?.title || "Query";
		return text.charAt(0).toUpperCase() + text.slice(1);
	});

	// A tab restored from the address bar has only the default name.
	$effect(() => {
		if (loaded && tab.label === "Query" && headline !== "Query") windowShellStore.updateTab(tab.id, { label: headline });
	});

	const rowsLine = $derived.by(() => {
		if (!loaded) return "";
		const { count, rows, more } = loaded.result;
		const n = (k: number) => `${k.toLocaleString()} ${k === 1 ? "row" : "rows"}`;
		if (!rows.length) return "No rows.";
		if (!more) return n(count);
		if (count > rows.length) return `${n(count)}. The model read the first ${rows.length.toLocaleString()}, shown here.`;
		return `More than ${n(rows.length)}. The first ${rows.length.toLocaleString()} are shown here.`;
	});

	// One row of up to four values reads as figures, as a `show` of numbers does.
	const asFigures = $derived(!!loaded && loaded.result.rows.length === 1 && loaded.result.columns.length <= 4);

	function openChat() {
		if (loaded) windowShellStore.openTabFromRoute(`/chat/${loaded.chatId}`, { label: loaded.chatTitle, focusExisting: true });
	}
</script>

{#snippet pieces(list: Piece[])}{#each list as piece, p (p)}{#if piece.marked}<mark>{piece.text}</mark>{:else}{piece.text}{/if}{/each}{/snippet}

<div class="query-view">
	<div class="inner">
		{#if loading}
			<div class="state">Loading the query…</div>
		{:else if error}
			<div class="state error">
				<Icon icon="mynaui:danger-circle" width="16" />
				<span>{error}</span>
			</div>
		{:else if loaded}
			<header class="head">
				<div class="eyebrow">
					<Icon icon="mynaui:table" width="12" />
					<span>Query{loaded.ran ? ` · ${formatTime(loaded.ran, { withDate: true })}` : ""}</span>
				</div>
				<h1 class="title">{headline}</h1>
				{#if loaded.sentences.length}
					<blockquote class="cited">
						{#each loaded.sentences.slice(0, 3) as sentence, i (i)}
							<p>{@render pieces(sentence)}</p>
						{/each}
					</blockquote>
				{/if}
				<p class="from">From <button type="button" class="link" onclick={openChat}>{loaded.chatTitle}</button></p>
			</header>

			{#if loaded.result.sql}
				<section class="section sql">
					<h2 class="label">The query your server ran</h2>
					<MarkdownCodeBlock token={{ lang: "sql", text: loaded.result.sql }} />
					<p class="note">It ran read-only, on your own database.</p>
				</section>
			{/if}

			<section class="section">
				<h2 class="label">What came back</h2>
				<p class="count">{rowsLine}</p>
				{#if asFigures}
					<ShowNumbers columns={loaded.result.columns} row={loaded.result.rows[0]} />
				{:else if loaded.result.rows.length}
					<ShowTable columns={loaded.result.columns} rows={loaded.result.rows} more={false} folded={false} />
				{/if}
			</section>
		{/if}
	</div>
</div>

<style>
	.query-view {
		width: 100%;
		height: 100%;
		overflow-y: auto;
	}
	.inner {
		max-width: 720px;
		margin: 0 auto;
		padding: 3.5rem 2rem 6rem;
	}

	.state {
		color: var(--color-foreground-subtle);
		font-size: 0.9rem;
		padding: 2rem 0;
	}
	.state.error {
		display: flex;
		align-items: center;
		gap: 0.5rem;
		color: var(--color-error);
	}

	.head {
		margin-bottom: 2.5rem;
	}
	.eyebrow {
		display: inline-flex;
		align-items: center;
		gap: 0.35rem;
		font-family: var(--font-sans);
		font-size: 0.75rem;
		color: var(--color-foreground-subtle);
		margin-bottom: 0.75rem;
	}
	/* The serif ships one cut: rank comes from size and ink, never weight. */
	.title {
		font-family: var(--font-serif);
		font-weight: 400;
		font-size: 1.75rem;
		line-height: 1.2;
		margin: 0;
		color: var(--color-foreground);
	}

	/* The answer's own sentences, in the serif, as a record card quotes a record. */
	.cited {
		margin: 1rem 0 0;
		font-family: var(--font-serif);
		font-size: 1.05rem;
		line-height: 1.5;
		color: var(--color-foreground-muted);
	}
	.cited p {
		margin: 0;
	}
	.cited p + p {
		margin-top: 0.5rem;
	}
	mark {
		background: var(--color-highlight);
		color: var(--color-foreground);
		padding: 0 1px;
		box-decoration-break: clone;
		-webkit-box-decoration-break: clone;
	}

	.from {
		margin: 1rem 0 0;
		font-size: 0.85rem;
		color: var(--color-foreground-subtle);
	}
	.link {
		all: unset;
		cursor: pointer;
		color: var(--color-primary);
	}
	.link:hover {
		text-decoration: underline;
		text-underline-offset: 2px;
	}
	.link:focus-visible {
		outline: 2px solid var(--color-border-focus);
		outline-offset: 2px;
	}

	.section + .section {
		margin-top: 2.25rem;
	}
	.label {
		margin: 0 0 0.75rem;
		font-family: var(--font-sans);
		font-size: 0.8rem;
		font-weight: 400;
		color: var(--color-foreground-subtle);
	}
	/* A model writes its SQL on one line as often as not. Wrapped, all of
	   it reads without a sideways scroll; its own line breaks still hold. */
	.sql :global(.prose-code pre),
	.sql :global(.prose-code code) {
		white-space: pre-wrap;
		overflow-wrap: anywhere;
	}
	.note {
		margin: 0.5rem 0 0;
		font-size: 0.8rem;
		color: var(--color-foreground-subtle);
	}
	.count {
		margin: 0 0 0.75rem;
		font-size: 0.95rem;
		color: var(--color-foreground);
	}

	@media (max-width: 640px) {
		.inner {
			padding: 2rem 1.25rem 4rem;
		}
	}
</style>
