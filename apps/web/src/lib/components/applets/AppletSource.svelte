<!--
	AppletSource — read the code that ran.

	Every applet gets this, whatever its provenance. For the reader who wants it
	this is the point of an open appliance; for the reader who never opens it,
	knowing it is there is the reassurance. Both are served by the same pane, so
	it is not gated behind a developer mode.

	Read-only. Editing an applet is a fork into the state root, which is a
	different surface with a different lifecycle.
-->
<script lang="ts">
	import Icon from '$lib/components/Icon.svelte';
	import {
		getAppletSource,
		getAppletSourceFile,
		forkApplet,
		type AppletSourceListing
	} from '$lib/api/client';

	let { appletId }: { appletId: string } = $props();

	let forking = $state(false);

	let listing = $state<AppletSourceListing | null>(null);
	let selected = $state<string | null>(null);
	let text = $state<string | null>(null);
	let loading = $state(true);
	let fileLoading = $state(false);
	let err = $state<string | null>(null);

	$effect(() => {
		const id = appletId;
		loading = true;
		err = null;
		getAppletSource(id)
			.then((l) => {
				listing = l;
				// Open the manifest by default — the listing sorts it first, and
				// it's what someone checking "what does this thing do" wants.
				const first = l.files.find((f) => f.readable);
				if (first) void open(first.path);
			})
			.catch((e) => (err = e instanceof Error ? e.message : String(e)))
			.finally(() => (loading = false));
	});

	async function open(path: string) {
		selected = path;
		fileLoading = true;
		text = null;
		try {
			const res = await getAppletSourceFile(appletId, path);
			text = res.text;
		} catch (e) {
			text = null;
			err = e instanceof Error ? e.message : String(e);
		} finally {
			fileLoading = false;
		}
	}

	async function fork() {
		forking = true;
		err = null;
		try {
			await forkApplet(appletId);
			listing = await getAppletSource(appletId);
		} catch (e) {
			err = e instanceof Error ? e.message : String(e);
		} finally {
			forking = false;
		}
	}

	function humanSize(n: number): string {
		if (n < 1024) return `${n} B`;
		if (n < 1024 * 1024) return `${Math.round(n / 1024)} KB`;
		return `${(n / (1024 * 1024)).toFixed(1)} MB`;
	}
</script>

<div class="source">
	{#if loading}
		<p class="muted">Loading source…</p>
	{:else if err && !listing}
		<p class="muted">{err}</p>
	{:else if listing}
		<div class="provenance">
			<Icon icon={listing.origin_root === 'shipped' ? 'ri:box-3-line' : 'ri:edit-line'} width="14" />
			<span>
				{listing.origin_root === 'shipped'
					? 'Shipped with Virtues'
					: 'Lives on this box — authored, imported, or forked'}
				· <code>{listing.dir}</code>
			</span>
			{#if listing.origin_root === 'shipped'}
				<!-- Copies the folder onto this box; the shipped version is
				     untouched, and deleting the copy reverts. -->
				<button type="button" class="fork" disabled={forking} onclick={() => void fork()}>
					{forking ? 'Copying…' : 'Make it mine'}
				</button>
			{/if}
		</div>

		{#if err}
			<p class="muted">{err}</p>
		{/if}

		<div class="panes">
			<ul class="files">
				{#each listing.files as f (f.path)}
					<li>
						<button
							type="button"
							class:active={selected === f.path}
							disabled={!f.readable}
							title={f.readable ? f.path : `${f.path} — not displayable`}
							onclick={() => void open(f.path)}
						>
							<span class="name">{f.path}</span>
							<span class="size">{humanSize(f.size)}</span>
						</button>
					</li>
				{/each}
				{#if listing.truncated}
					<li class="more">More files than can be listed here.</li>
				{/if}
			</ul>

			<div class="viewer">
				{#if fileLoading}
					<p class="muted">Loading…</p>
				{:else if text !== null}
					<pre>{text}</pre>
				{:else}
					<p class="muted">Pick a file.</p>
				{/if}
			</div>
		</div>
	{/if}
</div>

<style>
	.source {
		display: flex;
		flex-direction: column;
		gap: 0.625rem;
	}
	.muted {
		font-size: 0.8125rem;
		color: var(--color-foreground-muted, #6b7280);
		margin: 0;
	}

	.provenance {
		display: flex;
		align-items: center;
		gap: 0.375rem;
		font-size: 0.75rem;
		color: var(--color-foreground-muted, #6b7280);
	}
	.provenance code {
		font-size: 0.6875rem;
	}
	.fork {
		margin-left: auto;
		padding: 0.1875rem 0.5rem;
		border-radius: 5px;
		border: 1px solid var(--color-border, #d1d5db);
		background: var(--color-background, #fff);
		color: var(--color-foreground, #111827);
		font-size: 0.6875rem;
		font-weight: 500;
		cursor: pointer;
	}
	.fork:hover:not(:disabled) {
		background: var(--color-muted, #f3f4f6);
	}

	.panes {
		display: grid;
		grid-template-columns: minmax(10rem, 16rem) 1fr;
		gap: 0.75rem;
		align-items: start;
	}

	.files {
		list-style: none;
		margin: 0;
		padding: 0;
		border: 1px solid var(--color-border, #e5e7eb);
		border-radius: 8px;
		overflow: hidden;
		max-height: 26rem;
		overflow-y: auto;
	}
	.files button {
		display: flex;
		align-items: baseline;
		gap: 0.5rem;
		width: 100%;
		padding: 0.3125rem 0.625rem;
		border: none;
		background: none;
		text-align: left;
		font-size: 0.75rem;
		color: var(--color-foreground, #111827);
		cursor: pointer;
	}
	.files button:hover:not(:disabled) {
		background: var(--color-muted, #f3f4f6);
	}
	.files button.active {
		background: color-mix(in srgb, var(--color-foreground) 8%, transparent);
		font-weight: 500;
	}
	.files button:disabled {
		color: var(--color-foreground-subtle, #9ca3af);
		cursor: default;
	}
	.name {
		flex: 1;
		min-width: 0;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.size {
		flex-shrink: 0;
		font-size: 0.6875rem;
		color: var(--color-foreground-subtle, #9ca3af);
	}
	.more {
		padding: 0.3125rem 0.625rem;
		font-size: 0.6875rem;
		color: var(--color-foreground-subtle, #9ca3af);
	}

	.viewer {
		border: 1px solid var(--color-border, #e5e7eb);
		border-radius: 8px;
		max-height: 26rem;
		overflow: auto;
		padding: 0.625rem 0.75rem;
	}
	.viewer pre {
		margin: 0;
		font-family: var(--font-mono, ui-monospace, monospace);
		font-size: 0.6875rem;
		line-height: 1.55;
		white-space: pre;
		color: var(--color-foreground, #111827);
	}

	@media (max-width: 720px) {
		.panes {
			grid-template-columns: 1fr;
		}
	}
</style>
