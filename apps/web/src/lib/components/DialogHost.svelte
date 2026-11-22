<script lang="ts">
	/**
	 * DialogHost — renders whatever confirm, prompt or choice is pending on
	 * dialogStore. Mounted once in the app layout; call sites use
	 * confirmAction/promptText/chooseFrom and never touch this component.
	 */
	import Modal from '$lib/components/Modal.svelte';
	import Button from '$lib/components/Button.svelte';
	import Icon from '$lib/components/Icon.svelte';
	import { dialogStore } from '$lib/stores/dialog.svelte';
	import { contextMenu } from '$lib/stores/contextMenu.svelte';
	import { tick } from 'svelte';

	const pending = $derived(dialogStore.pending);

	let value = $state('');
	let inputEl = $state<HTMLInputElement | null>(null);

	// Seed + focus the field each time a text dialog or a choice opens.
	$effect(() => {
		const p = dialogStore.pending;
		if (p?.kind === 'prompt') {
			value = p.initialValue ?? '';
			tick().then(() => inputEl?.select());
		} else if (p?.kind === 'choose') {
			value = '';
			active = 0;
			tick().then(() => inputEl?.focus());
		}
	});

	// A choice: the options whose label holds what you typed, in their order.
	let active = $state(0);
	const matches = $derived.by(() => {
		if (pending?.kind !== 'choose') return [];
		const q = value.trim().toLowerCase();
		return q ? pending.options.filter((o) => o.label.toLowerCase().includes(q)) : pending.options;
	});
	$effect(() => {
		if (active >= matches.length) active = Math.max(0, matches.length - 1);
	});

	function onChooseKey(e: KeyboardEvent) {
		if (e.key === 'ArrowDown') {
			e.preventDefault();
			active = Math.min(active + 1, matches.length - 1);
		} else if (e.key === 'ArrowUp') {
			e.preventDefault();
			active = Math.max(active - 1, 0);
		} else if (e.key === 'Enter') {
			e.preventDefault();
			const pick = matches[active];
			if (pick) dialogStore.accept(pick.id);
		}
	}

	// A dialog raised from a context-menu action would otherwise sit on top of
	// the still-open menu: executeAction awaits the action before hiding, and
	// the action is now awaiting us. Dismiss the menu as we take over.
	$effect(() => {
		if (dialogStore.pending) contextMenu.hide();
	});

	/** A confirm marked danger; a prompt never is. */
	const isDanger = $derived(pending?.kind === 'confirm' && !!pending.danger);

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
	{#if pending?.kind === 'choose'}
		<!-- svelte-ignore a11y_autofocus -->
		<input
			bind:this={inputEl}
			bind:value
			class="modal-input"
			placeholder={pending.placeholder ?? 'Search'}
			autofocus
			role="combobox"
			aria-expanded="true"
			aria-controls="dialog-choices"
			aria-activedescendant={matches[active] ? `dialog-choice-${active}` : undefined}
			onkeydown={onChooseKey}
		/>
		<div class="choices" id="dialog-choices" role="listbox">
			{#each matches as option, i (option.id)}
				<button
					type="button"
					id="dialog-choice-{i}"
					class="choice"
					class:active={i === active}
					role="option"
					aria-selected={i === active}
					onmouseenter={() => (active = i)}
					onclick={() => dialogStore.accept(option.id)}
				>
					{#if option.icon}<Icon icon={option.icon} width="16" />{/if}
					<span class="choice-label">{option.label}</span>
					{#if option.checked}<Icon icon="mynaui:check" width="14" />{/if}
				</button>
			{:else}
				<p class="no-choice">Nothing matches "{value.trim()}".</p>
			{/each}
		</div>
	{/if}
	{#snippet footer()}
		<Button variant="secondary" onclick={() => dialogStore.cancel()}>
			{pending?.cancelLabel ?? 'Cancel'}
		</Button>
		{#if pending?.kind !== 'choose'}
		<Button
			variant={isDanger ? 'danger' : 'primary'}
			disabled={pending?.kind === 'prompt' && !value.trim()}
			onclick={accept}
		>
			{pending?.confirmLabel ?? (pending?.kind === 'prompt' ? 'Create' : 'Confirm')}
		</Button>
		{/if}
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
	.choices {
		display: flex;
		flex-direction: column;
		margin-top: 8px;
		max-height: min(50vh, 360px);
		overflow-y: auto;
	}
	.choice {
		display: flex;
		align-items: center;
		gap: 8px;
		height: 34px;
		padding: 0 12px;
		border: 0;
		border-radius: var(--radius-md);
		background: none;
		font: inherit;
		font-size: 0.9rem;
		color: var(--color-foreground);
		text-align: left;
		cursor: pointer;
	}
	.choice.active { background: var(--color-surface-overlay); }
	.choice :global(svg) { flex: none; color: var(--color-foreground-muted); }
	.choice-label {
		flex: 1;
		min-width: 0;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.no-choice {
		margin: 0;
		padding: 12px;
		font-size: 0.9rem;
		color: var(--color-foreground-muted);
	}
</style>
