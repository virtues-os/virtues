<!--
	DayInline.svelte

	One sentence of a day article, drawn inline. The article's paragraphs are
	rendered here rather than through Markdown so each sentence can be its own
	element (clicked, it shows the record behind it). A sentence carries only
	inline markdown: links, emphasis and plain text, so that is all this draws.
	Entity links render as the same quiet Ref the Markdown component uses, which
	keeps the veil (it hides every `.ref-link`) and the hover preview working.
-->
<script lang="ts">
	import Ref from "$lib/components/Ref.svelte";
	import { parseEntityRoute } from "$lib/utils/refRoutes";

	interface Props {
		markdown: string;
		/** Draws a person link; falls back to a quiet Ref when not given. */
		person?: import("svelte").Snippet<[{ name: string; url: string }]>;
	}

	let { markdown, person }: Props = $props();

	type Token =
		| { kind: "text"; text: string }
		| { kind: "link"; text: string; url: string }
		| { kind: "em" | "strong"; text: string };

	const INLINE = /\[([^\]]+)\]\(([^)\s]+)\)|\*\*([^*]+)\*\*|\*([^*]+)\*/g;

	const tokens = $derived.by(() => {
		const out: Token[] = [];
		let from = 0;
		for (const m of markdown.matchAll(INLINE)) {
			if ((m.index ?? 0) > from) out.push({ kind: "text", text: markdown.slice(from, m.index) });
			if (m[1] !== undefined) out.push({ kind: "link", text: m[1], url: m[2] });
			else if (m[3] !== undefined) out.push({ kind: "strong", text: m[3] });
			else out.push({ kind: "em", text: m[4] });
			from = (m.index ?? 0) + m[0].length;
		}
		if (from < markdown.length) out.push({ kind: "text", text: markdown.slice(from) });
		return out;
	});
</script>

{#each tokens as t, i (i)}{#if t.kind === "text"}{t.text}{:else if t.kind === "strong"}<strong>{t.text}</strong>{:else if t.kind === "em"}<em>{t.text}</em>{:else if t.kind === "link"}{@render link(t.text, t.url)}{/if}{/each}

{#snippet link(text: string, url: string)}{#if parseEntityRoute(url) === null}<a href={url} target="_blank" rel="noopener noreferrer">{text}</a>{:else if person && url.startsWith("/person/")}{@render person({ name: text, url })}{:else}<Ref displayName={text} {url} variant="quiet" />{/if}{/snippet}
