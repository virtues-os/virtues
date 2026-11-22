<script lang="ts">
	import Modal from '$lib/components/Modal.svelte';
	import IconButton from '$lib/components/IconButton.svelte';
	import AtlasIcon from '$lib/components/sidebar/AtlasIcon.svelte';
	import { askVirtues } from '$lib/stores/pendingPrompt.svelte';

	/**
	 * Making a site starts as a chat: you say what you want to see or keep
	 * track of, and Virtues builds it in the conversation. The examples fill
	 * the box and do not send, so each one is a starting point you can
	 * change before you ask.
	 *
	 * Every example is a site: a page you look at. A task for the assistant
	 * that repeats is asked for in a chat and shows in Tasks.
	 */
	let {
		open,
		onClose
	}: {
		open: boolean;
		onClose: () => void;
	} = $props();

	const EXAMPLES: { name: string; note: string; ask: string }[] = [
		{
			name: 'Sleep chart',
			note: 'Your nights, drawn from your record',
			ask: 'Make a page that charts how long I slept each night for the last month.'
		},
		{
			name: 'Water log',
			note: 'You tell it, the page keeps the record',
			ask: "Keep a water log. I'll tell you when I drink a glass, and you show each day on a page."
		},
		{
			name: 'Reading list',
			note: 'A table you add to',
			ask: 'Make a reading list I can add books to, with where I am in each one.'
		},
		{
			name: 'Trip budget',
			note: 'What you planned and what you spent',
			ask: 'Make a page for a trip budget: what I planned to spend and what I have spent so far.'
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

<Modal {open} {onClose} title="New site" width="md">
	<div class="sheet">
		<p class="lede">Describe what you want to see or keep track of. Virtues builds it in a new chat.</p>
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
				placeholder="A page that charts how long I slept each night"
				aria-label="What the site should show"
			/>
			<IconButton
				icon="mynaui:arrow-up"
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
						<span class="glyph" aria-hidden="true"><AtlasIcon name="dashboard" size={16} bare /></span>
						<span class="example-body">
							<span class="example-name">{ex.name}</span>
							<span class="example-note">{ex.note}</span>
						</span>
					</button>
				</li>
			{/each}
		</ul>
	</div>
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
		border-radius: var(--radius-full);
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
		border-radius: var(--radius-full);
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
		border-radius: var(--radius-xl);
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
		border-radius: var(--radius-md);
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
</style>
