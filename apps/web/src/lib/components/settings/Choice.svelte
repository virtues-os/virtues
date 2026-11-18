<script lang="ts" generics="T extends string">
	/**
	 * A choice among a few named options, as a select. Settings use a select
	 * rather than buttons side by side so one option reads as the default and
	 * the others as a deliberate change.
	 */
	let {
		id,
		value,
		options,
		label,
		disabled = false,
		onchange,
	}: {
		id?: string;
		value: T;
		options: readonly { value: T; label: string }[];
		/** For screen readers when no `<label for>` names it. */
		label?: string;
		disabled?: boolean;
		onchange: (next: T) => void;
	} = $props();
</script>

<select {id} {value} {disabled} aria-label={label} onchange={(e) => onchange(e.currentTarget.value as T)}>
	{#each options as option (option.value)}
		<option value={option.value}>{option.label}</option>
	{/each}
</select>

<style>
	/* A control (design.md, "Controls"): 34px tall, on the field edge that
	   holds 3:1 against the page. */
	select {
		font: inherit;
		font-size: var(--text-label);
		height: var(--control-h);
		padding: 0 8px;
		border-radius: var(--radius-md);
		border: 1px solid var(--color-border-field);
		background: var(--color-surface);
		color: var(--color-foreground);
	}

	@media (pointer: coarse) {
		select {
			height: var(--control-touch-h);
			font-size: 16px;
		}
	}
</style>
