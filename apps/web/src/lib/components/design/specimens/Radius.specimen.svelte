<script lang="ts">
	/** Every radius step, drawn at the size it is used at. */
	const STEPS = ['--radius-xs', '--radius-sm', '--radius-md', '--radius-lg', '--radius-xl', '--radius-full'];
</script>

<div class="steps">
	{#each STEPS as token (token)}
		<div class="step">
			<!-- design-ok: the radius is each --radius-* token in turn -->
			<div class="box" style="border-radius: var({token})"></div>
			<code>{token}</code>
		</div>
	{/each}
</div>

<style>
	.steps {
		display: flex;
		flex-wrap: wrap;
		gap: 24px;
	}
	.step {
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: 8px;
		font-size: var(--text-caption);
	}
	.box {
		width: 72px;
		height: 48px;
		background: var(--hover-bg);
		outline: 1px solid var(--color-border);
		outline-offset: -1px;
	}
</style>
