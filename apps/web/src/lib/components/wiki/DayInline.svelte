<!--
	DayInline.svelte

	One sentence of a day article, drawn inline. The article's paragraphs are
	rendered here rather than through Markdown so each sentence can be its own
	element (clicked, it shows the record behind it). A sentence carries only
	inline markdown, so that is all this draws: links (and the editor's
	`![@Name](/person/id)` ref form), emphasis, strong, strike, and code.
	Entity links render as the same quiet Ref the Markdown component uses,
	which keeps the veil (it hides every `.ref-link`) and the hover preview
	working; a page can pass its own person renderer (the day's gloss card).
-->
<script lang="ts">
	import Ref from "$lib/components/Ref.svelte";
	import LinkChip from "$lib/components/LinkChip.svelte";
	import { parseEntityRoute } from "$lib/utils/refRoutes";
	import { tokenize, type Token } from "$lib/wiki/inlineMarkdown";

	interface Props {
		markdown: string;
		/** Draws a person link; falls back to a quiet Ref when not given. */
		person?: import("svelte").Snippet<[{ name: string; url: string }]>;
	}

	let { markdown, person }: Props = $props();

	const tokens = $derived(tokenize(markdown));
</script>

{#snippet inline(ts: Token[])}{#each ts as t, i (i)}{#if t.kind === "text"}{t.text}{:else if t.kind === "code"}<code>{t.text}</code>{:else if t.kind === "strong"}<strong>{@render inline(t.children)}</strong>{:else if t.kind === "em"}<em>{@render inline(t.children)}</em>{:else if t.kind === "del"}<del>{@render inline(t.children)}</del>{:else if t.kind === "link"}{@render link(t.text, t.url)}{/if}{/each}{/snippet}

{#snippet link(text: string, url: string)}{#if parseEntityRoute(url) !== null}{#if person && url.startsWith("/person/")}{@render person({ name: text, url })}{:else}<Ref displayName={text} {url} variant="quiet" />{/if}{:else if /^https?:\/\//.test(url)}<LinkChip href={url} label={text} variant="quiet" />{:else}<a href={url}>{text}</a>{/if}{/snippet}

{@render inline(tokens)}
