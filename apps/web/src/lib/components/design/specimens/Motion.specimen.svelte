<script lang="ts">
	/**
	 * The motion tokens you can feel: a row's hover comes on fast and goes
	 * off slowly, a square control sinks to 94% while pressed, and a panel
	 * enters on --motion-enter. Under reduced motion the travel goes and the
	 * states stay.
	 */
	let open = $state(false);
</script>

<div class="demos">
	<div class="demo">
		<div class="rows">
			{#each ['Run the pointer down these rows', 'The hover trails behind you', 'Instant on, gentle off'] as text (text)}
				<div class="row">{text}</div>
			{/each}
		</div>
		<div class="caption"><code>--motion-hover-in</code> · <code>--motion-hover-out</code></div>
	</div>
	<div class="demo">
		<button type="button" class="square" aria-label="Press and hold">⌘</button>
		<div class="caption"><code>--motion-press</code> · <code>--motion-release</code></div>
	</div>
	<div class="demo">
		<button type="button" class="toggle" onclick={() => (open = !open)}>{open ? 'Close' : 'Open'} the panel</button>
		<div class="panel" class:open>A popover arriving</div>
		<div class="caption"><code>--motion-enter</code></div>
	</div>
</div>

<style>
	.demos {
		display: grid;
		grid-template-columns: repeat(auto-fit, minmax(200px, 1fr));
		gap: 24px;
		align-items: start;
	}
	.caption {
		margin-top: 8px;
		font-size: var(--text-caption);
		color: var(--color-foreground-subtle);
	}
	.row {
		height: 32px;
		padding: 0 12px;
		display: flex;
		align-items: center;
		border-radius: var(--radius-md);
		font-size: var(--text-label);
		transition: background-color var(--motion-hover-out) var(--ease-premium);
	}
	.row:hover {
		background: var(--hover-bg);
		transition-duration: var(--motion-hover-in);
	}
	.square {
		width: 34px;
		height: 34px;
		border: none;
		border-radius: var(--radius-md);
		background: var(--hover-bg);
		color: var(--color-foreground);
		font: inherit;
		cursor: pointer;
		transition: transform var(--motion-release) var(--ease-premium);
	}
	.square:active {
		transform: scale(0.94);
		transition-duration: var(--motion-press);
	}
	.toggle {
		border: none;
		background: none;
		padding: 0;
		font: inherit;
		font-size: var(--text-label);
		color: var(--color-primary);
		cursor: pointer;
	}
	.panel {
		margin-top: 12px;
		padding: 12px 16px;
		border-radius: var(--radius-md);
		background: var(--page);
		box-shadow: var(--shadow-float);
		font-size: var(--text-small);
		opacity: 0;
		transform: translateY(-4px);
		transition:
			opacity var(--motion-enter) var(--ease-premium),
			transform var(--motion-enter) var(--ease-premium);
	}
	.panel.open {
		opacity: 1;
		transform: none;
	}
	@media (prefers-reduced-motion: reduce) {
		.square:active,
		.panel {
			transform: none;
		}
	}
</style>
