<script lang="ts">
	/**
	 * The controls side by side on the 34px line, so a field, a select, a
	 * button and an icon button can be seen to share one height. Two
	 * hairlines mark the top and bottom of --control-h.
	 */
	import Button from '$lib/components/Button.svelte';
	import IconButton from '$lib/components/IconButton.svelte';
	import Input from '$lib/components/Input.svelte';
	import Choice from '$lib/components/settings/Choice.svelte';

	let query = $state('');
	let range = $state<'week' | 'month'>('week');
	const RANGES = [
		{ value: 'week', label: 'This week' },
		{ value: 'month', label: 'This month' }
	] as const;
</script>

<div class="band">
	<div class="row">
		<div class="field"><Input bind:value={query} placeholder="Filter pages" delight={false} /></div>
		<Choice value={range} options={RANGES} label="Range" onchange={(v) => (range = v)} />
		<Button variant="secondary">Export</Button>
		<Button>Save</Button>
		<IconButton icon="mynaui:dots" label="More" variant="secondary" />
		<span class="nested">Row <IconButton icon="mynaui:dots" label="More" size="nested" /></span>
	</div>
</div>

<style>
	.band {
		position: relative;
		padding: 16px 0;
	}
	.band::before,
	.band::after {
		content: '';
		position: absolute;
		left: 0;
		right: 0;
		height: 1px;
		background: var(--drop-tint);
	}
	.band::before {
		top: 16px;
	}
	.band::after {
		top: calc(16px + var(--control-h) - 1px);
	}
	.row {
		position: relative;
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: 12px;
	}
	.field {
		width: 200px;
	}
	.nested {
		display: inline-flex;
		align-items: center;
		gap: 8px;
		font-size: var(--text-label);
		color: var(--color-foreground-muted);
	}
</style>
