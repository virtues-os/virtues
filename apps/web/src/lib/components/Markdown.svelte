<script lang="ts">
	import { tick, untrack, type Component } from 'svelte';
	import { Streamdown, theme as streamdownTheme } from 'svelte-streamdown';
	import type { CitationContext } from '$lib/types/Citation';
	import { webKey } from '$lib/citations/builder';
	import Ref from './Ref.svelte';
	import RecordCard, { type CardSource } from './RecordCard.svelte';
	import { RECORD_CITE, recordCards, type RecordCiteDetail } from '$lib/utils/recordCards.svelte';
	import { windowShellStore } from '$lib/stores/window-shell.svelte';
	import LinkChip from './LinkChip.svelte';
	import MarkdownCodeBlock from './MarkdownCodeBlock.svelte';
	import Icon from './Icon.svelte';
	import { alertTone } from '$lib/document/callouts';
	import { linkTarget, parseEntityRoute } from '$lib/utils/refRoutes';
	import { preprocessMarkdown } from '$lib/utils/markdownPreprocess';
	import { openWebLink } from '$lib/utils/webLinks';
	import { backendUrl } from '$lib/config/backend';

	interface Props {
		content: string;
		isStreaming?: boolean;
		/**
		 * Fade words in as they arrive. Separate from `isStreaming` because it
		 * must NOT end with the stream: turning it off rebuilds every word of
		 * the reply as plain text in one go, and on WebKit (the phone, the Mac
		 * app) that rebuild threw a reader scrolled into the reply back to its
		 * first line. The caller keeps it on for a reply it watched arrive.
		 */
		animate?: boolean;
		/** What this text's turn can cite: its links to these open the record card. */
		citations?: CitationContext;
		// How inline entity refs render: "link" (default, chat answers) or "quiet"
		// (dotted-underline prose links for flowing text like the day biography).
		refVariant?: "link" | "quiet";
		/**
		 * The type register. "ui" (default) is the interface's sans — chat, a
		 * file preview, a PDF note. "article" is the record's serif, for a wiki
		 * article: a day, a year, a person, a place, an organization, the
		 * narrative identity.
		 *
		 * What the text looks like is prose.css (`.prose`, on Streamdown's root;
		 * `.prose--article` for an article); the VALUES live in `--md-*`
		 * (themes.css). Every one of these surfaces used to set its own
		 * font-family and font-size locally, which is how they ended up with
		 * five sizes between them and how the day article ended up in sans.
		 */
		variant?: "ui" | "article";
	}

	let {
		content,
		isStreaming = false,
		animate = false,
		citations,
		refVariant = "link",
		variant = "ui"
	}: Props = $props();

	const processedContent = $derived(content ? preprocessMarkdown(content) : '');

	// The record card. Every link to a record in this text is a citation, and
	// so is a link to a query or a web page this turn's tools returned: a
	// click opens one card for the whole text, anchored on that link, and ←
	// and → step through them in the order the text links them
	// (lib/utils/recordCards).
	let root = $state<HTMLElement | null>(null);
	const cards = recordCards(() => root);

	// A Ref to a record asks the text it is in for its card.
	$effect(() => {
		const el = root;
		if (!el) return;
		const onCite = (e: Event) => {
			const { route, focus } = (e as CustomEvent<RecordCiteDetail>).detail;
			e.preventDefault();
			cards.show(route, e.target as HTMLElement, focus);
		};
		el.addEventListener(RECORD_CITE, onCite);
		return () => el.removeEventListener(RECORD_CITE, onCite);
	});

	// A reply that is still arriving re-renders its links. Keep the card on
	// the same record's new link, or close it if the link is gone.
	$effect(() => {
		void processedContent;
		if (!untrack(() => cards.open)) return;
		tick().then(() => cards.reattach());
	});

	// What the open card shows: the query's rows, the web page as the search
	// returned it, or the record with the passage the turn's search matched.
	const openSources = $derived.by((): CardSource[] => {
		const o = cards.open;
		if (!o) return [];
		const target = linkTarget(o.route);
		if (target.kind === 'query') {
			const query = citations?.queries.get(target.callId);
			return query ? [{ ref: o.route, query }] : [];
		}
		if (target.kind === 'external') {
			const web = citations?.byUrl.get(webKey(o.route));
			return web ? [{ ref: o.route, web }] : [];
		}
		const found = citations?.byRoute.get(o.route);
		return [{ ref: o.route, passage: found?.passage, title: found?.title }];
	});

	// A record or a query's rows open in a tab beside; a web page opens where
	// a link to the web goes in the app.
	function openSource(ref: string) {
		const label = openSources[0]?.title ?? cards.open?.anchor.textContent ?? undefined;
		cards.close();
		if (linkTarget(ref).kind === 'external') void openWebLink(ref);
		else windowShellStore.openRouteBeside(ref, label);
	}

	// TeX (`$…$`, `$$…$$`) is drawn by KaTeX, which Streamdown's math
	// component loads. It is fetched the first time a reply holds a `$`, so
	// a reply without math never downloads it. Until it arrives (and when it
	// cannot parse the TeX) the formula shows as its source.
	let mathElement = $state<Component<any> | undefined>();
	$effect(() => {
		if (mathElement || !processedContent.includes('$')) return;
		import('svelte-streamdown/math').then((m) => (mathElement = m.default));
	});

	// Base for resolving relative URLs. The app is SPA-only (routes/+layout.js
	// sets `ssr = false`), so window always exists here.
	//
	// It has to be an http(s) origin even when ours isn't. Streamdown resolves
	// every relative href against this and then drops anything that doesn't come
	// back http(s) — so under the Tauri shell, where the origin is
	// `tauri://localhost`, a bare `page/page_abc` resolves to a `tauri:` URL and
	// renders as dead text with a `[blocked]` suffix. Absolute paths (`/page/…`)
	// bypass that check, so this only governs the relative-without-slash case;
	// the placeholder never reaches the DOM, because our `link` snippet below
	// renders from `token.href`, not from the resolved URL.
	const origin = /^https?:$/.test(window.location.protocol)
		? window.location.origin
		: 'http://localhost';

	/**
	 * Streamdown's own theme with its look taken out: prose.css styles every
	 * element both renderers share, and Streamdown's default classes leaked
	 * wherever a rule set nothing (a heading's top margin, a list item's
	 * padding, 200-400px table cells, hard-coded grays that ignore the dark
	 * themes). Code, images, tables, callouts and citations are our snippets
	 * below; mermaid and math keep Streamdown's classes, which place things
	 * rather than style text.
	 */
	const BLANKED = [
		'link', 'h1', 'h2', 'h3', 'h4', 'h5', 'h6', 'paragraph', 'ul', 'ol', 'li', 'codespan',
		'blockquote', 'table', 'thead', 'tbody', 'tfoot', 'tr', 'td', 'th', 'sup', 'sub', 'hr',
		'strong', 'br', 'em', 'del', 'footnoteRef', 'descriptionList', 'descriptionTerm',
		'descriptionDetail',
	] as const;
	const blank = (part: Record<string, unknown>) =>
		Object.fromEntries(Object.entries(part).map(([k, v]) => [k, typeof v === 'string' ? '' : v]));
	const proseTheme = {
		...streamdownTheme,
		...Object.fromEntries(BLANKED.map((key) => [key, blank(streamdownTheme[key] as Record<string, unknown>)])),
	} as typeof streamdownTheme;
