<script lang="ts">
	/** Modal, opened from a button, with a footer of actions. */
	import Button from './Button.svelte';
	import Modal from './Modal.svelte';

	let open = $state(false);
</script>

<Button variant="secondary" onclick={() => (open = true)}>Open a dialog</Button>

<Modal {open} onClose={() => (open = false)} title="Rename this page" width="sm">
	<p class="body">The new name shows in the tab, the sidebar and search.</p>
	{#snippet footer()}
		<Button variant="ghost" onclick={() => (open = false)}>Cancel</Button>
		<Button onclick={() => (open = false)}>Rename</Button>
	{/snippet}
</Modal>

<style>
	.body {
		margin: 0;
		font-size: var(--text-label);
		color: var(--color-foreground-muted);
	}
</style>
