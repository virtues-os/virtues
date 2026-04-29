<script lang="ts">
	import Modal from '$lib/components/Modal.svelte';
	import Icon from '$lib/components/Icon.svelte';
	import { spaceStore } from '$lib/stores/space.svelte';
	import type { Action } from '$lib/api/client';

	let {
		open,
		onClose,
		// Kept for API compatibility with ActionsPanel; chat-to-create will
		// call back into this once the tool is wired end-to-end.
		onCreated: _onCreated
	}: {
		open: boolean;
		onClose: () => void;
		onCreated: (action: Action) => void;
	} = $props();

	function startChatFlow() {
		spaceStore.openTabFromRoute('/chat', { forceNew: true });
		onClose();
	}
</script>

<Modal {open} {onClose} title="New action" width="md">
	<div class="picker">
		<button type="button" class="option primary" onclick={startChatFlow}>
			<div class="icon">
				<Icon icon="ri:chat-smile-2-line" width="22" />
			</div>
			<div class="text">
				<div class="label">
					Create with chat
					<span class="pill">Recommended</span>
				</div>
				<p class="desc">
					Describe what you want the action to do in plain language. Virtues will draft the
					prompt, schedule, and required connections for you.
				</p>
			</div>
			<Icon icon="ri:arrow-right-line" width="18" />
		</button>

		<button type="button" class="option" disabled>
			<div class="icon">
				<Icon icon="ri:code-s-slash-line" width="22" />
			</div>
			<div class="text">
				<div class="label">
					Advanced editor
					<span class="pill muted">Coming soon</span>
				</div>
				<p class="desc">
					Write the action prompt, code, schedule, and triggers by hand. For power users and
					community templates.
				</p>
			</div>
			<Icon icon="ri:lock-line" width="18" />
		</button>
	</div>
</Modal>

<style>
	.picker {
		display: flex;
		flex-direction: column;
		gap: 0.75rem;
	}
	.option {
		display: flex;
		align-items: center;
		gap: 1rem;
		padding: 1rem 1.125rem;
		border-radius: 10px;
		border: 1px solid var(--color-border, #e5e7eb);
		background: var(--color-surface, #fff);
		font: inherit;
		color: var(--color-foreground, inherit);
		text-align: left;
		cursor: pointer;
		transition: border-color 120ms ease, background 120ms ease, transform 120ms ease;
	}
	.option:hover:not(:disabled) {
		border-color: var(--color-primary, #4338ca);
		background: var(--color-surface-elevated, #f9fafb);
	}
	.option:disabled {
		opacity: 0.55;
		cursor: not-allowed;
	}
	.option.primary {
		border-color: var(--color-primary, #4338ca);
	}
	.icon {
		display: grid;
		place-items: center;
		width: 40px;
		height: 40px;
		border-radius: 999px;
		background: var(--color-surface-elevated, #f3f4f6);
		color: var(--color-primary, #4338ca);
		flex-shrink: 0;
	}
	.text {
		flex: 1;
		min-width: 0;
	}
	.label {
		display: flex;
		align-items: center;
		gap: 0.5rem;
		font-size: 0.9375rem;
		font-weight: 600;
	}
	.desc {
		margin: 0.25rem 0 0;
		font-size: 0.8125rem;
		line-height: 1.45;
		color: var(--color-foreground-muted, #6b7280);
	}
	.pill {
		font-size: 0.625rem;
		font-weight: 600;
		text-transform: uppercase;
		letter-spacing: 0.06em;
		padding: 0.125rem 0.5rem;
		border-radius: 999px;
		background: var(--color-primary, #4338ca);
		color: #fff;
	}
	.pill.muted {
		background: var(--color-surface-elevated, #e5e7eb);
		color: var(--color-foreground-muted, #6b7280);
	}
</style>
