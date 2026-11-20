<script lang="ts">
	import type { Tab } from '$lib/tabs/types';
	import Button from '$lib/components/Button.svelte';
	import TextAction from '$lib/components/TextAction.svelte';
	import FaceFrame from '$lib/components/applets/FaceFrame.svelte';
	import SiteSwitch from '$lib/components/applets/SiteSwitch.svelte';
	import { runApplet } from '$lib/api/client';
	import { appletsStore } from '$lib/stores/applets.svelte';
	import { windowShellStore } from '$lib/stores/window-shell.svelte';
	import { usePageActions } from '$lib/stores/paneActions.svelte';
	import { actionOf, problemOf } from '$lib/applets/standing';

	/**
	 * A site's page: its view, filling the pane. The switch to its options
	 * is in the tab's toolbar, and what it needs from you is one sentence
	 * above the view. The id comes from the route `/applet/<id>/view`.
	 */
	let { tab }: { tab: Tab; active: boolean } = $props();

	const appletId = $derived(tab.route.match(/\/(applet_[^/]+)\/view/)?.[1] ?? null);

	const toolbar = usePageActions(() => (appletId ? sides : null));

	// The list answers what the site needs; the view itself needs nothing from it.
	$effect(() => {
		if (!appletsStore.loaded) void appletsStore.load();
	});
	const site = $derived(appletsStore.list.find((a) => a.id === appletId) ?? null);
	const trouble = $derived(site ? problemOf(site) : null);
	const fix = $derived(site ? actionOf(site) : null);

	let starting = $state(false);
	let err = $state<string | null>(null);

	async function act() {
		if (!site || !fix) return;
		if (fix.kind === 'fix') {
			windowShellStore.openTabFromRoute(fix.href, { focusExisting: true });
			return;
		}
		starting = true;
		err = null;
		try {
			await runApplet(site.id);
			await appletsStore.load();
		} catch (e) {
			err = `Your server didn't start it: ${e instanceof Error ? e.message : String(e)}. Try again.`;
		} finally {
			starting = false;
		}
	}
</script>

{#snippet sides()}
	{#if appletId}
		<SiteSwitch tabId={tab.id} {appletId} name={site?.name ?? tab.label} hasView showing="site" />
	{/if}
{/snippet}

<div class="applet-view">
	{#if appletId}
		{#if !toolbar.inToolbar}
			<div class="bar">{@render sides()}</div>
		{/if}
		{#if trouble || err}
			<p class="needs">
				<span>{err ?? `${trouble}.`}</span>
				{#if fix?.kind === 'fix'}
					<Button variant="primary" onclick={act}>{fix.label}</Button>
				{:else if fix}
					<TextAction onclick={act} disabled={starting}>{starting ? 'Starting…' : fix.label}</TextAction>
				{/if}
			</p>
		{/if}
		<FaceFrame {appletId} height="100%" />
	{:else}
		<div class="state"><p class="muted">Your server has no site at this address.</p></div>
	{/if}
</div>

<style>
	.applet-view {
		height: 100%;
		min-height: 0;
		display: flex;
		flex-direction: column;
	}
	/* The view fills the pane, so the frame drops its own border. */
	.applet-view :global(iframe.face-frame) {
		flex: 1;
		border: none;
		border-radius: 0;
	}
	.bar {
		display: flex;
		justify-content: center;
		padding: 8px 16px;
	}
	.needs {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 16px;
		margin: 0;
		padding: 8px 24px;
		border-bottom: 1px solid var(--color-border);
		font-size: var(--text-label);
		color: var(--color-foreground);
	}
	.state {
		display: grid;
		place-items: center;
		height: 100%;
	}
	.muted {
		color: var(--color-foreground-subtle);
		font-size: var(--text-label);
	}
</style>
