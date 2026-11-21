<script lang="ts">
	import Icon from '$lib/components/Icon.svelte';
	import Button from '$lib/components/Button.svelte';
	import TextAction from '$lib/components/TextAction.svelte';
	import { patchApplet } from '$lib/api/client';
	import { windowShellStore } from '$lib/stores/window-shell.svelte';
	import { describeTriggers } from '$lib/applets/palette';
	import { appletsStore } from '$lib/stores/applets.svelte';
	import { openSiteBeside } from '$lib/applets/sites.svelte';
	import { homeRoute } from '$lib/applets/routes';
	import { homeOf } from '$lib/applets/kind';

	/**
	 * The receipt for an applet the assistant made or changed: what it does,
	 * when it runs, what it may spend, and a way to turn it off. An applet
	 * the assistant makes starts on under its limit (decided 2026-10-06), so
	 * the card asks nothing.
	 */
	let {
		appletId,
		name,
		description,
		schedule = null,
		capabilities = [],
		estimatedCostPerDay = null,
		enabled: startedOn = true,
		lifecycle = 'forever',
		updated = false,
		hasView = null
	}: {
		appletId: string;
		name: string;
		description?: string;
		schedule?: string | null;
		capabilities?: string[];
		estimatedCostPerDay?: number | null;
		enabled?: boolean;
		lifecycle?: string;
		updated?: boolean;
		/** Whether setup said it has a view. Null on a result stored before
		 *  setup reported it, where the applets list answers. */
		hasView?: boolean | null;
	} = $props();

	// What this card last set, over what setup reported.
	let toggled = $state<boolean | null>(null);
	const enabled = $derived(toggled ?? startedOn);
	let working = $state(false);
	let err = $state<string | null>(null);

	async function setEnabled(on: boolean) {
		working = true;
		err = null;
		try {
			await patchApplet(appletId, { enabled: on });
			toggled = on;
		} catch (e) {
			err = `Your server didn't save that: ${e instanceof Error ? e.message : String(e)}. Try again.`;
		} finally {
			working = false;
		}
	}

	// A site opens as its view. Anything else opens the page that says how it runs.
	const site = $derived(hasView ?? appletsStore.list.find((a) => a.id === appletId)?.has_face ?? false);

	function open() {
		if (site) openSiteBeside(appletId, name);
		else {
			// The list says what kind it is. Without it, the page corrects its own address.
			const known = appletsStore.list.find((a) => a.id === appletId);
			windowShellStore.openRouteBeside(known ? homeOf(known) : homeRoute(appletId));
		}
	}

	// Setup's result names the schedule. A data trigger comes from the
	// applets list, loaded once when this applet is not in it yet.
	let looked = false;
	$effect(() => {
		if (looked || (schedule && hasView !== null) || appletsStore.list.some((a) => a.id === appletId)) return;
		looked = true;
		void appletsStore.load();
	});
	const watch = $derived(appletsStore.list.find((a) => a.id === appletId)?.watch ?? null);
	const when = $derived(
		schedule || watch ? describeTriggers({ schedule: schedule ?? null, watch }) : 'When you ask it to'
	);
	const cost = $derived(
		estimatedCostPerDay != null && estimatedCostPerDay > 0
			? `about $${estimatedCostPerDay.toFixed(2)} a day`
			: null
	);
</script>

