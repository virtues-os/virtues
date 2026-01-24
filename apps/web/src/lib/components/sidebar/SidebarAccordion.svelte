<script lang="ts">
	import type { Snippet } from "svelte";

	interface Props {
		title: string;
		icon?: string;
		expanded?: boolean;
		badge?: string;
		collapsed?: boolean;
		onToggle?: () => void;
		animationDelay?: number;
		children: Snippet;
	}

	let {
		title,
		icon,
		expanded = true,
		badge,
		collapsed = false,
		onToggle,
		animationDelay = 0,
		children,
	}: Props = $props();
</script>

<div class="accordion" class:collapsed>
	{#if collapsed}
		<!-- Book Spine handles collapsed state - don't render anything here -->
	{:else}
		<!-- Expanded mode: normal accordion behavior -->
		<button
			class="accordion-header"
			class:expanded
			onclick={onToggle}
			aria-expanded={expanded}
			style="animation-delay: {animationDelay}ms; --stagger-delay: {animationDelay +
				400}ms"
		>
			<span class="section-title">{title}</span>
			<svg
				class="chevron"
				class:expanded
				width="10"
				height="10"
				viewBox="0 0 12 12"
				fill="none"
			>
				<path
					d="M4.5 3L7.5 6L4.5 9"
					stroke="currentColor"
					stroke-width="1.5"
					stroke-linecap="round"
					stroke-linejoin="round"
				/>
			</svg>
			{#if badge}
				<span class="badge">{badge}</span>
			{/if}
		</button>

		<div class="accordion-content" class:expanded>
			<div class="accordion-inner">
				{@render children()}
			</div>
		</div>
	{/if}
</div>

<style>
	@reference "../../../app.css";

	/* Premium easing - heavy friction feel */
	:root {
		--ease-premium: cubic-bezier(0.2, 0, 0, 1);
	}

	/* Staggered fade-slide animation with premium easing */
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

	.accordion {
		margin-bottom: 8px; /* Tighter spacing when section is closed */
	}

	.accordion:has(.accordion-content.expanded) {
		margin-bottom: 16px; /* More breathing room when section is open */
	}

	.accordion.collapsed {
		@apply flex justify-center;
		width: 100%;
		opacity: 0;
		transition: opacity 150ms var(--ease-premium);
	}

	.accordion-header {
		@apply flex w-full cursor-pointer items-center gap-2 rounded-lg;
		@apply px-2.5 py-1.5;
		margin-bottom: 2px;
		/* Staggered load animation (initial mount) */
		animation: fadeSlideIn 200ms var(--ease-premium) backwards;
		/* Staggered expand transition - uses --stagger-delay CSS var */
		opacity: 1;
		transform: translateX(0);
		transition:
			opacity 200ms var(--ease-premium) var(--stagger-delay, 400ms),
			transform 200ms var(--ease-premium) var(--stagger-delay, 400ms),
			background-color 200ms var(--ease-premium),
			color 200ms var(--ease-premium);
		background: transparent;
	}

	.accordion-header:hover {
		background: color-mix(in srgb, var(--color-foreground) 7%, transparent);
	}

	.chevron {
		flex-shrink: 0;
		margin-left: auto;
		color: var(--color-foreground-muted);
		transition: transform 200ms var(--ease-premium);
	}

	.chevron.expanded {
		transform: rotate(90deg);
	}

	/* Section label - Academic serif style */
	.section-title {
		font-family: "Charter", "Georgia", "Times New Roman", serif;
		font-size: 13px;
		font-weight: 400;
		letter-spacing: 0.02em;
		color: var(--color-foreground-muted);
	}

	.badge {
		font-size: 10px;
		font-weight: 500;
		color: rgba(0, 0, 0, 0.4);
		padding: 2px 6px;
		border-radius: 4px;
		background: rgba(0, 0, 0, 0.04);
	}

	:global([data-theme="midnight-oil"]) .badge,
	:global([data-theme="narnia-nights"]) .badge,
	:global([data-theme="dumb-ox"]) .badge,
	:global([data-theme="chiaroscuro"]) .badge,
	:global([data-theme="stoa"]) .badge,
	:global([data-theme="lyceum"]) .badge,
	:global([data-theme="tabula-rasa"]) .badge,
	:global([data-theme="hemlock"]) .badge,
	:global([data-theme="shire"]) .badge {
		color: rgba(255, 255, 255, 0.4);
		background: rgba(255, 255, 255, 0.06);
	}

	.accordion-content {
		display: grid;
		grid-template-rows: 0fr;
		transition: grid-template-rows 250ms var(--ease-premium);
	}

	.accordion-content.expanded {
		grid-template-rows: 1fr;
	}

	.accordion-inner {
		overflow: hidden;
		min-height: 0;
		@apply flex flex-col;
		gap: 2px; /* Subtle gap between items */
		padding-top: 6px; /* More space after header */
		padding-left: 0; /* No indent - cleaner look */
		/* Enhanced slide animation with fade and horizontal movement */
		opacity: 0;
		transform: translateX(-4px);
		transition:
			opacity 200ms var(--ease-premium) 50ms,
			transform 200ms var(--ease-premium) 50ms;
	}

	.accordion-content.expanded .accordion-inner {
		opacity: 1;
		transform: translateX(0);
	}
</style>
