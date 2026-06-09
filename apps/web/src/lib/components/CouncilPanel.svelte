<script lang="ts">
	import Icon from '$lib/components/Icon.svelte';
	import type { CouncilMemberStatus } from '$lib/stores/chatInstances.svelte';

	let { members }: { members: CouncilMemberStatus[] } = $props();

	// Strip the provider prefix for a compact label: "google/gemini-3-flash" → "gemini-3-flash".
	function shortModel(model: string): string {
		const slash = model.lastIndexOf('/');
		return slash >= 0 ? model.slice(slash + 1) : model;
	}

	const doneCount = $derived(members.filter((m) => m.status !== 'thinking').length);
	const allDone = $derived(members.length > 0 && doneCount === members.length);
</script>

{#if members.length > 0}
	<div class="council-panel" data-done={allDone}>
		<div class="council-header">
			<span class="council-mark">∴</span>
			<span class="council-title">
				{allDone ? 'Council deliberated' : 'Council deliberating'}
			</span>
			<span class="council-count">{doneCount}/{members.length}</span>
		</div>
		<div class="council-grid">
			{#each members as m (m.memberId)}
				<div class="council-member" data-status={m.status}>
					<span class="member-status">
						{#if m.status === 'thinking'}
							<Icon icon="ri:loader-4-line" width="13" class="spin" />
						{:else if m.status === 'done'}
							<Icon icon="ri:check-line" width="13" />
						{:else}
							<Icon icon="ri:close-line" width="13" />
						{/if}
					</span>
					<span class="member-model" title={m.model}>{shortModel(m.model)}</span>
					<span class="member-lens">{m.lens}</span>
					{#if m.tokens > 0}
						<span class="member-tokens">{m.tokens}t</span>
					{/if}
				</div>
			{/each}
		</div>
	</div>
{/if}

<style>
	.council-panel {
		border: 1px solid var(--color-border);
		border-radius: 0.5rem;
		background: color-mix(in srgb, var(--color-info) 5%, transparent);
		padding: 0.6rem 0.75rem;
		margin-bottom: 0.75rem;
		font-size: 0.8rem;
	}

	.council-header {
		display: flex;
		align-items: center;
		gap: 0.4rem;
		margin-bottom: 0.5rem;
		color: var(--color-info);
		font-weight: 500;
	}

	.council-mark {
		font-size: 0.95rem;
		line-height: 1;
	}

	.council-title {
		flex: 1;
	}

	.council-count {
		color: var(--color-secondary);
		font-variant-numeric: tabular-nums;
		font-size: 0.75rem;
	}

	.council-grid {
		display: grid;
		grid-template-columns: repeat(auto-fill, minmax(150px, 1fr));
		gap: 0.35rem;
	}

	.council-member {
		display: flex;
		align-items: center;
		gap: 0.35rem;
		padding: 0.3rem 0.45rem;
		border-radius: 0.375rem;
		background: var(--color-surface);
		border: 1px solid var(--color-border);
		min-width: 0;
	}

	.member-status {
		display: inline-flex;
		flex-shrink: 0;
		color: var(--color-secondary);
	}

	.council-member[data-status='done'] .member-status {
		color: var(--color-success);
	}

	.council-member[data-status='failed'] .member-status {
		color: var(--color-error);
	}

	.council-member[data-status='failed'] {
		opacity: 0.55;
	}

	.member-model {
		font-weight: 500;
		color: var(--color-foreground);
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
		min-width: 0;
	}

	.member-lens {
		color: var(--color-secondary);
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
		flex: 1;
		min-width: 0;
	}

	.member-tokens {
		color: var(--color-secondary);
		font-variant-numeric: tabular-nums;
		font-size: 0.7rem;
		flex-shrink: 0;
	}

	:global(.council-member .spin) {
		animation: council-spin 0.9s linear infinite;
	}

	@keyframes council-spin {
		to {
			transform: rotate(360deg);
		}
	}
</style>
