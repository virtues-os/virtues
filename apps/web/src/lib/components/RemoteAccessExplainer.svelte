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
				Office, dorm, and shared (hostile) wifi can't accept inbound
				connections — there's no way to reach your box from outside,
				no matter how it's paired. Your box still works fully on this
				local network; nothing is broken. But this isn't a suitable
				home for a box you want to reach from your other devices.
			</p>

			<div class="p-4 rounded-lg bg-surface-alt border border-border">
				<div class="text-foreground font-medium mb-1">
					Best fix: move the box to a network you control
				</div>
				<p class="text-foreground-muted">
					Home internet with a normal router (ideally IPv6) just works
					— it re-checks automatically.
				</p>
			</div>

			<details class="rounded-lg bg-surface-alt border border-border">
				<summary
					class="px-4 py-3 cursor-pointer text-foreground-muted hover:text-foreground transition-colors"
				>
					If you must stay on this network: bring your own networking
				</summary>
				<div class="px-4 pb-4 space-y-2 text-foreground-muted">
					<p>
						Reaching a box on a network you don't control requires an
						overlay you run yourself — a WireGuard VPS, Headscale, or
						installing Tailscale on the box and all your devices to
						form a private mesh. It's real setup, and you'll likely
						need it for as long as the box lives here. Virtues never
						runs or requires an overlay — but it works fine over one.
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