{#if site}
	<!-- A site is open beside the chat, so the chat keeps one line for it:
	     its name, when it refreshes, what that costs, and the switch. -->
	<div class="site">
		<div class="head">
			<Icon icon="atlas:applets" width="15" />
			<div class="titles">
				<TextAction onclick={open}>{name}</TextAction>
				{#if description}
					<p class="desc">{description}</p>
				{/if}
			</div>
		</div>
		{#if err}
			<p class="err">{err}</p>
		{/if}
		<div class="actions">
			{#if schedule || watch}
				<span class="state">{enabled ? when : 'Off'}{cost && enabled ? ` · ${cost} (estimated)` : ''}</span>
			{/if}
			<TextAction quiet onclick={open}>Open</TextAction>
			{#if schedule || watch}
				{#if enabled}
					<TextAction quiet onclick={() => setEnabled(false)}>Turn off</TextAction>
				{:else}
					<Button variant="secondary" onclick={() => setEnabled(true)} loading={working}>Turn on</Button>
				{/if}
			{/if}
		</div>
	</div>
{:else}
<div class="proposal" class:live={enabled}>
	<div class="head">
		<Icon icon={enabled ? 'mynaui:check' : 'mynaui:zap'} width="15" />
		<div class="titles">
			<TextAction onclick={open}>{name}</TextAction>
			{#if description}
				<p class="desc">{description}</p>
			{/if}
		</div>
	</div>

	<dl class="facts">
		<div><dt>Runs</dt><dd>{when}</dd></div>
		{#if lifecycle === 'once'}
			<div><dt>Then</dt><dd>finishes</dd></div>
		{/if}
		{#if cost}
			<!-- An estimate from the schedule and a per-run constant, not a
			     measurement. The detail page shows what it has actually spent. -->
			<div><dt>Costs</dt><dd>{cost} (estimated)</dd></div>
		{/if}
		{#if capabilities.length > 0}
			<div><dt>Can</dt><dd>{capabilities.join(' · ')}</dd></div>
		{/if}
	</dl>

	{#if err}
		<p class="err">{err}</p>
	{/if}

	<div class="actions">
		{#if enabled}
			<span class="state">
				{updated ? 'Updated and running.' : 'Running.'}
			</span>
			<TextAction quiet onclick={open}>Open</TextAction>
			<TextAction quiet onclick={() => setEnabled(false)}>Turn off</TextAction>
		{:else}
			<span class="state">Off.</span>
			<TextAction quiet onclick={open}>Open</TextAction>
			<Button variant="secondary" onclick={() => setEnabled(true)} loading={working}>Turn on</Button>
		{/if}
	</div>
</div>
{/if}

<style>
	.site {
		border-top: 1px solid var(--color-border);
		border-bottom: 1px solid var(--color-border);
		padding: 0.75rem 0;
		margin-bottom: 0.75rem;
		max-width: 34rem;
	}
	.proposal {
		border: 1px solid var(--color-border);
		border-radius: var(--radius-xl);
		background: var(--color-surface);
		padding: 0.75rem 0.875rem;
		margin-bottom: 0.75rem;
		max-width: 34rem;
	}
	.proposal.live {
		border-color: color-mix(in srgb, var(--color-success) 40%, var(--color-border));
	}
	.head {
		display: flex;
		gap: 0.5rem;
		align-items: flex-start;
	}
	.head :global(svg) {
		margin-top: 0.15rem;
		color: var(--color-foreground-subtle);
		flex-shrink: 0;
	}
	.titles {
		min-width: 0;
	}
	.desc {
		margin: 0.125rem 0 0;
		font-family: var(--font-serif, Georgia, serif);
		font-size: 0.8125rem;
		line-height: 1.45;
		color: var(--color-foreground-muted);
	}
	.facts {
		margin: 0.625rem 0 0;
		display: flex;
		flex-direction: column;
		gap: 0.2rem;
		font-size: 0.75rem;
	}
	.facts div {
		display: flex;
		gap: 0.5rem;
	}
	.facts dt {
		flex: 0 0 3.25rem;
		color: var(--color-foreground-subtle);
	}
	.facts dd {
		margin: 0;
		color: var(--color-foreground-muted);
		min-width: 0;
	}
	.actions {
		display: flex;
		align-items: center;
		gap: 0.625rem;
		margin-top: 0.75rem;
	}
	.state {
		font-size: 0.8125rem;
		color: var(--color-foreground-muted);
	}
	.err {
		margin: 0.5rem 0 0;
		font-size: 0.75rem;
		color: var(--color-error);
	}
</style>
