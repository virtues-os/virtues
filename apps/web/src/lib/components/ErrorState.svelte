<script lang="ts">
	import StateBlock from "$lib/components/StateBlock.svelte";
	import Button from "$lib/components/Button.svelte";
	import Icon from "$lib/components/Icon.svelte";

	/**
	 * A pane whose data did not arrive. The shell is
	 * [StateBlock](StateBlock.svelte).
	 *
	 * The retry control used to be a hand-rolled `<button>` with its own
	 * border, radius and hover — inside the component library, by the people
	 * who wrote the library. That is the clearest evidence there was that
	 * `Button` was not usable from a scoped-CSS file, and it is why Button was
	 * rewritten rather than merely adopted.
	 */
	let {
		title = "Something went wrong",
		message,
		onRetry,
		class: className = "",
	}: {
		title?: string;
		message?: string;
		onRetry?: () => void;
		class?: string;
	} = $props();
</script>

<StateBlock
	icon="ri:error-warning-line"
	tone="error"
	{title}
	{message}
	class={className}
	actions={onRetry ? retry : undefined}
/>

{#snippet retry()}
	<Button variant="secondary" size="sm" onclick={onRetry}>
		<Icon icon="ri:refresh-line" width="14" />
		Retry
	</Button>
{/snippet}
