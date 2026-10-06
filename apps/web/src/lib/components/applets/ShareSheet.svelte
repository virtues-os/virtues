<script lang="ts">
	/**
	 * Share an applet's page as a link anyone can open.
	 *
	 * The sheet shows what leaves before anything does: the page exactly as a
	 * visitor sees it, its images and outside links, and anything that looks
	 * like contact details. Creating a link freezes the page; Update re-freezes
	 * it under the same link; Revoke deletes it. Your server serves the page
	 * itself (`api::publications`, the door), so a link only opens while it is on.
	 */
	import Modal from '$lib/components/Modal.svelte';
	import Button from '$lib/components/Button.svelte';
	import Icon from '$lib/components/Icon.svelte';
	import {
		previewPublication,
		listPublications,
		createPublication,
		updatePublication,
		revokePublication,
		type Publication,
		type SharePreview
	} from '$lib/api/client';

	type Props = {
		open: boolean;
		appletId: string;
		onClose: () => void;
	};

	let { open, appletId, onClose }: Props = $props();

	let preview = $state<SharePreview | null>(null);
	let links = $state<Publication[]>([]);
	let loading = $state(false);
	let busy = $state(false);
	let error = $state<string | null>(null);
	let expiry = $state('30');
	let live = $state(false);
	let copied = $state<string | null>(null);
	let confirmRevoke = $state<string | null>(null);

	const EXPIRY_CHOICES = [
		{ value: '1', label: '1 day' },
		{ value: '7', label: '7 days' },
		{ value: '30', label: '30 days' },
		{ value: '0', label: 'Never' }
	];

	async function load() {
		loading = true;
		error = null;
		try {
			const [p, all] = await Promise.all([previewPublication(appletId), listPublications()]);
			preview = p;
			links = all.filter((l) => l.producer_id === appletId && !l.revoked_at);
		} catch (e) {
			error = e instanceof Error ? e.message : String(e);
		} finally {
			loading = false;
		}
	}

	$effect(() => {
		if (open && appletId) {
			copied = null;
			confirmRevoke = null;
			load();
		}
	});

	// The share server starts when the first link goes live and reports its
	// address a moment later, so a new link may arrive without one.
	async function waitForLink(id: string) {
		for (let i = 0; i < 15; i++) {
			const found = (await listPublications()).find((l) => l.id === id);
			if (found?.link) {
				links = links.map((l) => (l.id === id ? found : l));
				return;
			}
			await new Promise((r) => setTimeout(r, 1000));
		}
	}

	async function create() {
		busy = true;
		error = null;
		try {
			const made = await createPublication(appletId, Number(expiry), preview?.reads_data && live);
			links = [made, ...links];
			if (!made.link) await waitForLink(made.id);
		} catch (e) {
			error = e instanceof Error ? e.message : String(e);
		} finally {
			busy = false;
		}
	}

	async function update(id: string) {
		busy = true;
		error = null;
		try {
			const fresh = await updatePublication(id);
			links = links.map((l) => (l.id === id ? fresh : l));
			preview = await previewPublication(appletId);
		} catch (e) {
			error = e instanceof Error ? e.message : String(e);
		} finally {
			busy = false;
		}
	}

	async function revoke(id: string) {
		busy = true;
		error = null;
		try {
			await revokePublication(id);
			links = links.filter((l) => l.id !== id);
			confirmRevoke = null;
		} catch (e) {
			error = e instanceof Error ? e.message : String(e);
		} finally {
			busy = false;
		}
	}

	async function copy(link: string) {
		try {
			await navigator.clipboard.writeText(link);
			copied = link;
		} catch {
			copied = null;
		}
	}

	function kb(bytes: number): string {
		return bytes < 1024 ? `${bytes} bytes` : `${Math.round(bytes / 1024)} KB`;
	}

	function expires(l: Publication): string {
		if (!l.expires_at) return 'Never expires';
		return `Expires ${new Date(l.expires_at).toLocaleDateString(undefined, { month: 'short', day: 'numeric', year: 'numeric' })}`;
	}

	function opened(l: Publication): string {
		if (l.open_count === 0) return 'Not opened yet';
		return l.open_count === 1 ? 'Opened once' : `Opened ${l.open_count} times`;
	}

	// One row of a query's result, as the few values a person can scan.
	function rowText(row: Record<string, unknown>): string {
		return Object.entries(row)
			.slice(0, 4)
			.map(([k, v]) => `${k}: ${typeof v === 'object' ? JSON.stringify(v) : String(v)}`)
			.join(' · ');
	}

	function host(url: string): string {
		try {
			return new URL(url).host;
		} catch {
			return url;
		}
	}
