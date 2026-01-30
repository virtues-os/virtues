<script lang="ts">
	import Icon from "$lib/components/Icon.svelte";

	interface Props {
		error: { message?: string } | null;
		onRetry: () => void;
	}

	let { error, onRetry }: Props = $props();

	const isRateLimitError = $derived(
		error?.message?.includes("Rate limit exceeded") ||
		error?.message?.includes("rate limit") ||
		error?.message?.includes("429")
	);
</script>

{#if error}
	<div class="flex justify-start">
		<div
			class="error-container"
			class:rate-limit-error={isRateLimitError}
		>
			<div class="error-icon">
				<Icon
					icon={isRateLimitError ? "ri:time-line" : "ri:error-warning-line"}
					width="20"
				/>
			</div>
			<div class="error-content">
				<div class="error-title">
					{isRateLimitError ? "Rate Limit Reached" : "An error occurred"}
				</div>
				<div class="error-message">
					{#if isRateLimitError}
						You've reached your API usage limit. Please wait for the limit to reset or check your usage dashboard for details.
					{:else}
						{error.message || "Something went wrong. Please try again."}
					{/if}
				</div>
				<div class="error-actions">
					{#if isRateLimitError}
						<a href="/usage" class="usage-link">
							<Icon icon="ri:bar-chart-line" width="16" />
							View Usage Dashboard
						</a>
					{:else}
						<button
							type="button"
							class="retry-button"
							onclick={onRetry}
						>
							<Icon icon="ri:refresh-line" width="16" />
							Retry
						</button>
					{/if}
				</div>
			</div>
		</div>
	</div>
{/if}

<style>
	@reference "../../../app.css";

	.error-container {
		display: flex;
		gap: 12px;
		padding: 16px;
		background: var(--color-error-subtle);
		border: 1px solid var(--color-error);
		border-radius: 12px;
		max-width: 600px;
	}

	.error-container.rate-limit-error {
		background: var(--color-warning-subtle);
		border-color: var(--color-warning);
	}

	.error-icon {
		flex-shrink: 0;
		color: var(--color-error);
	}

	.rate-limit-error .error-icon {
		color: var(--color-warning);
	}

	.error-content {
		display: flex;
		flex-direction: column;
		gap: 6px;
	}

	.error-title {
		font-weight: 600;
		color: var(--color-foreground);
	}

	.error-message {
		font-size: 14px;
		color: var(--color-foreground-muted);
		line-height: 1.5;
	}

	.error-actions {
		margin-top: 8px;
	}

	.retry-button,
	.usage-link {
		display: inline-flex;
		align-items: center;
		gap: 6px;
		padding: 8px 12px;
		font-size: 13px;
		font-weight: 500;
		border-radius: 6px;
		cursor: pointer;
		transition: all 150ms ease;
	}

	.retry-button {
		background: var(--color-error);
		color: white;
		border: none;
	}

	.retry-button:hover {
		opacity: 0.9;
	}

	.usage-link {
		background: var(--color-warning);
		color: var(--color-foreground);
		text-decoration: none;
	}

	.usage-link:hover {
		opacity: 0.9;
	}
</style>
