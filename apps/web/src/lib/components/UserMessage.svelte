<script lang="ts">
	import { slide } from '$lib/utils/transitions';
	import Ref from './Ref.svelte';
	import Icon from './Icon.svelte';
	import { parseEntityRoute } from '$lib/utils/refRoutes';
	import { skills } from '$lib/stores/skills.svelte';

	let { text }: { text: string } = $props();

	// The user's message is shown as they typed it — plain text, whitespace
	// preserved — not as markdown. Rendering it through the answer renderer
	// turned a leading `- ` into a list, four spaces into a code block, and
	// zeroed paragraph gaps; the echo stopped matching the composer (VIR-333).
	// The one construct we do recognize is the @-mention pill, which the
	// composer serializes as `[Name](/person/id)` so the model sees the link.
	// That, and only that, comes back as a chip; any other bracket-paren text
	// stays literal. An insight's page counts too: "Write a report" on an
	// insight sends `[the insight](/insight/key)`. Messages sent before that
	// wrote `"the insight" (/insight/key)`, so that shape is read the same
	// way rather than showing the raw route.
	type Segment = { kind: 'text'; text: string } | { kind: 'ref'; name: string; url: string; quiet: boolean };

	const MENTION = /\[([^\]\n]+)\]\(([^)\s]+)\)|"([^"\n]+)"\s*\((\/[^)\s]+)\)/g;
	const INSIGHT_ROUTE = /^\/insight\/[^/\s]+$/;

	const linkable = (url: string) => parseEntityRoute(url) !== null || INSIGHT_ROUTE.test(url);

	function segment(src: string): Segment[] {
		const out: Segment[] = [];
		let last = 0;
		for (const m of src.matchAll(MENTION)) {
			const whole = m[0];
			const name = m[1] ?? m[3];
			const url = m[2] ?? m[4];
			const at = m.index ?? 0;
			if (!linkable(url)) continue;
			// A mention keeps its @ and type icon. An insight is quoted inside
			// a sentence, so it reads as words with a dotted line under them.
			const quiet = INSIGHT_ROUTE.test(url);
			if (at > last) out.push({ kind: 'text', text: src.slice(last, at) });
			out.push({ kind: 'ref', name, url, quiet });
			last = at + whole.length;
		}
		if (last < src.length) out.push({ kind: 'text', text: src.slice(last) });
		return out;
	}

	// A message sent with a skill starts with its `/name` (ChatInput), which
	// shows as the badge the composer drew. Any other leading slash is text.
	const SKILL = /^\/([a-z][a-z0-9-]{1,39})(?=\s|$)/;
	const skillName = $derived(SKILL.exec(text)?.[1] ?? null);
	$effect(() => {
		if (skillName && !skills.loaded) void skills.load();
	});
	const skill = $derived(
		skillName && skills.names.has(skillName)
			? (skills.items.find((s) => s.name === skillName) ?? skills.create ?? null)
			: null,
	);
	const body = $derived(skill ? text.slice(skillName!.length + 1).replace(/^ /, '') : text);
	const segments = $derived(segment(body));

	let isExpanded = $state(false);
	let lineCount = $state(0);
	let textContainer: HTMLDivElement | undefined = $state();
	const LINE_THRESHOLD = 8;

	// Calculate line count after mount
	$effect(() => {
		if (textContainer) {
			const lineHeight = parseFloat(getComputedStyle(textContainer).lineHeight);
			const height = textContainer.scrollHeight;
			lineCount = Math.round(height / lineHeight);
		}
	});

	const shouldTruncate = $derived(lineCount > LINE_THRESHOLD);
	const maxHeight = $derived(!isExpanded && shouldTruncate ? `${LINE_THRESHOLD * 1.5}rem` : 'none');
</script>

<div class="user-message-container">
	<!-- `text-foreground`, NOT `text-primary`. Tailwind's `text-primary` reads
	     like "the primary text color" and resolves to `--color-primary`, which
	     in this system is the ACCENT — the one blue, which means interactive
	     (themes.css). So every user message in the app was painted #0A84FF
	     while the assistant's reply beside it was ink, and the person's own
	     words looked like a link they could click. What someone typed is text,
	     and it is the same text as the answer under it. -->
	<div
		bind:this={textContainer}
		class="text-base text-foreground user-message-content"
		style="max-height: {maxHeight}; overflow: hidden;"
	>{#if skill}<span class="skill-label" title={skill.description}><Icon icon={skill.icon ?? 'mynaui:zap'} width="13" style="color: inherit" />/{skill.name}</span>{/if}{#each segments as seg, i (i)}{#if seg.kind === 'ref'}<Ref displayName={seg.name} url={seg.url} variant={seg.quiet ? 'quiet' : 'link'} />{:else}{seg.text}{/if}{/each}</div>

	{#if shouldTruncate}
		<button
			onclick={() => (isExpanded = !isExpanded)}
			class="block text-sm text-primary bg-surface-elevated rounded px-2 py-1 mt-2 transition-colors hover:bg-primary/15 cursor-pointer"
			transition:slide={{ duration: 200 }}
		>
			{isExpanded ? '↑ Show less' : '↓ Show more'}
		</button>
	{/if}
</div>

<style>
	/* What was typed, as typed: newlines, indentation, runs of spaces. The
	   container is written without inner whitespace so pre-wrap has nothing
	   of ours to preserve. */
	.user-message-content {
		white-space: pre-wrap;
		overflow-wrap: anywhere;
		line-height: 1.5;
	}

	/* The skill the message was sent with, as a small line over the words.
	   A filled chip inside the filled bubble read as a badge in a badge. */
	.skill-label {
		display: flex;
		align-items: center;
		gap: 0.25rem;
		margin-bottom: 0.125rem;
		font-size: 0.75rem;
		font-weight: 500;
		color: var(--color-foreground-muted);
		white-space: nowrap;
	}
</style>