</script>

<Modal {open} onClose={onClose} title={preview ? `Share ${preview.title}` : 'Share'} width="lg">
	<div class="sheet">
		{#if loading && !preview}
			<p class="hint">Getting the page ready…</p>
		{:else if preview?.problem}
			<div class="warn">
				<Icon icon="ri:error-warning-line" width="16" />
				<div>
					<p class="warn-title">You can't share this page yet</p>
					<p>{preview.problem}</p>
					<p>Ask the assistant to rewrite it so everything is inside the page.</p>
				</div>
			</div>
		{:else if preview?.html}
			<section class="leaves">
				<h3>What leaves your server</h3>
				<iframe
					class="frame"
					title="The page as visitors see it"
					sandbox="allow-scripts"
					srcdoc={preview.html}
				></iframe>
				<ul class="facts">
					<li>This page, {kb(preview.size_bytes)}, exactly as shown above.</li>
					<li>
						{preview.image_count === 0
							? 'No images.'
							: `${preview.image_count} ${preview.image_count === 1 ? 'image' : 'images'}, inside the page.`}
					</li>
					{#if preview.links.length}
						<li>
							Links to {preview.links.map(host).filter((h, i, a) => a.indexOf(h) === i).join(', ')}.
						</li>
					{/if}
				</ul>
				{#if preview.reads_data}
					<div class="data">
						<h3>Data on this page</h3>
						{#each preview.queries as q, i (i)}
							<div class="query">
								<p>
									{q.row_count === 1 ? '1 row' : `${q.row_count} rows`}{q.sample.length
										? ', starting with:'
										: '.'}
								</p>
								{#each q.sample as row, j (j)}
									<p class="row">{rowText(row)}</p>
								{/each}
								<details>
									<summary>Show the query</summary>
									<pre>{q.sql}</pre>
								</details>
							</div>
						{/each}
						<fieldset class="mode">
							<label>
								<input type="radio" name="share-mode" value={false} bind:group={live} />
								<span>
									<strong>Snapshot.</strong> The page carries these rows as they are now. Opening
									it asks your server for nothing.
								</span>
							</label>
							<label>
								<input type="radio" name="share-mode" value={true} bind:group={live} />
								<span>
									<strong>Live.</strong> Your server runs these queries each time someone opens
									the page, so they see current data. It runs only these, read-only.
								</span>
							</label>
						</fieldset>
					</div>
				{/if}
				{#if preview.looks_private.length}
					<div class="warn">
						<Icon icon="ri:eye-line" width="16" />
						<div>
							<p class="warn-title">This looks like contact details</p>
							<p class="private">{preview.looks_private.join(' · ')}</p>
							<p>Anyone with the link will see it. Remove it from the page first if you don't mean to send it.</p>
						</div>
					</div>
				{/if}
			</section>

			{#if links.length}
				<section class="links">
					<h3>Links to this page</h3>
					{#each links as l (l.id)}
						<div class="link-row">
							{#if l.link}
								<code class="link">{l.link}</code>
							{:else}
								<span class="hint">Starting your share server…</span>
							{/if}
							<div class="link-meta">
								{#if preview?.reads_data}
									<span>{l.is_live ? 'Live' : 'Snapshot'}</span>
								{/if}
								<span>{expires(l)}</span>
								<span>{opened(l)}</span>
							</div>
							<div class="link-actions">
								{#if l.link}
									<Button size="sm" variant="secondary" icon="ri:file-copy-line" onclick={() => copy(l.link!)}>
										{copied === l.link ? 'Copied' : 'Copy link'}
									</Button>
								{/if}
								<Button size="sm" variant="ghost" disabled={busy} onclick={() => update(l.id)}>Update to this version</Button>
								{#if confirmRevoke === l.id}
									<Button size="sm" variant="danger" disabled={busy} onclick={() => revoke(l.id)}>
										Turn off this link for everyone
									</Button>
									<Button size="sm" variant="ghost" onclick={() => (confirmRevoke = null)}>Keep it</Button>
								{:else}
									<Button size="sm" variant="ghost" onclick={() => (confirmRevoke = l.id)}>Revoke</Button>
								{/if}
							</div>
						</div>
					{/each}
				</section>
			{/if}

			<section class="create">
				<label class="expiry">
					<span class="label">Link works for</span>
					<select id="share-expiry" bind:value={expiry}>
						{#each EXPIRY_CHOICES as c (c.value)}
							<option value={c.value}>{c.label}</option>
						{/each}
					</select>
				</label>
				<Button variant="primary" icon="ri:link" disabled={busy} onclick={create}>
					{links.length ? 'Create another link' : 'Create link'}
				</Button>
			</section>
			<p class="hint">
				Anyone with the link can open the page. Your server sends it to them encrypted, and
				virtues never stores it. The link only opens while your server is on.
			</p>
		{/if}

		{#if error}
			<div class="error">{error}</div>
		{/if}
	</div>
</Modal>

<style>
	.sheet {
		display: flex;
		flex-direction: column;
		gap: 1.25rem;
	}
	h3 {
		margin: 0 0 0.5rem;
		font-size: 0.8125rem;
		font-weight: 600;
		color: var(--color-foreground-muted);
	}
	.frame {
		width: 100%;
		height: 260px;
		border: 1px solid var(--color-border);
		border-radius: 8px;
		background: var(--color-surface);
	}
	.facts {
		margin: 0.5rem 0 0;
		padding-left: 1.1rem;
		font-size: 0.8125rem;
		line-height: 1.6;
		color: var(--color-foreground);
	}
	.warn {
		display: flex;
		align-items: flex-start;
		gap: 0.5rem;
		margin-top: 0.75rem;
		padding: 0.625rem 0.75rem;
		border-radius: 8px;
		border: 1px solid color-mix(in srgb, var(--color-warning) 35%, transparent);
		background: color-mix(in srgb, var(--color-warning) 10%, transparent);
		color: var(--color-foreground);
		font-size: 0.8125rem;
		line-height: 1.5;
	}
	.warn p {
		margin: 0;
	}
	.warn-title {
		font-weight: 600;
	}
	.data {
		display: flex;
		flex-direction: column;
		gap: 0.75rem;
		margin-top: 1rem;
	}
	.query p {
		margin: 0;
		font-size: 0.8125rem;
	}
	.query .row {
		font-family: var(--font-mono);
		font-size: 0.75rem;
		color: var(--color-foreground-muted);
		overflow-wrap: anywhere;
	}
	details {
		margin-top: 0.25rem;
		font-size: 0.75rem;
		color: var(--color-foreground-muted);
	}
	pre {
		margin: 0.25rem 0 0;
		padding: 0.5rem;
		border-radius: 6px;
		background: var(--color-surface-elevated);
		font-size: 0.75rem;
		white-space: pre-wrap;
		overflow-wrap: anywhere;
	}
	.mode {
		display: flex;
		flex-direction: column;
		gap: 0.5rem;
		margin: 0;
		padding: 0;
		border: 0;
	}
	.mode label {
		display: flex;
		gap: 0.5rem;
		align-items: flex-start;
		font-size: 0.8125rem;
		line-height: 1.5;
	}
	.private {
		font-family: var(--font-mono);
		font-size: 0.75rem;
	}
	.links {
		display: flex;
		flex-direction: column;
		gap: 0.75rem;
	}
	.link-row {
		display: flex;
		flex-direction: column;
		gap: 0.375rem;
		padding: 0.75rem;
		border: 1px solid var(--color-border);
		border-radius: 8px;
		min-width: 0;
	}
	.link {
		font-family: var(--font-mono);
		font-size: 0.75rem;
		overflow-wrap: anywhere;
		color: var(--color-foreground);
	}
	.link-meta {
		display: flex;
		gap: 1rem;
		font-size: 0.75rem;
		color: var(--color-foreground-muted);
	}
	.link-actions {
		display: flex;
		flex-wrap: wrap;
		gap: 0.5rem;
	}
	.create {
		display: flex;
		align-items: flex-end;
		gap: 0.75rem;
		flex-wrap: wrap;
	}
	.expiry {
		display: flex;
		flex-direction: column;
		gap: 0.375rem;
	}
	.label {
		font-size: 0.75rem;
		font-weight: 500;
		color: var(--color-foreground-muted);
	}
	select {
		font: inherit;
		font-size: 0.875rem;
		padding: 0.4rem 0.5rem;
		border: 1px solid var(--color-border);
		border-radius: 6px;
		background: var(--color-surface);
		color: var(--color-foreground);
	}
	.hint {
		margin: 0;
		font-size: 0.75rem;
		line-height: 1.5;
		color: var(--color-foreground-muted);
	}
	.error {
		font-size: 0.8125rem;
		color: var(--color-error);
		padding: 0.5rem 0.625rem;
		background: var(--color-error-bg);
		border-radius: 6px;
	}
</style>
