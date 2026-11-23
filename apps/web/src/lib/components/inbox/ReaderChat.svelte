<script lang="ts" module>
	import { SvelteMap } from 'svelte/reactivity';

	/** What you wrote into a box that is a mockup, by the thing it was about, while the page is open. */
	const written = new SvelteMap<string, string[]>();
</script>

<script lang="ts">
	/**
	 * The box that ends a thing's history on the Inbox's right side, for
	 * writing about it. What you write belongs to the thing's own history,
	 * as a comment does on an issue.
	 *
	 * On a task it is real: `onSend` saves what you write as a note, or as a
	 * reply to what the assistant left, and the task's history shows it. On
	 * anything else it is a mockup, to see how writing about an insight, a
	 * question or a thread would sit on the page: what you write shows above
	 * the box, says so, and goes nowhere.
	 *
	 * `inset` gives it the right side's own margins, for a host whose content
	 * does not have them.
	 */
	import { tick } from 'svelte';
	import TaskComposer from '$lib/components/tasks/TaskComposer.svelte';
	import { reducedMotion } from '$lib/stores/motion.svelte';

	let {
		placeholder,
		about,
		inset = false,
		onSend,
	}: {
		placeholder: string;
		inset?: boolean;
		/** The thing's key: a new thing starts an empty box. */
		about: string;
		/** Save what you wrote. Resolve true to clear the box. Unset, the box is a mockup. */
		onSend?: (text: string) => Promise<boolean>;
	} = $props();

	const mine = $derived(onSend ? [] : (written.get(about) ?? []));
	let box = $state<HTMLDivElement | null>(null);

	async function send(text: string): Promise<boolean> {
		const said = text.trim();
		if (!said) return false;
		let sent = true;
		if (onSend) sent = await onSend(said);
		else written.set(about, [...(written.get(about) ?? []), said]);
		if (sent) void showLatest();
		return sent;
	}

	/** What you just wrote lands at the foot of the side, so it scrolls there, as a chat does. */
	async function showLatest() {
		await tick();
		box?.scrollIntoView({ block: 'nearest', behavior: reducedMotion() ? 'auto' : 'smooth' });
	}
</script>

<div class="chat" class:inset>
<div class="log" aria-live="polite">
	{#each mine as text, i (i)}
		<div class="said">
			<span class="who">You wrote</span>
			<p class="text">{text}</p>
		</div>
	{/each}
	{#if mine.length > 0}
		<p class="mock">This box is a mockup here. The assistant doesn't read it yet, and it clears when you leave the page.</p>
	{/if}
</div>
<div class="box" bind:this={box}>
	{#key about}
		<TaskComposer label="Write about this" {placeholder} sendLabel="Send" onCommit={send} />
	{/key}
</div>
</div>

<style>
	.chat {
		display: flex;
		flex-direction: column;
		gap: 16px;
	}
	.chat.inset {
		padding: 0 40px 40px;
	}
	.log {
		display: flex;
		flex-direction: column;
		gap: 12px;
	}
	.log:empty {
		display: none;
	}
	.said {
		display: flex;
		flex-direction: column;
		gap: 4px;
		align-self: flex-end;
		max-width: 80%;
		padding: 8px 12px;
		border-radius: var(--radius-md);
		background: var(--fill-bg);
	}
	.who {
		font-size: var(--text-caption);
		line-height: 16px;
		color: var(--color-foreground-muted);
	}
	.text {
		margin: 0;
		font-family: var(--font-serif);
		font-size: var(--text-body);
		line-height: 24px;
		color: var(--color-foreground);
		white-space: pre-wrap;
	}
	.mock {
		margin: 0;
		font-size: var(--text-small);
		line-height: 20px;
		color: var(--color-foreground-muted);
	}
	/* A card at the end of the history, as a comment box is. */
	.box {
		padding: 12px 16px;
		border: 1px solid var(--color-border-subtle);
		border-radius: var(--radius-md);
		background: var(--color-page);
	}
</style>
