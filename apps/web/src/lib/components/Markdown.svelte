<script lang="ts">
	import { browser } from '$app/environment';
	import { Streamdown } from 'svelte-streamdown';
	import type { BundledTheme } from 'shiki';
	import MarkdownCodeBlock from './MarkdownCodeBlock.svelte';
	import { preprocessMarkdown } from '$lib/utils/markdownPreprocess';

	interface Props {
		content: string;
		isStreaming?: boolean;
	}

	let { content, isStreaming = false }: Props = $props();

	const processedContent = $derived(preprocessMarkdown(content));

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
		// Cells rendered by streamdown's default td/th (inline markdown works); we
		// only style them. The `table` snippet supplies the scrolling wrapper.
		thead: { base: 'bg-surface-elevated' },
		tbody: { base: '' },
		tr: { base: '' },
		th: { base: 'px-3 py-2 text-left font-medium border-b border-border-subtle' },
		td: { base: 'px-3 py-2 align-top border-b border-border-subtle/50' }
	};
</script>

{#if browser}
	<div class="markdown">
		<Streamdown
			content={processedContent}
			class="streamdown-content"
			shikiTheme={currentShikiTheme}
			parseIncompleteMarkdown={isStreaming}
			theme={customTheme}
			controls={{ table: true }}
			allowedLinkPrefixes={['*']}
			animation={{
				enabled: isStreaming,
				type: 'fade',
				duration: 300,
				tokenize: 'word',
				animateOnMount: false
			}}
		>
			{#snippet table({ children }: { children: import('svelte').Snippet })}
				<div class="my-4 w-full overflow-x-auto rounded-xl border border-border-subtle">
					<table class="w-full border-collapse text-sm">{@render children()}</table>
				</div>
			{/snippet}

			{#snippet code({ token }: { token: any })}
				<MarkdownCodeBlock {token} {isStreaming} />
			{/snippet}
		</Streamdown>
	</div>
{:else}
	<!-- SSR fallback: plain text with basic styling -->
	<div class="markdown markdown-ssr">
		<pre class="whitespace-pre-wrap text-foreground" style="line-height: 1.8;">{content}</pre>
	</div>
{/if}
