<script lang="ts">
	/**
	 * Home's notes line, between the message box and the tiles. It shows one
	 * note at a time, with its action,
	 * "Not now" and "Next". Notes come from three places, most pressing first:
	 * what the server says needs you (`attention`, the ∴ inbox's list), what
	 * it noticed about your tasks and threads (`notes`), and chats with a
	 * reply you haven't read. "Not now" on a
	 * task or thread note sets it aside on the server until its condition
	 * changes; on the others it hides the note until the app reloads.
	 * Nothing here counts.
	 */
	import { onMount } from 'svelte';
	import { fade } from '$lib/utils/transitions';
	import { attention } from '$lib/stores/attention.svelte';
	import { notesStore } from '$lib/stores/notes.svelte';
	import { chatSessions } from '$lib/stores/chatSessions.svelte';
	import { windowShellStore } from '$lib/stores/window-shell.svelte';

	type Note = { key: string; text: string; action: string; run: () => void; dismiss?: () => void };

	onMount(() => {
		attention.start();
		notesStore.start();
		return () => {
			attention.stop();
			notesStore.stop();
		};
	});

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
		for (const n of notesStore.items) {
			out.push({
				key: `note:${n.key}`,
				text: n.text,
				action: n.action,
				run: () => windowShellStore.openTabFromRoute(n.action_route, { focusExisting: true }),
				dismiss: () => void notesStore.dismiss(n)
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
		return out.filter((n) => !hidden.includes(n.key));
	});

	const current = $derived(notes.length ? notes[at % notes.length] : null);

	// A server note leaves the store when set aside and comes back with its
	// Undo, so only the others are hidden here.
	function notNow(n: Note) {
		n.dismiss?.();
		if (!n.key.startsWith('note:')) hidden = [...hidden, n.key];
	}

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
	/* It sits on the tiles' top row, beside their switch (HomeTiles). The
	   dot is close to its words, the actions a little further off. */
	.note {
		flex: 1;
		min-width: 0;
		margin: 0;
		pointer-events: auto;
		display: flex;
		align-items: baseline;
		gap: 8px;
		font-size: 14px;
		color: var(--color-foreground);
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
		margin-left: 4px;
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
