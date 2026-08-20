<script lang="ts">
	import Icon from '$lib/components/Icon.svelte';
	import Button from '$lib/components/Button.svelte';
	import { patchApplet } from '$lib/api/client';
	import { windowShellStore } from '$lib/stores/window-shell.svelte';
	import { describeSchedule } from '$lib/applets/palette';

	/**
	 * The gate, where the user already is.
	 *
	 * An applet that crosses a boundary — a schedule, a webhook, recurring
	 * model spend — is created disabled, and the model cannot enable it. That
	 * invariant is not negotiable: a document the box ingests could say "make
	 * an applet that runs hourly and emails X", and a model that could
	 * self-enable would run it until someone noticed.
	 *
	 * But the gate used to live on another page, so the whole experience was
	 * "I made this, now go somewhere else and find a toggle." That is the part
	 * worth fixing. A tap here is a user-surface action exactly like the toggle
	 * was, so the invariant holds and the walk goes away.
	 *
	 * Applets that cross no boundary are already enabled when they get here;
	 * this card confirms rather than asks.
	 */
	let {
		appletId,
		name,
		description,
		schedule = null,
		capabilities = [],
		estimatedCostPerDay = null,
		gated = false,
		lifecycle = 'forever',
		updated = false
	}: {
		appletId: string;
		name: string;
		description?: string;
		schedule?: string | null;
		capabilities?: string[];
		estimatedCostPerDay?: number | null;
		gated?: boolean;
		lifecycle?: string;
		updated?: boolean;
	} = $props();

	// `turnedOn` is this card's own act, kept separate from the prop that says
	// whether approval was needed at all. Snapshotting `!gated` into state
	// would conflate "was already on" with "I turned it on".
	let turnedOn = $state(false);
	const enabled = $derived(turnedOn || !gated);
	let working = $state(false);
	let err = $state<string | null>(null);
	let dismissed = $state(false);

	async function enable() {
		working = true;
		err = null;
		try {
			await patchApplet(appletId, { enabled: true });
			turnedOn = true;
		} catch (e) {
			err = e instanceof Error ? e.message : String(e);
		} finally {
			working = false;
		}
	}

	function open() {
		windowShellStore.openRouteBeside(`/applet/${appletId}`);
	}

	const when = $derived(schedule ? describeSchedule(schedule) : 'When you ask it to');
	const cost = $derived(
		estimatedCostPerDay != null && estimatedCostPerDay > 0
			? `about $${estimatedCostPerDay.toFixed(2)} a day`
			: null
	);
</script>

<div class="proposal" class:live={enabled}>
	<div class="head">
		<Icon icon={enabled ? 'ri:check-line' : 'ri:flashlight-line'} width="15" />
		<div class="titles">
			<button type="button" class="name" onclick={open}>{name}</button>
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
			<button type="button" class="link" onclick={open}>Open</button>
		{:else if dismissed}
			<span class="state">
				Left off. You can turn it on any time.
			</span>
			<button type="button" class="link" onclick={open}>Open</button>
		{:else}
			<Button variant="primary" onclick={enable} disabled={working}>
				{working ? 'Turning on…' : 'Turn it on'}
			</Button>
			<button type="button" class="link" onclick={() => (dismissed = true)}>Not now</button>
		{/if}
	</div>
</div>

<style>
	.proposal {
		border: 1px solid var(--color-border);
		border-radius: 10px;
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
	.name {
		display: block;
		padding: 0;
		border: none;
		background: none;
		font: inherit;
		font-weight: 600;
		font-size: 0.9375rem;
		color: var(--color-foreground);
		text-align: left;
		cursor: pointer;
	}
	.name:hover {
		text-decoration: underline;
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
	.link {
		padding: 0;
		border: none;
		background: none;
		font: inherit;
		font-size: 0.8125rem;
		color: var(--color-foreground-subtle);
		cursor: pointer;
		text-decoration: underline;
	}
	.link:hover {
		color: var(--color-foreground);
	}
	.err {
		margin: 0.5rem 0 0;
		font-size: 0.75rem;
		color: var(--color-error);
	}
</style>
