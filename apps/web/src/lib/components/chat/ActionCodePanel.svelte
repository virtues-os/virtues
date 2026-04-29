<script lang="ts">
	import { onDestroy, tick } from 'svelte';
	import { spaceStore } from '$lib/stores/space.svelte';
	import { createPlainEditor } from '$lib/codemirror/plainEditor';
	import type { EditorView } from '@codemirror/view';

	interface Props {
		conversationId: string | undefined;
		active: boolean;
		tabId: string;
	}

	let { conversationId, active, tabId }: Props = $props();

	let activationCode = $state('');
	let originalCode = $state('');
	let loading = $state(true);
	let saving = $state(false);
	let error = $state<string | null>(null);
	let editor: EditorView | null = null;
	let editorContainer: HTMLDivElement;

	const dirty = $derived(activationCode !== originalCode);

	async function loadActivationCode() {
		if (!conversationId) {
			error = 'No conversation ID';
			loading = false;
			return;
		}

		loading = true;
		error = null;

		try {
			const res = await fetch(`/api/chats/${conversationId}/action`);
			if (!res.ok) throw new Error(`Failed to fetch: ${res.status}`);

			const data = await res.json();
			originalCode = data.action_activation || '';
			activationCode = originalCode;
		} catch (e) {
			error = e instanceof Error ? e.message : 'Unknown error';
		} finally {
			loading = false;
		}

		await tick();
		if (editorContainer && !editor) {
			editor = createPlainEditor({
				parent: editorContainer,
				content: originalCode,
				onChange: (code) => { activationCode = code; }
			});
		}
	}

	async function handleSave() {
		if (!conversationId || saving) return;

		saving = true;
		error = null;
		try {
			const res = await fetch(`/api/chats/${conversationId}`, {
				method: 'PATCH',
				headers: { 'Content-Type': 'application/json' },
				body: JSON.stringify({ action_activation: activationCode || null })
			});

			if (!res.ok) throw new Error(`Failed to save: ${res.status}`);

			originalCode = activationCode;
			goBack();
		} catch (e) {
			error = e instanceof Error ? e.message : 'Save failed';
		} finally {
			saving = false;
		}
	}

	function goBack() {
		spaceStore.updateTab(tabId, { route: `/chat/${conversationId}` });
	}

	$effect(() => {
		if (active && conversationId) {
			loadActivationCode();
		}
	});

	onDestroy(() => {
		editor?.destroy();
		editor = null;
	});
</script>

<div class="activation-panel">
	<div class="panel-header">
		<button class="back-btn" onclick={goBack} type="button">
			&larr; Back
		</button>
		<h3>Activation Code</h3>
		<div class="header-actions">
			{#if dirty}
				<span class="unsaved-badge">Unsaved</span>
			{/if}
			<button
				class="save-btn"
				onclick={handleSave}
				disabled={saving || !dirty}
				type="button"
			>
				{saving ? 'Saving...' : 'Save'}
			</button>
		</div>
	</div>

	{#if loading}
		<div class="panel-status">Loading...</div>
	{:else if error}
		<div class="panel-status panel-error">
			<span>{error}</span>
			<button type="button" onclick={loadActivationCode}>Retry</button>
		</div>
	{:else}
		<div class="editor-wrapper" bind:this={editorContainer}></div>
	{/if}
</div>

<style>
	.activation-panel {
		display: flex;
		flex-direction: column;
		height: 100%;
		overflow: hidden;
	}

	.panel-header {
		display: flex;
		align-items: center;
		gap: 12px;
		padding: 8px 16px;
		border-bottom: 1px solid var(--color-border);
		background: var(--color-surface-elevated);
		flex-shrink: 0;
	}

	.panel-header h3 {
		margin: 0;
		font-size: 0.875rem;
		font-weight: 500;
		color: var(--color-foreground);
		flex: 1;
	}

	.header-actions {
		display: flex;
		align-items: center;
		gap: 8px;
	}

	.back-btn {
		padding: 4px 8px;
		border: none;
		background: transparent;
		color: var(--color-foreground-muted);
		cursor: pointer;
		font-size: 0.8125rem;
		border-radius: 4px;
	}

	.back-btn:hover {
		background: var(--color-surface-hover);
		color: var(--color-foreground);
	}

	.unsaved-badge {
		font-size: 0.6875rem;
		color: var(--color-warning);
		font-weight: 500;
	}

	.save-btn {
		padding: 4px 12px;
		border: 1px solid var(--color-border);
		background: var(--color-surface);
		border-radius: 6px;
		font-size: 0.8125rem;
		color: var(--color-foreground);
		cursor: pointer;
		transition: background-color 0.15s ease;
	}

	.save-btn:hover:not(:disabled) {
		background: var(--color-surface-hover);
	}

	.save-btn:disabled {
		opacity: 0.5;
		cursor: not-allowed;
	}

	.panel-status {
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: 0.75rem;
		padding: 3rem;
		color: var(--color-foreground-muted);
	}

	.panel-error {
		color: var(--color-error);
	}

	.panel-error button {
		padding: 0.5rem 1rem;
		border: 1px solid var(--color-border);
		background: var(--color-surface);
		border-radius: 6px;
		cursor: pointer;
	}

	.editor-wrapper {
		flex: 1;
		overflow: auto;
		padding: 0 16px;
	}
</style>
