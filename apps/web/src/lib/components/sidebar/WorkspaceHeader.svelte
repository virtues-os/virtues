<script lang="ts">
	interface Props {
		collapsed?: boolean;
		animationDelay?: number;
		/** Opens the search / command modal (the whole masthead is the ⌘K trigger). */
		onSearch?: () => void;
	}

	let { collapsed = false, animationDelay = 0, onSearch }: Props = $props();

	// The masthead is one quiet control: the ∴ mark sits over the icon column
	// below it, and the whole row opens the command palette (⌘K). No app menu —
	// Account/System/Sign out live in the Settings folder at the foot.
</script>

<div class="masthead" class:collapsed>
	<button
		type="button"
		class="masthead-btn animate-row"
		style="animation-delay: {animationDelay}ms; --stagger-delay: {animationDelay}ms"
		onclick={() => onSearch?.()}
		title="Ask or search (⌘K)"
		aria-label="Ask or search"
	>
		<span class="mark">∴</span>
		<kbd class="kbd">⌘K</kbd>
	</button>
</div>

<style>
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

	/* Right inset 0 to match .workspace-nav (12px 0 12px 8px) so the row's
	   hover pill spans the same width as the nav rows below. */
	.masthead {
		padding: 14px 0 8px 8px;
	}

	.masthead.collapsed {
		opacity: 0;
		transform: translateX(-8px);
		pointer-events: none;
		transition:
			opacity 150ms var(--ease-premium),
			transform 150ms var(--ease-premium);
	}

	.animate-row {
		animation: fadeSlideIn 200ms var(--ease-premium) backwards;
	}

	/* One full-width row: ∴ left (flush over the nav icon column below —
	   same --sidebar-padding-left-base + 16px column as the rows), ⌘K right. */
	.masthead-btn {
		display: flex;
		align-items: center;
		justify-content: space-between;
		width: 100%;
		height: 30px;
		box-sizing: border-box;
		padding: 0 10px 0 var(--sidebar-padding-left-base);
		border-radius: 6px;
		background: none;
		border: none;
		cursor: pointer;
		transition: background 0.15s ease;
	}

	.masthead-btn:hover {
		background: color-mix(in srgb, var(--color-foreground) 6%, transparent);
	}

	.mark {
		display: inline-block;
		width: 16px;
		text-align: center;
		font-family: var(--font-serif, serif);
		font-size: 18px;
		line-height: 1;
		color: var(--color-foreground);
		letter-spacing: 0.02em;
	}

	.kbd {
		font-family: var(--font-sans);
		font-size: 10px;
		color: var(--color-foreground-subtle);
		opacity: 0.7;
		transition: opacity 0.15s ease, color 0.15s ease;
	}

	.masthead-btn:hover .kbd {
		opacity: 1;
		color: var(--color-foreground-muted);
	}
</style>
