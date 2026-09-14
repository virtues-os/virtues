<!--
	One hairline field. The room asks for a few short things — an email, a
	URL, a key — and each is a line to write on, not a boxed control.
-->
<script lang="ts">
	interface Props {
		value: string;
		type?: "text" | "email" | "url" | "password";
		placeholder?: string;
		disabled?: boolean;
		autofocus?: boolean;
		grow?: boolean;
	}
	let {
		value = $bindable(""),
		type = "text",
		placeholder = "",
		disabled = false,
		autofocus = false,
		grow = true,
	}: Props = $props();
	let el = $state<HTMLInputElement | null>(null);
	$effect(() => {
		if (autofocus) el?.focus();
	});
</script>

<input
	bind:this={el}
	bind:value
	{type}
	{placeholder}
	{disabled}
	spellcheck="false"
	class="line"
	class:grow
/>

<style>
	.line {
		font: inherit;
		font-size: 1rem;
		padding: 0.35rem 0;
		border: 0;
		border-bottom: 1px solid var(--color-border);
		border-radius: 0;
		background: transparent;
		color: var(--color-foreground);
		outline: none;
		min-width: 0;
		transition: border-color 0.2s ease;
	}
	.line.grow {
		flex: 1;
	}
	.line::placeholder {
		color: var(--color-foreground-subtle);
	}
	.line:focus {
		border-bottom-color: var(--color-foreground);
	}
	.line:disabled {
		opacity: 0.5;
	}
</style>
