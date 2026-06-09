<script lang="ts">
	import Icon from '$lib/components/Icon.svelte';
	import type { SubagentStatus } from '$lib/stores/chatInstances.svelte';

	let { subagents }: { subagents: SubagentStatus[] } = $props();

	// Strip the provider prefix: "google/gemini-3-flash" → "gemini-3-flash".
	function shortModel(model: string): string {
		const slash = model.lastIndexOf('/');
		return slash >= 0 ? model.slice(slash + 1) : model;
	}

	const doneCount = $derived(subagents.filter((s) => s.status !== 'thinking').length);
	const allDone = $derived(subagents.length > 0 && doneCount === subagents.length);
</script>

{#if subagents.length > 0}
	<div class="subagent-panel" data-done={allDone}>
		<div class="subagent-header">
			<span class="subagent-mark">∴</span>
			<span class="subagent-title">
				{allDone ? 'Researchers reported back' : 'Researchers investigating'}
			</span>
			<span class="subagent-count">{doneCount}/{subagents.length}</span>
		</div>
		<div class="subagent-grid">
			{#each subagents as s (s.subagentId)}
				<div class="subagent-card" data-status={s.status}>
					<span class="subagent-status-icon">
						{#if s.status === 'thinking'}
							<Icon icon="ri:loader-4-line" width="13" class="spin" />
						{:else if s.status === 'done'}
							<Icon icon="ri:check-line" width="13" />
						{:else}
							<Icon icon="ri:close-line" width="13" />
						{/if}
					</span>
					<span class="subagent-mission" title={s.title}>{s.title}</span>
					<span class="subagent-model" title={s.model}>{shortModel(s.model)}</span>
					{#if s.tokens > 0}
						<span class="subagent-tokens">{s.tokens}t</span>
					{/if}
				</div>
			{/each}
		</div>
	</div>
{/if}

<style>
	.subagent-panel {
		border: 1px solid var(--color-border);
		border-radius: 0.5rem;
		background: color-mix(in srgb, var(--color-info) 5%, transparent);
		padding: 0.6rem 0.75rem;
		margin-bottom: 0.75rem;
		font-size: 0.8rem;
	}

	.subagent-header {
		display: flex;
		align-items: center;
		gap: 0.4rem;
		margin-bottom: 0.5rem;
		color: var(--color-info);
		font-weight: 500;
	}

	.subagent-mark {
		font-size: 0.95rem;
		line-height: 1;
	}

	.subagent-title {
		flex: 1;
	}

	.subagent-count {
		color: var(--color-secondary);
		font-variant-numeric: tabular-nums;
		font-size: 0.75rem;
	}

	.subagent-grid {
		display: grid;
		grid-template-columns: repeat(auto-fill, minmax(180px, 1fr));
		gap: 0.35rem;
	}

	.subagent-card {
		display: flex;
		align-items: center;
		gap: 0.35rem;
		padding: 0.3rem 0.45rem;
		border-radius: 0.375rem;
		background: var(--color-surface);
		border: 1px solid var(--color-border);
		min-width: 0;
	}

	.subagent-status-icon {
		display: inline-flex;
		flex-shrink: 0;
		color: var(--color-secondary);
	}

	.subagent-card[data-status='done'] .subagent-status-icon {
		color: var(--color-success);
	}

	.subagent-card[data-status='failed'] .subagent-status-icon {
		color: var(--color-error);
	}

	.subagent-card[data-status='failed'] {
		opacity: 0.55;
	}

	.subagent-mission {
		font-weight: 500;
		color: var(--color-foreground);
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
		flex: 1;
		min-width: 0;
	}

	.subagent-model {
		color: var(--color-secondary);
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
		font-size: 0.7rem;
		flex-shrink: 0;
	}

	.subagent-tokens {
		color: var(--color-secondary);
		font-variant-numeric: tabular-nums;
		font-size: 0.7rem;
		flex-shrink: 0;
	}

	:global(.subagent-card .spin) {
		animation: subagent-spin 0.9s linear infinite;
	}

	@keyframes subagent-spin {
		to {
			transform: rotate(360deg);
		}
	}
</style>
