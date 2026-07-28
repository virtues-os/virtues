<script lang="ts">
	import Icon from "$lib/components/Icon.svelte";

	interface Props {
		collapsed?: boolean;
		animationDelay?: number;
		/** Opens the search / command modal. */
		onSearch?: () => void;
		/** Starts a new chat. */
		onNewChat?: () => void;
		/** Goes Home — the ∴ mark's job. */
		onHome?: () => void;
	}

	let {
		collapsed = false,
		animationDelay = 0,
		onSearch,
		onNewChat,
		onHome,
	}: Props = $props();

	// Three affordances, not one. The masthead used to be a single full-width
	// button — ∴ on the left, a ⌘K chip on the right — which read as a label
	// rather than a control: it was the only row in the sidebar carrying a
	// keyboard hint, so it looked like chrome. Now the mark goes Home (and Home
	// leaves the nav list below, where it was redundant), and search and new-chat
	// get their own targets on the right. The ⌘K hint moves into the tooltip,
	// where hints belong.
</script>

<div class="masthead" class:collapsed>
	<div
		class="masthead-row animate-row"
		style="animation-delay: {animationDelay}ms; --stagger-delay: {animationDelay}ms"
	>
		<button
			type="button"
			class="mark-btn"
			onclick={() => onHome?.()}
			title="Home"
			aria-label="Home"
		>
			<span class="mark">∴</span>
		</button>

		<div class="masthead-actions">
			<button
				type="button"
				class="masthead-action"
				onclick={() => onSearch?.()}
				title="Ask or search (⌘K)"
				aria-label="Ask or search"
			>
				<Icon icon="ri:search-line" width="15" />
			</button>
			<button
				type="button"
				class="masthead-action"
				onclick={() => onNewChat?.()}
				title="New chat (⌘N)"
				aria-label="New chat"
			>
				<Icon icon="ri:add-line" width="16" />
			</button>
		</div>
	</div>
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

	/* Height matches the pane toolbar's row so the sidebar's top edge and the
	   toolbar's share a baseline across the seam. */
	.masthead-row {
		display: flex;
		align-items: center;
		justify-content: space-between;
		width: 100%;
		height: 30px;
		box-sizing: border-box;
		padding: 0 6px 0 calc(var(--sidebar-padding-left-base) - 4px);
	}

	/* The mark keeps the nav rows' icon column, so ∴ sits directly above the
	   icons below it rather than floating in its own margin. */
	.mark-btn {
		display: flex;
		align-items: center;
		justify-content: center;
		width: 24px;
		height: 24px;
		padding: 0;
		border: none;
		border-radius: 6px;
		background: none;
		cursor: pointer;
		transition: background 0.15s ease;
	}

	.mark-btn:hover {
		background: color-mix(in srgb, var(--color-foreground) 8%, transparent);
	}

	.mark {
		font-family: var(--font-serif, serif);
		font-size: 18px;
		line-height: 1;
		color: var(--color-foreground);
		letter-spacing: 0.02em;
	}

	.masthead-actions {
		display: flex;
		align-items: center;
		gap: 2px;
	}

	.masthead-action {
		display: flex;
		align-items: center;
		justify-content: center;
		width: 24px;
		height: 24px;
		padding: 0;
		border: none;
		border-radius: 6px;
		background: none;
		cursor: pointer;
		color: var(--color-foreground-subtle);
		transition: background 0.15s ease, color 0.15s ease;
	}

	.masthead-action:hover {
		background: color-mix(in srgb, var(--color-foreground) 8%, transparent);
		color: var(--color-foreground);
	}

	.mark-btn:focus-visible,
	.masthead-action:focus-visible {
		outline: 2px solid var(--color-primary);
		outline-offset: -2px;
	}
</style>
