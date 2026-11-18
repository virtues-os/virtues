<script lang="ts">
	/** Switch and Choice, the settings controls that every page will share once they move to lib/components. */
	import Choice from './Choice.svelte';
	import Switch from './Switch.svelte';

	let on = $state(true);
	let size = $state<'small' | 'medium' | 'large'>('medium');
	const SIZES = [
		{ value: 'small', label: 'Small' },
		{ value: 'medium', label: 'Medium' },
		{ value: 'large', label: 'Large' }
	] as const;
</script>

<div class="rows">
	<div class="row"><span>Reduce motion</span><Switch checked={on} label="Reduce motion" onchange={(v) => (on = v)} /></div>
	<div class="row"><span>Disabled</span><Switch checked={false} label="Disabled" disabled onchange={() => {}} /></div>
	<div class="row"><span>Text size</span><Choice value={size} options={SIZES} label="Text size" onchange={(v) => (size = v)} /></div>
</div>

<style>
	.rows {
		display: flex;
		flex-direction: column;
		gap: 12px;
		max-width: 360px;
	}
	.row {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 16px;
		font-size: var(--text-label);
	}
</style>
