<script lang="ts">
	// First-run setup nudge. Reads the public box-health endpoint and, until the
	// box is fully set up, shows a single next-step banner — the web echo of the
	// `virtues status` CLI dashboard. CLI-first, so this only nudges; it never
	// blocks. Polls gently while incomplete so it clears itself once the bearer
	// mints / a device pairs.
	import { onMount, onDestroy } from 'svelte';
	import { spaceStore } from '$lib/stores/space.svelte';

	type Gates = {
		infra: boolean;
		identity: boolean;
		linked: boolean;
		entitled: boolean;
		paired: boolean;
	};
	type Health = { ready: boolean; gates: Gates };

	let health = $state<Health | null>(null);
	let dismissed = $state(false);
	let timer: ReturnType<typeof setInterval> | null = null;

	function stopPolling() {
		if (timer) {
			clearInterval(timer);
			timer = null;
		}
	}

	async function refresh() {
		try {
			const res = await fetch('/api/box/health');
			if (res.ok) health = await res.json();
		} catch {
			// transient; keep the last known state
		}
		if (health?.ready) stopPolling();
	}

	onMount(() => {
		dismissed = sessionStorage.getItem('boxSetupNudgeDismissed') === '1';
		refresh();
		timer = setInterval(refresh, 15000);
	});
	onDestroy(stopPolling);

	function dismiss() {
		dismissed = true;
		sessionStorage.setItem('boxSetupNudgeDismissed', '1');
	}

	function openBilling() {
		spaceStore.openTabFromRoute('/virtues/billing', { label: 'Billing' });
	}

	// The single next step, mirroring the CLI dashboard's `next:` line.
	const step = $derived.by(() => {
		const g = health?.gates;
		if (!g || health?.ready) return null;
		if (!g.identity) return { kind: 'identity', text: 'Finishing box setup…' };
		if (!g.linked) return { kind: 'linked', text: 'Connect your subscription to turn on AI.' };
		if (!g.entitled) return { kind: 'entitled', text: 'Activating AI…' };
		if (!g.paired)
			return { kind: 'paired', text: 'Add your phone — open this box in the Virtues iOS app to pair.' };
		return null;
	});
</script>

{#if step && !dismissed}
	<div
		class="fixed bottom-4 left-1/2 -translate-x-1/2 z-40 flex items-center gap-3 px-4 py-2.5 rounded-lg border border-border bg-background shadow-lg text-sm"
	>
		<span class="text-foreground">{step.text}</span>
		{#if step.kind === 'linked'}
			<button
				onclick={openBilling}
				class="px-3 py-1 bg-accent text-on-accent rounded-md text-xs font-medium hover:opacity-90 transition-opacity"
			>
				Connect subscription
			</button>
		{/if}
		<button
			onclick={dismiss}
			aria-label="Dismiss"
			class="text-foreground-muted hover:text-foreground leading-none text-lg"
		>
			×
		</button>
	</div>
{/if}
