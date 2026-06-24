<script lang="ts">
	import { VIRTUES_LOGO_PATH } from "$lib/utils/svgPaths";

	interface Props {
		collapsed?: boolean;
		animationDelay?: number;
	}

	let {
		collapsed = false,
		animationDelay = 0,
	}: Props = $props();

	// Single-workspace model: the shell is always the system "Virtues" workspace.
	const activeLabel = "Virtues";
</script>

<div class="header-container" class:collapsed>
	<div
		class="title-row animate-row"
		style="animation-delay: {animationDelay}ms; --stagger-delay: {animationDelay}ms"
	>
		<div class="title-icon">
			<svg
				class="title-svg"
				width="16"
				height="16"
				viewBox="0 0 24 24"
			>
				<path d={VIRTUES_LOGO_PATH} fill="currentColor" />
			</svg>
		</div>

		<span class="title-label">{activeLabel}</span>
	</div>
</div>

<style>
	@reference "../../../app.css";

	:root {
		--ease-premium: cubic-bezier(0.2, 0, 0, 1);
	}

	@keyframes fadeSlideIn {
		from {
			opacity: 0;
			transform: translateX(-8px);
		}
		to {
			opacity: 1;
			transform: translateX(0);
		}
	}

	.header-container {
		display: flex;
		flex-direction: column;
		padding: 16px 0 10px 8px;
	}

	.header-container.collapsed {
		opacity: 0;
		transform: translateX(-8px);
		transition:
			opacity 150ms var(--ease-premium),
			transform 150ms var(--ease-premium);
	}

	.animate-row {
		animation: fadeSlideIn 200ms var(--ease-premium) backwards;
		opacity: 1;
		transform: translateX(0);
		transition:
			opacity 200ms var(--ease-premium) var(--stagger-delay, 0ms),
			transform 200ms var(--ease-premium) var(--stagger-delay, 0ms);
	}

	.title-row {
		display: flex;
		align-items: center;
		gap: 6px;
		padding: 8px var(--sidebar-padding-left-base, 10px);
		height: 32px;
		box-sizing: border-box;
		cursor: pointer;
		border-radius: 6px;
	}

	.title-icon {
		display: flex;
		align-items: center;
		justify-content: center;
		width: 16px;
		height: 16px;
		flex-shrink: 0;
		color: var(--color-foreground);
		position: relative;
	}

	.title-svg {
		display: block;
		transition: opacity 0.15s ease;
	}

	.title-label {
		flex: 1;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
		font-size: 17px;
		font-weight: 400;
		font-family: var(--font-serif, serif);
		color: var(--color-foreground);
		line-height: 1.4;
	}
</style>
