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
	select {
		font: inherit;
		font-size: 13px;
		padding: 4px 8px;
		border-radius: var(--radius-md);
		border: 1px solid var(--color-border);
		background: var(--color-background);
		color: var(--color-foreground);
	}
</style>
