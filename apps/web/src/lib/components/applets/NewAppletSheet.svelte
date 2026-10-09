<script lang="ts">
	import Modal from '$lib/components/Modal.svelte';
	import TextAction from '$lib/components/TextAction.svelte';
	import IconButton from '$lib/components/IconButton.svelte';
	import AtlasIcon from '$lib/components/sidebar/AtlasIcon.svelte';
	import { askVirtues } from '$lib/stores/pendingPrompt.svelte';

	/**
	 * Making an applet starts as a chat: you say what you want, and Virtues
	 * builds it in the conversation. The examples fill the box rather than
	 * sending, so each one is a starting point you can change before you ask.
	 */
	let {
		open,
		onClose,
		onImport
	}: {
		open: boolean;
		onClose: () => void;
		/** Bring applets in from a Git repository instead. */
		onImport: () => void;
	} = $props();

	// Each with the glyph of where its work would go, as its row on the
	// Applets page would show it.
	const EXAMPLES: { name: string; note: string; glyph: string; ask: string }[] = [
		{
			name: 'Weekly summary',
			note: 'A page every Sunday',
			glyph: 'pages',
			ask: 'Every Sunday evening, write a page that sums up my week.'
		},
		{
			name: 'Food log',
			note: 'You tell it, it keeps the count',
			glyph: 'dashboard',
			ask: "Keep a food log. I'll tell you what I eat in plain words, and you keep a daily calorie total."
		},
		{
			name: 'One-time reminder',
			note: 'Runs once, then finishes',
			glyph: 'chats',
			ask: 'Remind me on the 25th to renew the car registration.'
		},
		{
			name: 'Workout nudge',
			note: 'Checks quietly, speaks up when needed',
			glyph: 'chats',
			ask: 'If I go three days without a workout, tell me. Not more than once a day.'
		}
	];

	let draft = $state('');

	function send() {
		const text = draft.trim();
		if (!text) return;
		askVirtues(text);
		draft = '';
		onClose();
	}
</script>

<Modal {open} {onClose} title="New applet" width="md">
	<div class="sheet">
		<p class="lede">Describe what it should do and when. Virtues sets it up in a new chat.</p>
		<form
			class="ask"
			onsubmit={(e) => {
				e.preventDefault();
				send();
			}}
		>
			<input
				type="text"
				bind:value={draft}
				placeholder="Every Sunday evening, write a page that sums up my week"
				aria-label="What the applet should do"
			/>
			<IconButton
				icon="ri:arrow-up-line"
				label="Start the chat"
				variant="secondary"
				class="send"
				disabled={!draft.trim()}
				onclick={send}
			/>
		</form>
		<h3 class="examples-head">Or start from one of these</h3>
		<ul class="examples" role="list">
			{#each EXAMPLES as ex (ex.name)}
				<li>
					<button type="button" class="example" onclick={() => (draft = ex.ask)}>
						<span class="glyph" aria-hidden="true"><AtlasIcon name={ex.glyph} size={16} bare /></span>
						<span class="example-body">
							<span class="example-name">{ex.name}</span>
							<span class="example-note">{ex.note}</span>
						</span>
					</button>
				</li>
			{/each}
		</ul>
	</div>
	{#snippet footer()}
		<span class="import"><TextAction quiet onclick={onImport}>Import from Git</TextAction></span>
	{/snippet}
</Modal>

<style>
	.sheet {
		display: flex;
		flex-direction: column;
		gap: 12px;
	}
	.lede {
		margin: 0;
		font-size: 15px;
		color: var(--color-foreground-muted);
	}
	/* The ask, shaped like the chat composer it hands off to. */
	.ask {
		display: flex;
		align-items: center;
		gap: 8px;
		padding: 4px 4px 4px 16px;
		border: 1px solid var(--color-border);
		border-radius: 999px;
		background: var(--color-surface);
	}
	.ask input {
		flex: 1;
		min-width: 0;
		border: 0;
		background: none;
		font: inherit;
		font-size: 15px;
		color: var(--color-foreground);
		outline: none;
	}
	.ask :global(.v-iconbtn.send) {
		width: 32px;
		height: 32px;
		border-radius: 999px;
	}
	.examples-head {
		margin: 8px 0 0;
		font-family: var(--font-sans);
		font-size: 13px;
		font-weight: 500;
		color: var(--color-foreground-muted);
	}
	.examples {
		list-style: none;
		margin: 0;
		padding: 0;
		display: flex;
		flex-direction: column;
		gap: 8px;
	}
	.example {
		display: flex;
		align-items: center;
		gap: 12px;
		width: 100%;
		padding: 12px;
		border: 1px solid var(--color-border);
		border-radius: 12px;
		background: var(--color-surface);
		text-align: left;
		font: inherit;
		cursor: pointer;
		color: var(--color-foreground);
	}
	.example:hover,
	.example:focus-visible {
		border-color: var(--color-border-strong);
	}
	.glyph {
		width: 32px;
		height: 32px;
		flex: none;
		display: inline-flex;
		align-items: center;
		justify-content: center;
		border-radius: 6px;
		background: color-mix(in srgb, var(--color-foreground) 6%, transparent);
		color: var(--color-foreground-muted);
	}
	.example-body {
		display: flex;
		flex-direction: column;
		gap: 4px;
	}
	.example-name {
		font-size: 15px;
	}
	.example-note {
		font-size: 13px;
		color: var(--color-foreground-muted);
	}
	.import {
		margin-right: auto;
	}
</style>
