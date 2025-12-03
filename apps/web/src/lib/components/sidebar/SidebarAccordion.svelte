<script lang="ts">
	import 'iconify-icon';
	import type { Snippet } from 'svelte';

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
		children
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
			style="animation-delay: {animationDelay}ms; --stagger-delay: {animationDelay + 400}ms"
		>
			<span class="section-title">{title}</span>
			{#if badge}
				<span class="badge">{badge}</span>
			{/if}
			<iconify-icon
				icon="ri:arrow-right-s-line"
				width="14"
				class="chevron"
				class:expanded
			></iconify-icon>
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
		--ease-premium: cubic-bezier(0.2, 0.0, 0, 1.0);
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
		margin-bottom: 16px; /* More breathing room between sections */
	}

	.accordion.collapsed {
		@apply flex justify-center;
		width: 100%;
	}

	.accordion-header {
		@apply flex items-center w-full rounded-md;
		@apply cursor-pointer;
		gap: 6px;
		/* Align section title with list item text: 10px + 16px + 10px = 36px */
		padding: 4px 10px 4px 36px;
		margin-bottom: 4px;
		/* Staggered load animation (initial mount) */
		animation: fadeSlideIn 200ms var(--ease-premium) backwards;
		/* Staggered expand transition - uses --stagger-delay CSS var */
		opacity: 1;
		transform: translateX(0);
		transition:
			opacity 200ms var(--ease-premium) var(--stagger-delay, 400ms),
			transform 200ms var(--ease-premium) var(--stagger-delay, 400ms),
			background-color 200ms var(--ease-premium);
	}

	/* Subtle hover effect on section headers */
	.accordion-header:hover .chevron {
		opacity: 0.6;
	}

	.chevron {
		@apply shrink-0 ml-auto;
		color: rgba(0, 0, 0, 0.3);
		transition: all 200ms var(--ease-premium);
	}

	:global([data-theme="dark"]) .chevron,
	:global([data-theme="night"]) .chevron {
		color: rgba(255, 255, 255, 0.3);
	}

	.chevron.expanded {
		transform: rotate(90deg);
	}

	/* Section label - Academic serif style */
	.section-title {
		font-family: 'Charter', 'Georgia', 'Times New Roman', serif;
		font-size: 13px;
		font-weight: 400;
		letter-spacing: 0.02em;
		color: rgba(0, 0, 0, 0.55);
	}

	:global([data-theme="dark"]) .section-title,
	:global([data-theme="night"]) .section-title {
		color: rgba(255, 255, 255, 0.55);
	}

	.badge {
		font-size: 10px;
		font-weight: 500;
		color: rgba(0, 0, 0, 0.4);
		padding: 2px 6px;
		border-radius: 4px;
		background: rgba(0, 0, 0, 0.04);
	}

	:global([data-theme="dark"]) .badge,
	:global([data-theme="night"]) .badge {
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
