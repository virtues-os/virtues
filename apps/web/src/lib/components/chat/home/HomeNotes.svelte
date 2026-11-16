<script lang="ts">
	/**
	 * Home's notes line, between the message box and the tiles. It shows one
	 * note at a time, with its action,
	 * "Not now" and "Next". Notes come from three places, most pressing first:
	 * what the server says needs you (`attention`, the ∴ inbox's list), chats
	 * with a reply you haven't read, and the box's suggested question. "Not
	 * now" hides a note until the app reloads; a condition the server still
	 * sees comes back then. Nothing here counts.
	 */
	import { onMount } from 'svelte';
	import { fade } from 'svelte/transition';
	import type { SuggestedQuestion } from '$lib/api/client';
	import { attention } from '$lib/stores/attention.svelte';
	import { chatSessions } from '$lib/stores/chatSessions.svelte';
	import { windowShellStore } from '$lib/stores/window-shell.svelte';

	type Note = { key: string; text: string; action: string; run: () => void; dismiss?: () => void };

	let {
		note = null,
		onAsk,
		onDismiss,
		onShown
	}: {
		note?: SuggestedQuestion | null;
		onAsk?: (note: SuggestedQuestion) => void;
		onDismiss?: (note: SuggestedQuestion) => void;
		onShown?: (note: SuggestedQuestion) => void;
	} = $props();

	onMount(() => attention.start());

	let hidden = $state<string[]>([]);
	let at = $state(0);

	const notes = $derived.by(() => {
		const out: Note[] = [];
		for (const item of attention.items) {
			out.push({
				key: `attention:${item.key}`,
				text: item.title,
				action: 'Open',
				run: () => windowShellStore.openTabFromRoute(item.route, { label: item.title, focusExisting: true })
			});
		}
		for (const s of chatSessions.sessions.filter((s) => s.unread).slice(0, 3)) {
			const title = s.title?.trim() || 'a chat';
			out.push({
				key: `reply:${s.conversation_id}`,
				text: `New reply in ${title}`,
				action: 'Open',
				run: () => windowShellStore.openTabFromRoute(`/chat/${s.conversation_id}`, { label: title, focusExisting: true })
			});
		}
		if (note) {
			const n = note;
			out.push({ key: `ask:${n.id}`, text: n.question, action: 'Ask', run: () => onAsk?.(n), dismiss: () => onDismiss?.(n) });
		}
		return out.filter((n) => !hidden.includes(n.key));
	});

	const current = $derived(notes.length ? notes[at % notes.length] : null);

	function notNow(n: Note) {
		n.dismiss?.();
		hidden = [...hidden, n.key];
	}

	let reported: string | null = null;
	$effect(() => {
		if (note && current?.key === `ask:${note.id}` && reported !== note.id) {
			reported = note.id;
			onShown?.(note);
		}
	});
</script>

{#if current}
	{#key current.key}
		<p class="note" in:fade={{ duration: 200 }}>
			<span class="dot" aria-hidden="true"></span>
			<span class="text">{current.text}</span>
			<button class="act" onclick={current.run}>{current.action}</button>
			<button class="later" onclick={() => notNow(current)}>Not now</button>
			{#if notes.length > 1}
				<button class="later" onclick={() => (at = (at + 1) % notes.length)}>Next</button>
			{/if}
		</p>
	{/key}
{/if}

<style>
	.note {
		width: min(var(--reading-measure, 38rem), calc(100% - 2 * var(--reading-gutter, 1.5rem)));
		box-sizing: border-box;
		padding-left: 4px;
		margin: 0;
		pointer-events: auto;
		display: flex;
		align-items: baseline;
		gap: 12px;
		font-size: 14px;
		color: var(--color-foreground);
		min-width: 0;
	}

	.dot {
		flex-shrink: 0;
		width: 6px;
		height: 6px;
		border-radius: 50%;
		background: var(--color-primary);
		transform: translateY(-1px);
	}

	.text {
		min-width: 0;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.act,
	.later {
		flex-shrink: 0;
		border: 0;
		padding: 0;
		background: none;
		font: inherit;
		cursor: pointer;
	}

	.act {
		font-weight: 500;
		color: var(--color-foreground);
	}

	.later {
		color: var(--color-foreground-subtle);
	}

	.act:hover,
	.later:hover {
		opacity: 0.65;
	}

</style>
