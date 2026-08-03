<script lang="ts">
	import { browser } from '$app/environment';
	import { Streamdown } from 'svelte-streamdown';
	import type { CitationContext, Citation } from '$lib/types/Citation';
	import InlineCitation from './citations/InlineCitation.svelte';
	import Ref from './Ref.svelte';
	import LinkChip from './LinkChip.svelte';
	import MarkdownCodeBlock from './MarkdownCodeBlock.svelte';
	import { parseEntityRoute } from '$lib/utils/refRoutes';
	import { preprocessMarkdown } from '$lib/utils/markdownPreprocess';
	import type { BundledTheme } from 'shiki';

	interface Props {
		content: string;
		isStreaming?: boolean;
		citations?: CitationContext;
		onCitationClick?: (citation: Citation) => void;
		// How inline entity refs render: "link" (default, chat answers) or "quiet"
		// (dotted-underline prose links for flowing text like the day biography).
		refVariant?: "link" | "quiet";
	}

	let { content, isStreaming = false, citations, onCitationClick, refVariant = "link" }: Props =
		$props();

	// Read Shiki theme from CSS variable (defined in themes.css)
	function getShikiTheme(): BundledTheme {
		if (!browser) return 'github-light';
		const theme = getComputedStyle(document.documentElement).getPropertyValue('--shiki-theme').trim();
		return (theme || 'github-light') as BundledTheme;
	}

	let currentShikiTheme = $state<BundledTheme>(getShikiTheme());

	// Update when theme changes
	$effect(() => {
		if (!browser) return;
		const handleThemeChange = () => {
			currentShikiTheme = getShikiTheme();
		};
		window.addEventListener('themechange', handleThemeChange);
		return () => window.removeEventListener('themechange', handleThemeChange);
	});

	// Helper to get Citation from token key
	function getCitation(key: string): Citation | undefined {
		return citations?.byId.get(key);
	}

	const processedContent = $derived.by(() => {
		if (!content) return '';
		// Fix adjacent citations [1][2] -> [1] [2]
		return preprocessMarkdown(content.replace(/\](\[\d+\])/g, '] $1'));
	});

	// Convert CitationContext to Streamdown's sources format
	const sources = $derived.by(() => {
		if (!citations) return {};
		const result: Record<string, { title: string; url?: string; content?: string }> = {};
		for (const [id, citation] of citations.byId) {
			result[id] = {
				title: citation.title || citation.label || 'Source',
				url: citation.url,
				content: citation.preview
			};
		}
		return result;
	});

	// Get current origin for relative URL handling
	const origin = browser ? window.location.origin : 'https://app.local';

	// Streamdown theme
	const customTheme = {
		code: {
			base: 'my-4 w-full overflow-hidden rounded-xl border border-border-subtle flex flex-col',
			container: '',
			header: 'flex items-center justify-between px-4 py-2 text-foreground-muted text-xs font-mono bg-surface-elevated',
			languageLabel: 'text-foreground-muted font-medium',
			copyButton: 'px-2 py-1 rounded hover:bg-border/50 transition-colors text-foreground-muted',
			copyIcon: 'w-4 h-4',
			pre: 'overflow-x-auto p-4 text-sm bg-surface-elevated',
			skeleton: 'block text-foreground bg-transparent animate-none',
			downloadButton: 'px-2 py-1 rounded hover:bg-border/50 transition-colors text-foreground-muted',
			downloadIcon: 'w-4 h-4'
		},
		// Cells are rendered by streamdown's default td/th components (so inline
		// markdown — **bold**, links, `code` — renders); we only style them here.
		// The `table` snippet below supplies the scrolling wrapper.
		thead: { base: 'bg-surface-elevated' },
		tbody: { base: '' },
		tr: { base: '' },
		th: { base: 'px-3 py-2 text-left font-medium border-b border-border-subtle' },
		td: { base: 'px-3 py-2 align-top border-b border-border-subtle/50' }
	};
</script>

{#if browser}
	<div class="markdown cited-markdown">
		<Streamdown
			content={processedContent}
			{sources}
			inlineCitationsMode="list"
			class="streamdown-content"
			shikiTheme={currentShikiTheme}
			parseIncompleteMarkdown={isStreaming}
			theme={customTheme}
			controls={{ table: true }}
			defaultOrigin={origin}
			allowedLinkPrefixes={['*']}
			animation={{
				enabled: isStreaming,
				type: 'fade',
				duration: 300,
				tokenize: 'word',
				animateOnMount: false
			}}
		>
			{#snippet inlineCitationPreview({ token })}
				{@const citation = getCitation(token.keys[0])}
				<InlineCitation
					citationId={token.keys[0]}
					{citation}
					onPanelOpen={onCitationClick}
				/>
			{/snippet}

			{#snippet inlineCitationPopover()}
				<!-- Empty - we use CitationPanel at page level instead -->
			{/snippet}

			{#snippet table({ children }: { children: import('svelte').Snippet })}
				<div class="my-4 w-full overflow-x-auto rounded-xl border border-border-subtle">
					<table class="w-full border-collapse text-sm">{@render children()}</table>
				</div>
			{/snippet}

			{#snippet code({ token }: { token: any })}
				<MarkdownCodeBlock {token} {isStreaming} />
			{/snippet}

			{#snippet link({ children, token }: { children: import('svelte').Snippet; token: any })}
				{@const url = token?.href}
				{@const isEntity = url ? parseEntityRoute(url) !== null : false}
				{@const isExternal = url ? /^https?:\/\//.test(url) : false}
				{#if isEntity}
					<Ref displayName={token.text} url={url} variant={refVariant} />
				{:else if isExternal}
					<!-- Always quiet, even where entity refs are loud. A web
					     citation is metadata about the sentence, not part of it,
					     and an answer that cites six of them was ending every
					     bullet in a row of blue — the paragraph read as a link
					     farm with prose in between. Dotted, body-colored, no
					     globe: the domain name is already the tell, and hovering
					     names the site in full. -->
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
					<img src={url} alt={token?.text ?? ''} loading="lazy" style="max-width: 100%; height: auto;" />
				{/if}
			{/snippet}
		</Streamdown>
	</div>
{:else}
	<div class="markdown markdown-ssr">
		<pre class="whitespace-pre-wrap text-foreground" style="line-height: 1.8;">{content}</pre>
	</div>
{/if}

<style>
	.cited-markdown {
		display: block;
	}

	.cited-markdown :global(.streamdown-content) {
		display: block;
	}

	.cited-markdown :global([data-streamdown-citation-preview]) {
		all: unset;
		display: inline;
	}

	.cited-markdown :global([data-streamdown-citation-popover]) {
		display: none !important;
	}
</style>
