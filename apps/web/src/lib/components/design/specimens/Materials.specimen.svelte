<script lang="ts">
	/**
	 * Desk, paper and glass over the window's painting: a glass control
	 * and a glass card, which are the same one material, a paper card, and
	 * the one shadow a floating layer carries. The Glass setting changes the glass
	 * here as it does in the shell.
	 */
	import Background from "$lib/components/Background.svelte";
</script>

<div class="stage">
	<div class="scene" aria-hidden="true">
		<Background />
	</div>
	<div class="row">
		<button type="button" class="control lq">Glass</button>
		<div class="card lq">
			<div class="label">Glass</div>
			<div class="line">Holds words, like the sidebar.</div>
		</div>
		<div class="paper">
			<div class="label">Paper</div>
			<div class="line">Reading text lives here, never on glass.</div>
		</div>
		<div class="float">
			<div class="label">A floating layer</div>
			<div class="line"><code>--shadow-float</code></div>
		</div>
	</div>
</div>

<style>
	.stage {
		position: relative;
		overflow: hidden;
		border-radius: var(--radius-xl);
		background: var(--desk);
		padding: 24px;
		min-height: 220px;
	}
	.scene {
		position: absolute;
		inset: 0;
	}
	.row {
		position: relative;
		display: flex;
		flex-wrap: wrap;
		align-items: flex-start;
		gap: 16px;
	}
	.control {
		height: 34px;
		padding: 0 16px;
		border: none;
		border-radius: var(--radius-full);
		font: inherit;
		font-size: var(--text-label);
		color: var(--color-foreground);
		cursor: pointer;
	}
	.card,
	.paper,
	.float {
		width: 180px;
		padding: 12px 16px;
		border-radius: var(--radius-xl);
	}
	.paper {
		background: var(--page);
	}
	.float {
		background: var(--page);
		box-shadow: var(--shadow-float);
	}
	.label {
		font-size: var(--text-label);
		color: var(--color-foreground);
	}
	.line {
		margin-top: 4px;
		font-size: var(--text-small);
		color: var(--color-foreground-muted);
	}
</style>
