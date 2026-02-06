<script lang="ts">
	interface Props {
		conversationId: string;
		usagePercentage: number;
		totalTokens: number;
		contextWindow: number;
		status: 'healthy' | 'warning' | 'critical';
		onclick: () => void;
	}

	let { conversationId, usagePercentage, totalTokens, contextWindow, status, onclick }: Props =
		$props();

	// Format numbers for display
	function formatTokens(tokens: number): string {
		if (tokens >= 1_000_000) {
			return `${(tokens / 1_000_000).toFixed(1)}M`;
		}
		if (tokens >= 1_000) {
			return `${(tokens / 1_000).toFixed(1)}K`;
		}
		return tokens.toString();
	}

	// Get color based on status - use primary for healthy, warning/critical colors otherwise
	function getStatusColor(s: 'healthy' | 'warning' | 'critical'): string {
		switch (s) {
			case 'healthy':
				return 'var(--color-primary)';
			case 'warning':
				return '#f97316'; // orange
			case 'critical':
				return '#ef4444'; // red
		}
	}

	// Calculate SVG arc values for the ring
	const radius = 10;
	const circumference = 2 * Math.PI * radius;
	const strokeDasharray = `${circumference}`;
	const strokeDashoffset = $derived(`${circumference - (usagePercentage / 100) * circumference}`);

	const statusColor = $derived(getStatusColor(status));
	const shouldPulse = $derived(status === 'warning' || status === 'critical');
</script>

<button
	type="button"
	class="context-indicator"
	class:pulse={shouldPulse}
	{onclick}
	title={`${formatTokens(totalTokens)} / ${formatTokens(contextWindow)} tokens (${usagePercentage.toFixed(1)}%)`}
>
	<svg viewBox="0 0 24 24" class="ring-svg">
		<!-- Background ring -->
		<circle cx="12" cy="12" r="10" fill="none" stroke="var(--color-border)" stroke-width="3" />
		<!-- Progress ring -->
		<circle
			cx="12"
			cy="12"
			r="10"
			fill="none"
			stroke={statusColor}
			stroke-width="3"
			stroke-linecap="round"
			stroke-dasharray={strokeDasharray}
			stroke-dashoffset={strokeDashoffset}
			transform="rotate(-90 12 12)"
			class="progress-ring"
		/>
	</svg>
</button>

<style>
	.context-indicator {
		padding: 0.25rem;
		border: none;
		background: transparent;
		cursor: pointer;
		display: flex;
		align-items: center;
		justify-content: center;
		border-radius: 50%;
		transition: background-color 0.15s ease;
	}

	.context-indicator:hover {
		background-color: var(--color-surface-elevated);
	}

	.ring-svg {
		width: 16px;
		height: 16px;
	}

	.progress-ring {
		transition: stroke-dashoffset 0.3s ease;
	}

	.pulse {
		animation: pulse 2s ease-in-out infinite;
	}

	@keyframes pulse {
		0%,
		100% {
			opacity: 1;
		}
		50% {
			opacity: 0.7;
		}
	}
</style>