</script>

<div class="markdown" bind:this={root}>
		<Streamdown
			content={processedContent}
			class={variant === "article" ? "streamdown-content prose prose--article" : "streamdown-content prose"}
			theme={proseTheme}
			mergeTheme={false}
			defaultOrigin={origin}
			components={mathElement ? { math: mathElement } : undefined}
			allowedLinkPrefixes={['*']}
			animation={{
				enabled: animate,
				type: 'fade',
				duration: 300,
				tokenize: 'word',
				animateOnMount: false
			}}
		>
			<!-- `[1]` stays as written. An answer cites by linking what a tool
			     returned, so a bracketed number names no source, and Streamdown's
			     own citation badge would point at nothing. -->
			{#snippet inlineCitation({ token }: { token: any })}{token.raw}{/snippet}

			<!-- The page editor's table markup, so prose.css draws one table for both. -->
			{#snippet table({ children }: { children: import('svelte').Snippet })}
				<div class="tableWrapper">
					<table>{@render children()}</table>
				</div>
			{/snippet}

			<!-- `> [!NOTE]` as a page's callout: the tone's mark beside the body,
			     the alert's kind read as the converter reads it. -->
			{#snippet alert({ children, token }: { children: import('svelte').Snippet; token: any })}
				{@const tone = alertTone(token.variant)}
				<aside class="prose-callout" data-tone={tone.tone}>
					<span class="prose-callout-mark"><Icon icon={tone.icon} width="16" aria-label={tone.label} /></span>
					<div class="prose-callout-body">{@render children()}</div>
				</aside>
			{/snippet}

			<!-- To-dos in a page's to-do markup (`ul[data-type=taskList]`, each
			     item a box beside its text), which prose.css draws for both. The
			     box is the record's; it cannot be ticked from a reply. -->
			{#snippet ul({ children, token }: { children: import('svelte').Snippet; token: any })}
				{#if token.tokens?.some((item: any) => item.task)}
					<ul data-type="taskList">{@render children()}</ul>
				{:else}
					<ul>{@render children()}</ul>
				{/if}
			{/snippet}

			{#snippet li({ children, token }: { children: import('svelte').Snippet; token: any })}
				{#if token.task}
					<li data-checked={token.checked ? 'true' : 'false'}>
						<label><input type="checkbox" checked={token.checked} disabled /></label>
						<div>{@render children()}</div>
					</li>
				{:else}
					<li value={token.value ?? undefined}>{@render children()}</li>
				{/if}
			{/snippet}

			{#snippet code({ token }: { token: any })}
				<MarkdownCodeBlock {token} {isStreaming} />
			{/snippet}

			{#snippet link({ children, token }: { children: import('svelte').Snippet; token: any })}
				{@const url = token?.href}
				{@const target = linkTarget(url)}
				{@const web = target.kind === 'external' ? citations?.byUrl.get(webKey(url)) : undefined}
				{#if target.kind === 'query' && citations?.queries.has(target.callId)}
					<!-- The rows behind a figure: a citation like a record's.
					     A click opens its card; Cmd-click opens the rows beside. -->
					<button
						type="button"
						class="ref-link ref-link--cite"
						data-cite-route={url}
						aria-haspopup="dialog"
						onclick={(e) => {
							if (e.metaKey || e.ctrlKey) windowShellStore.openRouteBeside(url, token.text);
							else cards.show(url, e.currentTarget, e.detail === 0);
						}}
					>{@render children()}</button>
				{:else if target.kind === 'query'}
					<!-- A query ref with no result in hand (an answer whose
					     tool calls are not loaded): the rows open beside, read
					     from the stored chat. -->
					<button type="button" class="ref-link" onclick={() => windowShellStore.openRouteBeside(url, token.text)}
						>{@render children()}</button
					>
				{:else if web}
					<!-- A page a web search in this turn returned: a citation,
					     whose card shows the page as the search read it. A
					     click opens the card; Cmd-click opens the page. -->
					<button
						type="button"
						class="ref-link ref-link--cite"
						data-cite-route={url}
						aria-haspopup="dialog"
						onclick={(e) => {
							if (e.metaKey || e.ctrlKey) void openWebLink(url);
							else cards.show(url, e.currentTarget, e.detail === 0);
						}}
					>{@render children()}</button>
				{:else if target.kind === 'entity'}
					<!-- `[@X](/entity/id)`: the editor's `@` marks the ref, and
					     is no part of its name, as in the image form below. A
					     link to a record draws itself as a citation. -->
					<Ref displayName={(token.text ?? '').replace(/^@/, '')} url={url} variant={refVariant} />
				{:else if target.kind === 'external'}
					<!-- A link to the web that no search in this turn returned.
					     Always quiet, even where entity refs are loud: an answer
					     that links six sites was ending every bullet in a row of
					     blue, and the paragraph read as a link farm with prose in
					     between. Dotted, body-colored, no globe: the domain name
					     is already the tell, and hovering names the site in full. -->
					<LinkChip href={url} label={token.text} variant="quiet" />
				{:else if url}
					<a href={url} target="_blank" rel="noopener noreferrer">{@render children()}</a>
				{:else}
					<span>{@render children()}</span>
				{/if}
			{/snippet}

			{#snippet image({ token }: { token: any })}
				{@const url = token?.href}
				{@const label = (token?.text ?? '').replace(/^@/, '')}
				{@const isEntity = url ? parseEntityRoute(url) !== null : false}
				{#if isEntity}
					<!-- `![@X](/entity/id)` — entities are always inline links now (no
					     card), same as the editor. Streamdown parses it as an image;
					     we render the inline ref instead. -->
					<Ref displayName={label} {url} variant={refVariant} />
				{:else}
					<img src={url ? backendUrl(url) : url} alt={token?.text ?? ''} loading="lazy" />
				{/if}
			{/snippet}
	</Streamdown>

	{#if cards.open && openSources.length}
		<RecordCard
			anchor={cards.open.anchor}
			sources={openSources}
			sentence={cards.open.sentence}
			label={openSources[0].title}
			mode={cards.open.mode}
			position={cards.open.position}
			focus={cards.open.focus}
			onopen={openSource}
			onstep={(by) => cards.step(by)}
			onclose={(back) => cards.close(back)}
		/>
	{/if}
</div>

<style>
	/* Scoped to this component's own .markdown div (Svelte hashes the class). */
	.markdown {
		display: block;
	}

	.markdown :global(.streamdown-content) {
		display: block;
	}

	/* Each list item is its own layout root while a reply streams in. */
	.markdown :global(li) {
		contain: layout;
	}

	/* Streamdown fades each word in its own span and makes it inline-block,
	   which turns the space after a word into a box of its own: a line that
	   wrapped on a space started with it, indented. Opacity animates on
	   inline boxes just as well. The style is inline, hence the !important. */
	.markdown :global(.streamdown-content span[style*="animation-name: sd-"]) {
		display: inline !important;
	}

	@media (prefers-reduced-motion: reduce) {
		.markdown :global(.streamdown-content [style*="animation-name: sd-"]) {
			animation: none !important;
			opacity: 1 !important;
		}
	}
</style>
