<script lang="ts">
	import { browser } from '$app/environment';
	import { Streamdown } from 'svelte-streamdown';
	import type { BundledTheme } from 'shiki';

	interface Props {
		content: string;
		isStreaming?: boolean;
	}

	let { content, isStreaming = false }: Props = $props();

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
	<div class="markdown">
		<Streamdown
			{content}
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
		/>
	</div>
{:else}
	<!-- SSR fallback: plain text with basic styling -->
	<div class="markdown markdown-ssr">
		<pre class="whitespace-pre-wrap text-foreground" style="line-height: 1.8;">{content}</pre>
	</div>
{/if}
