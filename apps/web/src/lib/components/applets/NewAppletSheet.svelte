<script lang="ts">
	import Modal from '$lib/components/Modal.svelte';
	import Button from '$lib/components/Button.svelte';
	import TextAction from '$lib/components/TextAction.svelte';
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

	const EXAMPLES: { name: string; ask: string }[] = [
		{
			name: 'Weekly summary',
			ask: 'Every Sunday evening, write a page that sums up my week.'
		},
		{
			name: 'Food log',
			ask: "Keep a food log. I'll tell you what I eat in plain words, and you keep a daily calorie total."
		},
		{
			name: 'One-time reminder',
			ask: 'Remind me on the 25th to renew the car registration.'
		},
		{
			name: 'Workout nudge',
			ask: "If I go three days without a workout, tell me. Not more than once a day."
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
		<textarea
			rows="3"
			bind:value={draft}
			placeholder="Every Sunday evening, write a page that sums up my week."
			aria-label="What the applet should do"
			onkeydown={(e) => {
				if (e.key === 'Enter' && (e.metaKey || e.ctrlKey)) {
					e.preventDefault();
					send();
				}
			}}
		></textarea>
		<h3 class="examples-head">Or start from one of these</h3>
		<ul class="examples" role="list">
			{#each EXAMPLES as ex (ex.name)}
				<li>
					<button type="button" class="example" onclick={() => (draft = ex.ask)}>
						<span class="example-name">{ex.name}</span>
						<span class="example-ask">{ex.ask}</span>
					</button>
				</li>
			{/each}
		</ul>
	</div>
	{#snippet footer()}
		<span class="import"><TextAction quiet onclick={onImport}>Import from Git</TextAction></span>
		<Button variant="primary" onclick={send} disabled={!draft.trim()}>Start the chat</Button>
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
	textarea {
		font: inherit;
		font-size: 15px;
		line-height: 1.5;
		padding: 12px 16px;
		border-radius: 12px;
		border: 1px solid var(--color-border);
		background: var(--color-surface);
		color: var(--color-foreground);
		resize: vertical;
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
	}
	.example {
		display: flex;
		flex-direction: column;
		gap: 4px;
		width: 100%;
		padding: 8px 12px;
		border: 0;
		border-radius: 6px;
		background: none;
		text-align: left;
		font: inherit;
		cursor: pointer;
		color: var(--color-foreground);
	}
	.example:hover,
	.example:focus-visible {
		background: color-mix(in srgb, var(--color-foreground) 5%, transparent);
	}
	.example-name {
		font-size: 14px;
		font-weight: 500;
	}
	.example-ask {
		font-size: 13px;
		color: var(--color-foreground-muted);
	}
	.import {
		margin-right: auto;
	}
</style>
