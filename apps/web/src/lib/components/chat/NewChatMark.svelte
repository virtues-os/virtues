<!--
	NewChatMark.svelte

	What an empty new chat shows above the composer: the mark and the word,
	settling in as the screen opens.
-->
<script lang="ts">
	import { fade } from 'svelte/transition';
</script>

<div class="mark" in:fade={{ duration: 200 }}>
	<svg class="init-mark" viewBox="0 0 12 10.5" width="19.87" height="17.39" fill="currentColor" aria-hidden="true">
		<circle class="init-dot init-dot-1" cx="6" cy="2.4" r="1.5" />
		<circle class="init-dot init-dot-2" cx="2.6" cy="8.1" r="1.5" />
		<circle class="init-dot init-dot-3" cx="9.4" cy="8.1" r="1.5" />
	</svg>
	<span class="init-word">Virtues</span>
</div>

<style>
	.mark {
		display: flex;
		align-items: center;
		justify-content: center;
		gap: 0.5rem;
		color: var(--color-foreground);
		pointer-events: none;
	}
	.init-mark {
		overflow: visible;
	}
	.init-word {
		font-family: var(--font-serif);
		font-size: 1.375rem;
		font-weight: 400;
		letter-spacing: 0.03em;
		-webkit-text-stroke: 0.2px currentColor;
	}

	@media (prefers-reduced-motion: no-preference) {
		.init-dot {
			animation: init-dot-settle 0.55s cubic-bezier(0.22, 1, 0.36, 1) backwards;
			transform-origin: center;
			transform-box: fill-box;
		}
		.init-dot-1 {
			animation-delay: 0.15s;
		}
		.init-dot-2 {
			animation-delay: 0.32s;
		}
		.init-dot-3 {
			animation-delay: 0.49s;
		}
		.init-word {
			animation: init-word-rise 0.7s cubic-bezier(0.22, 1, 0.36, 1) 0.75s backwards;
		}
	}

	@keyframes init-dot-settle {
		from {
			opacity: 0;
			transform: scale(0.3) translateY(3px);
		}
	}
	@keyframes init-word-rise {
		from {
			opacity: 0;
			transform: translateY(8px);
		}
	}
</style>
