<!--
	Waiting on something that is happening elsewhere — an email, a checkout.
	One line, one breathing dot, no spinner.
-->
<script lang="ts">
	import type { Snippet } from "svelte";
	let { children }: { children: Snippet } = $props();
</script>

<p class="waiting"><span class="dot" aria-hidden="true"></span>{@render children()}</p>

<style>
	@keyframes breathe {
		from {
			opacity: 0.2;
		}
	}
	.waiting {
		display: flex;
		align-items: center;
		gap: 0.6rem;
		margin: 0;
		font-size: 0.875rem;
		color: var(--color-foreground-muted);
	}
	.dot {
		width: 0.45rem;
		height: 0.45rem;
		border-radius: 999px;
		background: var(--color-foreground);
		animation: breathe 1.3s ease-in-out infinite alternate;
		flex: none;
	}
</style>
