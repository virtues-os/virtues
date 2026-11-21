<!--
	The connect window for an account (OAuth through the proxy, and Plaid)
	and for Past chats (agents/plan/sources-redesign-plan.md, slice 6).

	An account opens in steps, held in `connectFlow.pending`:

	  intro    what your server will read, and what your assistant may do there
	  opening  asking your server for the provider's page
	  waiting  that page is open in the browser (the app only; a browser tab
	           navigates to it and comes back to /sources?connected=…)
	  done     the account arrived
	  error    the page could not be opened, with Try again

	Pairing a device and signing in to a site (X) keep their own windows,
	DevicePairModal and ApiKeyConnectModal, which onboarding opens too. All
	of them use the same Modal.
-->
<script lang="ts">
	import Modal from '$lib/components/Modal.svelte';
	import Button from '$lib/components/Button.svelte';
	import Icon from '$lib/components/Icon.svelte';
	import TextAction from '$lib/components/TextAction.svelte';
	import ImportCard from './ImportCard.svelte';
	import OwnAppConnect from './OwnAppConnect.svelte';
	import { IMPORT_SOURCES } from '$lib/imports';
	import { connectFlow } from '$lib/stores/connectFlow.svelte';
	import { sourcesStore } from '$lib/stores/sources.svelte';
	import { windowShellStore } from '$lib/stores/window-shell.svelte';
	import { sourceMark } from '$lib/sources/brands';
	import { SOURCE_CAPABILITIES } from '$lib/sources/capabilities';
	import { reloadOnReturn } from './connectDispatch';

	const pending = $derived(connectFlow.pending);
	const account = $derived(pending.kind === 'account' ? pending : null);
	const source = $derived(account?.source ?? null);
	const name = $derived(source?.name ?? '');
	const mark = $derived(source ? sourceMark(source.id, source.icon) : null);
	const reconnecting = $derived(!!account?.credentialId);

	/** "Email and Calendar events": what connecting gives your server. */
	const reads = $derived.by(() => {
		const list = (source?.provides ?? []).map((p) => p.toLowerCase());
		if (list.length <= 1) return list[0] ?? '';
		return `${list.slice(0, -1).join(', ')} and ${list[list.length - 1]}`;
	});

	/** What your assistant may do there, as the source's switches say it. */
	const mayDo = $derived(
		Object.values(SOURCE_CAPABILITIES[source?.id ?? ''] ?? {}).map((w) => w.label.toLowerCase())
	);

	// The connections this source had when the provider's page opened, so
	// coming back can tell whether a new one arrived.
	let before = 0;

	async function go() {
		before = sourcesStore.bySource.get(source?.id ?? '')?.length ?? 0;
		await connectFlow.continueAccount();
		if (connectFlow.pending.kind === 'account' && connectFlow.pending.step === 'waiting') watch();
	}

	/**
	 * Your server finalizes the sign-in while the person is still in the
	 * browser. Check when they come back; until the account shows, the window
	 * keeps waiting and watches for the next return.
	 */
	function watch() {
		reloadOnReturn(async () => {
			await sourcesStore.load();
			const p = connectFlow.pending;
			if (p.kind !== 'account' || p.step !== 'waiting') return;
			const list = sourcesStore.bySource.get(p.source.id) ?? [];
			const arrived = p.credentialId
				? list.some((c) => c.id === p.credentialId && !c.broken)
				: list.length > before;
			if (arrived) connectFlow.accountConnected();
			else watch();
		});
	}

	function close() {
		connectFlow.close();
		void sourcesStore.load();
	}

	function openSource() {
		if (!source) return;
		const id = source.id;
		close();
		windowShellStore.navigate(`/sources/${id}`, { label: name });
	}
</script>

