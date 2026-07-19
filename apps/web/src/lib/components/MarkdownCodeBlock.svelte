<script lang="ts">
	import { browser } from '$app/environment';
	import { highlightCode, getThemeFromCSS } from '$lib/shiki/highlighter';
	import type { ThemedToken } from 'shiki';

	// Custom renderer for svelte-streamdown `code` tokens. We override the default
	// (which re-runs Shiki on the whole block every streaming delta → O(n²) flicker)
	// and instead render PLAIN monospace while the message is still streaming, then
	// highlight once — when the turn finishes.
	//
	// Why gate on `!isStreaming` rather than a closing-fence regex: streamdown's
	// `parseIncompleteMarkdown` can synthesize a closing fence for an open block
	// mid-stream, which would make a fence check report "closed" and re-highlight on
	// every token — the thrash we're removing. Finishing the turn is the robust,
	// thrash-free signal. (Per-block fence-close could be added later if we confirm
	// raw-fence semantics under parseIncompleteMarkdown.)
	let { token, isStreaming = false }: { token: any; isStreaming?: boolean } = $props();

	const code = $derived((token?.text ?? '') as string);
	const lang = $derived((token?.lang ?? '') as string);
	const ready = $derived(!isStreaming && !!code);

	let tokens = $state<ThemedToken[][] | null>(null);
	let copied = $state(false);

	// Track active theme so we re-highlight on theme toggle.
	let themeKey = $state(browser ? getThemeFromCSS() : '');
	$effect(() => {
		if (!browser) return;
		const onChange = () => (themeKey = getThemeFromCSS());
		window.addEventListener('themechange', onChange);
		return () => window.removeEventListener('themechange', onChange);
	});

	// Highlight once the turn is done (and re-run if code/lang/theme change).
	$effect(() => {
		const c = code;
		const l = lang;
		void themeKey; // dependency: re-highlight on theme change
		if (!ready) {
			tokens = null;
			return;
		}
		let cancelled = false;
		highlightCode(c, l).then((res) => {
			if (!cancelled) tokens = res;
		});
		return () => {
			cancelled = true;
		};
	});

	async function copy() {
		try {
			await navigator.clipboard.writeText(code);
			copied = true;
			setTimeout(() => (copied = false), 1500);
		} catch {
			/* clipboard unavailable */
		}
	}
</script>

<div
	class="my-4 w-full overflow-hidden rounded-xl border border-border-subtle flex flex-col"
>
	<div
		class="flex items-center justify-between px-4 py-2 text-foreground-muted text-xs font-mono bg-surface-elevated"
	>
		<span class="text-foreground-muted font-medium">{lang || ''}</span>
		<button
			type="button"
			onclick={copy}
			class="px-2 py-1 rounded hover:bg-border/50 transition-colors text-foreground-muted"
			aria-label="Copy code"
		>
			{copied ? 'Copied' : 'Copy'}
		</button>
	</div>

	{#if tokens}
		<pre class="overflow-x-auto p-4 text-sm bg-surface-elevated"><code
				>{#each tokens as line, i}{#each line as t}<span
							style:color={t.color}>{t.content}</span
						>{/each}{#if i < tokens.length - 1}{'\n'}{/if}{/each}</code
			></pre>
	{:else}
		<pre
			class="overflow-x-auto p-4 text-sm bg-surface-elevated"><code>{code}</code></pre>
	{/if}
</div>
