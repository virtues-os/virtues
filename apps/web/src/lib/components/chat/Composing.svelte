<!--
	Composing.svelte

	The activity indicator for the two rooms that show no thinking block — the
	narrative interview and Getting Started, where "the machine's workings are
	not the subject" (ChatView). Three dots, breathing. Nothing about tools,
	nothing about models, no word.

	It replaces a vendored character engine (`lib/bloub`, 15 files) whose
	resting state was the ∴ mark and whose thinking state was an animated
	ellipsis — with a rotating random gerund beside it. The ellipsis was doing
	all the work; the gerund was a second indicator of the same one bit, and
	the one it added was which verb came up.

	THE ANNOUNCEMENT IS THE POINT, not the dots. The wrapper it replaced was
	`role="presentation"` over an SVG animation, so that random gerund was the
	only thing a screen reader could perceive about the box working at all.
	Here the dots are decoration and a live region carries the fact.
-->

<script lang="ts">
	interface Props {
		/** What a screen reader hears. Steady, not a rotating word. */
		label?: string;
	}
	let { label = "Composing a reply" }: Props = $props();
</script>

<div class="composing" role="status" aria-live="polite">
	<span class="dots" aria-hidden="true">
		<span></span><span></span><span></span>
	</span>
	<span class="sr-only">{label}</span>
</div>

<style>
	.composing {
		display: flex;
		align-items: center;
		padding: 0 0 0.5rem;
	}

	.dots {
		display: flex;
		align-items: center;
		gap: 0.3rem;
	}

	/* The house idiom for waiting is one breathing dot (getting-started's
	   `Waiting`); three of them, offset, read as a reply being composed rather
	   than as a job running elsewhere. */
	.dots span {
		width: 0.3rem;
		height: 0.3rem;
		border-radius: 999px;
		background: var(--color-foreground);
		opacity: 0.25;
		animation: breathe 1.4s ease-in-out infinite;
	}

	.dots span:nth-child(2) {
		animation-delay: 0.18s;
	}

	.dots span:nth-child(3) {
		animation-delay: 0.36s;
	}

	@keyframes breathe {
		0%,
		100% {
			opacity: 0.2;
		}
		50% {
			opacity: 0.7;
		}
	}

	/* Reduced motion keeps the dots and drops the pulse — the live region is
	   what actually reports, so nothing is lost by holding them still. */
	@media (prefers-reduced-motion: reduce) {
		.dots span {
			animation: none;
			opacity: 0.45;
		}
	}

	.sr-only {
		position: absolute;
		width: 1px;
		height: 1px;
		padding: 0;
		margin: -1px;
		overflow: hidden;
		clip: rect(0, 0, 0, 0);
		white-space: nowrap;
		border: 0;
	}
</style>