<Modal open={account !== null} onClose={close} label={name ? `Connect ${name}` : 'Connect'} width="sm">
	{#if account && source && mark}
		<div class="window">
			{#if account.step === 'done'}
				<span class="done" aria-hidden="true"><Icon icon="mynaui:check" width="20" /></span>
				<p class="title">{reconnecting ? `${name} is signed in again` : `${name} is connected`}</p>
				<p class="body">It starts sending now. You can close this window: your server keeps going without it.</p>
			{:else if account.step === 'waiting'}
				<span class="spinner" aria-hidden="true"><Icon icon="mynaui:spinner" width="22" class="animate-spin" /></span>
				<p class="title">Finish signing in to {name}</p>
				<p class="body">{name}'s page opened in your browser. Sign in and allow access there, then come back here.</p>
				<p class="body"><TextAction onclick={() => void go()}>Open {name}'s page again</TextAction></p>
			{:else if account.step === 'error'}
				<span class="tile" aria-hidden="true"><Icon icon={mark.icon} width="22" style={mark.color ? `color: ${mark.color}` : undefined} /></span>
				<p class="title">Couldn't open {name}'s page</p>
				<p class="body">{account.message ?? 'Your server did not answer.'} Try again in a moment.</p>
			{:else}
				<span class="tile" aria-hidden="true"><Icon icon={mark.icon} width="22" style={mark.color ? `color: ${mark.color}` : undefined} /></span>
				<p class="title">{reconnecting ? `Sign in to ${name} again` : `Connect ${name}`}</p>
				{#if reconnecting}
					<p class="body">Your server keeps everything it already has. Once you sign in, it can read what is new.</p>
				{:else if reads}
					<p class="body">Your server reads your {reads} from the account you pick.</p>
				{:else if source.description}
					<p class="body">{source.description}</p>
				{/if}
				{#if mayDo.length > 0 && !reconnecting}
					<p class="body">
						When you ask, your assistant can also {mayDo.join(', and ')}. You can turn this off on {name}'s
						page.
					</p>
				{/if}
			{/if}

			<div class="actions">
				{#if account.step === 'done'}
					<Button variant="ghost" onclick={close}>Done</Button>
					<Button variant="primary" onclick={openSource}>Open {name}</Button>
				{:else if account.step === 'waiting'}
					<Button variant="ghost" onclick={close}>Cancel</Button>
				{:else}
					<Button variant="ghost" onclick={close}>Cancel</Button>
					<Button variant="primary" loading={account.step === 'opening'} onclick={() => void go()}>
						{account.step === 'error' ? 'Try again' : `Continue to ${name}`}
					</Button>
				{/if}
			</div>
		</div>
	{/if}
</Modal>

{#if pending.kind === 'own_app'}
	<OwnAppConnect open credentialId={pending.credentialId} onClose={close} onReturn={() => void sourcesStore.load()} />
{/if}

{#if pending.kind === 'import'}
	<Modal open onClose={close} title={IMPORT_SOURCES[pending.sourceId]?.title ?? 'Import'} width="md">
		<ImportCard sourceId={pending.sourceId} />
	</Modal>
{/if}

<style>
	.window {
		display: flex;
		flex-direction: column;
		min-height: 260px;
	}
	.tile,
	.done,
	.spinner {
		width: 44px;
		height: 44px;
		box-sizing: border-box;
		display: flex;
		align-items: center;
		justify-content: center;
		border-radius: var(--radius-lg);
		color: var(--color-foreground);
	}
	.tile {
		border: 1px solid var(--color-border);
		background: var(--color-background);
	}
	.done {
		border-radius: 50%;
		background: var(--color-foreground);
		color: var(--color-background);
	}
	.title {
		margin: 16px 0 0;
		font-family: var(--font-serif);
		font-size: var(--text-heading);
		line-height: 1.25;
		color: var(--color-foreground);
	}
	.body {
		margin: 10px 0 0;
		font-size: var(--text-label);
		line-height: 1.5;
		color: var(--color-foreground-muted);
	}
	.actions {
		display: flex;
		justify-content: flex-end;
		gap: 8px;
		margin-top: auto;
		padding-top: 24px;
	}
</style>
