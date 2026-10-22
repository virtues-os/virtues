<script lang="ts">
	import { browser } from '$app/environment';
	import { highlightCode, getThemeFromCSS } from '$lib/shiki/highlighter';
	import IconButton from '$lib/components/IconButton.svelte';
	import { languageLabel } from '$lib/document/languages';
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

<!-- A page's code block's markup (prose.css `.prose-code`), so the two are one. -->
<div class="prose-code">
	<div class="prose-code-head">
		<span class="prose-code-lang">{languageLabel(lang) || 'Plain text'}</span>
		<IconButton icon={copied ? 'ri:check-line' : 'ri:file-copy-line'} label="Copy code" size="sm" onclick={copy} />
	</div>

	{#if tokens}
		<pre><code
				>{#each tokens as line, i}{#each line as t}<span
							style:color={t.color}>{t.content}</span
						>{/each}{#if i < tokens.length - 1}{'\n'}{/if}{/each}</code
			></pre>
	{:else}
		<pre><code>{code}</code></pre>
	{/if}
</div>
