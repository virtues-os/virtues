<!--
  Remote access explainer — opened from the next-wins checklist when the
  remote_access verdict is "not available from this network".

  The verdict line comes verbatim from the server (`detail`); this modal only
  adds context and options. This is deliberately the ONLY user-facing surface
  that mentions BYO/overlay networking — it appears at the moment of intent,
  never during setup.
-->
<script lang="ts">
	import Modal from "$lib/components/Modal.svelte";

	interface Props {
		open?: boolean;
		onClose: () => void;
		/** Server-authored verdict line, rendered verbatim. */
		detail?: string;
		/** Cosmetic hint from the server ("ipv6_direct" | "byo" | net class). Unused for behavior. */
		kind?: string;
	}

	// `kind` stays in Props so the shape matches the server step, but it's
	// cosmetic-only per the setup-state contract — not destructured until a
	// rendering actually varies on it.
	let { open = false, onClose, detail }: Props = $props();
</script>

<Modal {open} {onClose} title="Remote access" width="md">
	{#snippet children()}
		<div class="space-y-4 text-sm">
			{#if detail}
				<!-- The factual status, straight from the box's own check -->
				<p class="font-medium text-foreground">{detail}</p>
			{/if}

			<p class="text-foreground-muted">
				Office, dorm, and shared networks can't accept direct
				connections. Your box works fully on its local network —
				nothing is broken.
			</p>

			<div class="p-4 rounded-lg bg-surface-alt border border-border">
				<div class="text-foreground font-medium mb-1">
					Move the box to a network you control
				</div>
				<p class="text-foreground-muted">
					It re-checks automatically.
				</p>
			</div>

			<details class="rounded-lg bg-surface-alt border border-border">
				<summary
					class="px-4 py-3 cursor-pointer text-foreground-muted hover:text-foreground transition-colors"
				>
					Bring your own network
				</summary>
				<div class="px-4 pb-4 space-y-2 text-foreground-muted">
					<p>
						You can put the box on an overlay network you run
						yourself — Tailscale, Headscale, or a WireGuard VPS.
						Virtues never runs or requires one.
					</p>
					<a
						href="https://github.com/jaces-com/virtues/blob/main/docs/byo-networking.md"
						target="_blank"
						rel="noopener noreferrer"
						class="text-primary hover:underline inline-block"
					>
						Read the BYO networking guide
					</a>
				</div>
			</details>
		</div>
	{/snippet}
</Modal>
