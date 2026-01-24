<script lang="ts">
	/**
	 * Axiology Chat - Values Discovery Conversation
	 * This is a special chat page that initiates the values discovery process
	 * Users land here after completing the technical onboarding tunnel
	 */
	import { onMount } from "svelte";
	import { goto } from "$app/navigation";
	import "iconify-icon";

	// For now, this is a placeholder that will redirect to the main chat
	// with a special context for values discovery
	let loading = $state(true);

	onMount(async () => {
		// In the future, this could:
		// 1. Create a new conversation with axiology agent
		// 2. Pre-populate with a values discovery prompt
		// 3. Store that this is an axiology conversation

		// For now, redirect to main chat with a query param
		setTimeout(() => {
			goto("/?mode=axiology");
		}, 2000);
	});
</script>

<svelte:head>
	<title>Discover Your Values | Virtues</title>
</svelte:head>

<div class="axiology-intro">
	<div class="content">
		<div class="icon-container">
			<iconify-icon icon="ri:compass-3-line" width="48"></iconify-icon>
		</div>

		<h1>Let's discover what matters to you</h1>

		<p class="lead">
			You've completed the technical setup. Now comes the meaningful part:
			understanding your values, goals, and what you want to achieve.
		</p>

		<div class="features">
			<div class="feature">
				<iconify-icon icon="ri:chat-1-line" width="20"></iconify-icon>
				<span>A thoughtful conversation about your life and aspirations</span>
			</div>
			<div class="feature">
				<iconify-icon icon="ri:time-line" width="20"></iconify-icon>
				<span>Takes about 10-15 minutes</span>
			</div>
			<div class="feature">
				<iconify-icon icon="ri:shield-check-line" width="20"></iconify-icon>
				<span>Everything stays private on your device</span>
			</div>
		</div>

		<div class="loading-indicator">
			<iconify-icon icon="ri:loader-4-line" width="24" class="animate-spin"></iconify-icon>
			<span>Starting conversation...</span>
		</div>
	</div>
</div>

<style>
	.axiology-intro {
		min-height: 100vh;
		display: flex;
		align-items: center;
		justify-content: center;
		padding: 24px;
		background: var(--surface);
	}

	.content {
		max-width: 500px;
		text-align: center;
	}

	.icon-container {
		width: 80px;
		height: 80px;
		border-radius: 50%;
		background: var(--primary-bg, rgba(59, 130, 246, 0.1));
		display: flex;
		align-items: center;
		justify-content: center;
		margin: 0 auto 24px;
		color: var(--primary);
	}

	h1 {
		font-family: var(--font-serif);
		font-size: 28px;
		font-weight: 500;
		color: var(--foreground);
		margin-bottom: 16px;
	}

	.lead {
		font-size: 16px;
		line-height: 1.6;
		color: var(--foreground-muted);
		margin-bottom: 32px;
	}

	.features {
		display: flex;
		flex-direction: column;
		gap: 12px;
		text-align: left;
		margin-bottom: 40px;
	}

	.feature {
		display: flex;
		align-items: center;
		gap: 12px;
		padding: 12px 16px;
		background: var(--surface-elevated);
		border-radius: 8px;
		font-size: 14px;
		color: var(--foreground);
	}

	.feature iconify-icon {
		color: var(--primary);
		flex-shrink: 0;
	}

	.loading-indicator {
		display: flex;
		align-items: center;
		justify-content: center;
		gap: 12px;
		color: var(--foreground-muted);
		font-size: 14px;
	}

	:global(.animate-spin) {
		animation: spin 1s linear infinite;
	}

	@keyframes spin {
		from {
			transform: rotate(0deg);
		}
		to {
			transform: rotate(360deg);
		}
	}
</style>
