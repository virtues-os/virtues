<script lang="ts">
	import { browser } from '$app/environment';
	import { Streamdown } from 'svelte-streamdown';
	import type { CitationContext, Citation } from '$lib/types/Citation';
	import InlineCitation from './citations/InlineCitation.svelte';
	import EntityChip from './EntityChip.svelte';
	import type { BundledTheme } from 'shiki';

	interface Props {
		content: string;
		isStreaming?: boolean;
		citations?: CitationContext;
		onCitationClick?: (citation: Citation) => void;
	}

	let { content, isStreaming = false, citations, onCitationClick }: Props = $props();

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

	// Preprocess content to fix adjacent citations [1][2] -> [1] [2]
	// markedCitations rejects adjacent ][ as it looks like markdown link syntax
	const processedContent = $derived(content.replace(/\](\[\d+\])/g, '] $1'));

	// Convert CitationContext to Streamdown's sources format
	// Streamdown expects: { "1": { title, url, content }, "2": { ... } }
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

	// Streamdown theme (code blocks only - citations use custom snippets)
	const customTheme = {
		code: {
			base: 'my-4 w-full overflow-hidden rounded-xl border border-border-subtle flex flex-col',
			container: '',
			header: 'flex items-center justify-between px-4 py-2 text-foreground-muted text-xs font-mono bg-[var(--code-bg)]',
			languageLabel: 'text-foreground-muted font-medium',
			copyButton: 'px-2 py-1 rounded hover:bg-border/50 transition-colors text-foreground-muted',
			copyIcon: 'w-4 h-4',
			pre: 'overflow-x-auto p-4 text-sm bg-[var(--code-bg)]',
			skeleton: 'block text-[var(--code-fg)] bg-transparent animate-none',
			downloadButton: 'px-2 py-1 rounded hover:bg-border/50 transition-colors text-foreground-muted',
			downloadIcon: 'w-4 h-4'
		}
	};
</script>

{#if browser}
	<!-- Pass sources to Streamdown with custom citation UI -->
	<div class="markdown cited-markdown">
		<Streamdown
			content={processedContent}
			{sources}
			inlineCitationsMode="list"
			class="streamdown-content"
			shikiTheme={currentShikiTheme}
			parseIncompleteMarkdown={isStreaming}
			theme={customTheme}
			controls={{ table: false }}
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

		{#snippet link({ token }: { token: any })}
			{#if token.url?.startsWith('entity:')}
				<EntityChip 
					displayName={token.text} 
					entityId={token.url.replace('entity:', '')} 
				/>
			{:else if token.url}
				<a href={token.url} target="_blank" rel="noopener noreferrer">{token.text}</a>
			{:else}
				<span>{token.text}</span>
			{/if}
		{/snippet}
		</Streamdown>
	</div>
{:else}
	<!-- SSR fallback: plain text with basic styling -->
	<div class="markdown markdown-ssr">
		<pre class="whitespace-pre-wrap text-foreground" style="line-height: 1.8;">{processedContent}</pre>
	</div>
{/if}

<style>
	/* Container inherits normal block flow from markdown */
	.cited-markdown {
		display: block;
	}

	/* Ensure streamdown content flows properly */
	.cited-markdown :global(.streamdown-content) {
		display: block;
	}

	/* Reset Streamdown's wrapper button - we use our own InlineCitation styling */
	.cited-markdown :global([data-streamdown-citation-preview]) {
		all: unset;
		display: inline;
	}

	/* Hide Streamdown's popover - we use CitationPanel instead */
	.cited-markdown :global([data-streamdown-citation-popover]) {
		display: none !important;
	}
</style>
