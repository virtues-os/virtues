<script lang="ts">
	/**
	 * DialogHost — renders whatever confirm/prompt is pending on dialogStore.
	 * Mounted once in the app layout; call sites use confirmAction/promptText
	 * and never touch this component.
	 */
	import Modal from '$lib/components/Modal.svelte';
	import Button from '$lib/components/Button.svelte';
	import { dialogStore } from '$lib/stores/dialog.svelte';
	import { contextMenu } from '$lib/stores/contextMenu.svelte';
	import { tick } from 'svelte';

	const pending = $derived(dialogStore.pending);

	let value = $state('');
	let inputEl = $state<HTMLInputElement | null>(null);

	// Seed + focus the field each time a text dialog opens.
	$effect(() => {
		const p = dialogStore.pending;
		if (p?.kind === 'prompt') {
			value = p.initialValue ?? '';
			tick().then(() => inputEl?.select());
		}
	});

	// A dialog raised from a context-menu action would otherwise sit on top of
	// the still-open menu: executeAction awaits the action before hiding, and
	// the action is now awaiting us. Dismiss the menu as we take over.
	$effect(() => {
		if (dialogStore.pending) contextMenu.hide();
	});

	/** A confirm marked danger; a prompt never is. */
	const isDanger = $derived(pending?.kind === 'confirm' && pending.danger);

	function accept() {
		dialogStore.accept(pending?.kind === 'prompt' ? value : undefined);
	}
</script>

<Modal open={!!pending} title={pending?.title} width="sm" onClose={() => dialogStore.cancel()}>
	{#if pending?.body}
		<p class="dialog-body">{pending.body}</p>
	{/if}
	{#if pending?.kind === 'prompt'}
		<!-- svelte-ignore a11y_autofocus -->
		<input
			bind:this={inputEl}
			bind:value
			class="modal-input"
			class:with-body={!!pending.body}
			placeholder={pending.placeholder ?? ''}
			autofocus
			onkeydown={(e) => {
				if (e.key === 'Enter') { e.preventDefault(); accept(); }
			}}
		/>
	{/if}
	{#snippet footer()}
		<Button variant="secondary" size="sm" onclick={() => dialogStore.cancel()}>
			{pending?.cancelLabel ?? 'Cancel'}
		</Button>
		<Button
			variant={isDanger ? 'danger' : 'primary'}
			size="sm"
			disabled={pending?.kind === 'prompt' && !value.trim()}
			onclick={accept}
		>
			{pending?.confirmLabel ?? (pending?.kind === 'prompt' ? 'Create' : 'Confirm')}
		</Button>
	{/snippet}
</Modal>

<style>
	.dialog-body {
		margin: 0;
		font-size: 0.9rem;
		line-height: 1.55;
		color: var(--color-foreground-muted);
	}
	.with-body { margin-top: 14px; }
</style>
