<!--
	PageStatusNote.svelte

	A page says nothing about itself while all is well: no counts, no
	"Saved". This note appears in the page's lower corner only when there is
	something to act on or know: the assistant writing (with Stop), the
	assistant failing, the page offline, or a save that failed. The page's
	counts are under the toolbar's ⋯.
-->
<script lang="ts">
	import Icon from "$lib/components/Icon.svelte";
	import { aiSession } from "$lib/ai/aiSession.svelte";
	import { treeAiDriver } from "$lib/ai/treeAiSession";

	interface Props {
		isConnected: boolean;
		saveError: boolean;
		connectionGracePeriod: boolean;
	}

	let { isConnected, saveError, connectionGracePeriod }: Props = $props();

	const offline = $derived(!isConnected && !connectionGracePeriod);
</script>

{#if aiSession.active}
	<button class="page-status-note ai" onclick={() => treeAiDriver.abort()} title="Stop">
		<Icon icon="ri:sparkling-2-line" width="13" class="ai-spark" />
		<span>Virtues is writing…</span>
		<Icon icon="ri:stop-circle-line" width="14" />
	</button>
{:else if aiSession.status === "error"}
	<button class="page-status-note error" onclick={() => aiSession.reset()} title={aiSession.error ?? "AI error"}>
		<Icon icon="ri:error-warning-line" width="13" />
		<span>AI couldn’t finish</span>
		<Icon icon="ri:close-line" width="14" />
	</button>
{:else if offline}
	<div class="page-status-note offline" role="status">
		<Icon icon="ri:wifi-off-line" width="13" />
		<span>Offline</span>
	</div>
{:else if saveError}
	<div class="page-status-note error" role="status">
		<Icon icon="ri:error-warning-line" width="13" />
		<span>Couldn’t save</span>
	</div>
{/if}

<style>
	.page-status-note {
		position: absolute;
		right: 16px;
		bottom: 12px;
		z-index: var(--z-sticky);
		display: flex;
		align-items: center;
		gap: 8px;
		padding: 4px 12px;
		border: 1px solid var(--color-border);
		border-radius: var(--card-radius);
		background: var(--color-surface);
		font-family: var(--font-sans);
		font-size: 12px;
		color: var(--color-foreground-muted);
		box-shadow: var(--shadow-float);
		animation: note-in 0.16s ease-out;
	}

	button.page-status-note {
		cursor: pointer;
		transition: background-color 0.12s ease;
	}

	.ai {
		color: var(--color-primary);
	}
	.ai:hover {
		background: var(--color-primary-subtle);
	}

	.error {
		color: var(--color-error);
	}
	button.error:hover {
		background: color-mix(in srgb, var(--color-error) 12%, var(--color-surface));
	}

	.offline {
		color: var(--color-warning);
	}

	:global(.ai-spark) {
		animation: ai-spark-pulse 1.4s ease-in-out infinite;
	}

	@keyframes ai-spark-pulse {
		0%,
		100% {
			opacity: 0.5;
		}
		50% {
			opacity: 1;
		}
	}

	@keyframes note-in {
		from {
			opacity: 0;
			transform: translateY(4px);
		}
		to {
			opacity: 1;
			transform: translateY(0);
		}
	}
</style>
