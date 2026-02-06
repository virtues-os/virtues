<script lang="ts">
	/**
	 * Update Banner
	 *
	 * Persistent thin banner shown when an update is available.
	 * Unlike toasts, this can't be dismissed — it reflects ongoing state,
	 * not a one-time event. It disappears when the state resolves
	 * (user refreshes or triggers the update).
	 *
	 * Two variants:
	 *   - Frontend drift: new code deployed, user needs to refresh
	 *   - System update: new image available, user needs to trigger update
	 *
	 * If both are true, shows only the system update banner (updating
	 * the system also delivers the frontend update).
	 */
	import { slide } from "svelte/transition";
	import Icon from "$lib/components/Icon.svelte";
	import { versionStore } from "$lib/stores/version.svelte";

	function handleRefresh() {
		location.reload();
	}

	function handleUpdate() {
		versionStore.triggerUpdate().catch(() => {
			// Error handling is done via the toast in the layout
		});
	}

	// Derive which banner to show (system update takes priority)
	const mode = $derived(
		versionStore.systemUpdateAvailable && !versionStore.updating
			? "system"
			: versionStore.updateAvailable
				? "drift"
				: null,
	);
</script>

{#if mode}
	<div
		class="update-banner"
		class:system={mode === "system"}
		role="status"
		aria-live="polite"
		transition:slide={{ duration: 200 }}
	>
		<div class="banner-content">
			<Icon icon="ri:download-line" width="16" />
			{#if mode === "system"}
				<span>A new version of Virtues is ready to install.</span>
				<button class="banner-action" onclick={handleUpdate}>
					Update now
				</button>
			{:else}
				<span>A new version is available.</span>
				<button class="banner-action" onclick={handleRefresh}>
					Refresh
				</button>
			{/if}
		</div>
	</div>
{/if}

<style>
	.update-banner {
		flex-shrink: 0;
		background: color-mix(
			in srgb,
			var(--color-primary, #3b82f6) 12%,
			var(--color-surface, #1a1a1a)
		);
		border-bottom: 1px solid
			color-mix(
				in srgb,
				var(--color-primary, #3b82f6) 25%,
				var(--color-border, #333)
			);
	}

	.update-banner.system {
		background: color-mix(
			in srgb,
			var(--color-primary, #3b82f6) 18%,
			var(--color-surface, #1a1a1a)
		);
	}

	.banner-content {
		display: flex;
		align-items: center;
		justify-content: center;
		gap: 0.5rem;
		padding: 0.375rem 1rem;
		font-size: 0.8125rem;
		color: var(--color-foreground, #fff);
	}

	.banner-content span {
		color: var(--color-foreground-muted, #888);
	}

	.banner-action {
		background: none;
		border: none;
		color: var(--color-primary, #3b82f6);
		font-size: 0.8125rem;
		font-weight: 500;
		cursor: pointer;
		padding: 0;
		text-decoration: underline;
		text-underline-offset: 2px;
		transition: opacity 0.15s ease;
	}

	.banner-action:hover {
		opacity: 0.8;
	}

	@media (prefers-reduced-motion: reduce) {
		.update-banner {
			transition: none;
		}
	}
</style>
