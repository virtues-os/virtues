<script lang="ts">
	/**
	 * Settings → Box → Updates.
	 *
	 * Checks on open rather than on a timer: a background poll means the box
	 * making periodic outbound calls to GitHub on its own initiative, which
	 * isn't something an appliance holding someone's whole life should do
	 * unprompted.
	 *
	 * Doesn't apply the update. `virtues upgrade` needs root — it writes
	 * /usr/local/bin and drives systemctl — and the sudoers grant that would let
	 * the server invoke it ships with the installer work. Until then this shows
	 * the command rather than pretending a button will do it.
	 */
	import { onMount } from 'svelte';
	import Icon from '$lib/components/Icon.svelte';
	import { getUpdateStatus, setUpdateChannel, type UpdateStatus } from '$lib/api/client';

	let status = $state<UpdateStatus | null>(null);
	let loading = $state(true);
	let switching = $state(false);

	async function check() {
		loading = true;
		try {
			status = await getUpdateStatus();
		} catch (err) {
			console.error('[updates] check failed:', err);
			status = null;
		} finally {
			loading = false;
		}
	}

	async function switchChannel(channel: 'stable' | 'prerelease') {
		if (switching || status?.channel === channel) return;
		switching = true;
		try {
			await setUpdateChannel(channel);
			await check();
		} catch (err) {
			console.error('[updates] channel switch failed:', err);
		} finally {
			switching = false;
		}
	}

	onMount(check);

	// A box on prerelease is normally *ahead* of the newest stable tag, and the
	// upgrade path refuses to move backwards. Switching to stable therefore means
	// "stop taking prereleases" — it doesn't roll anything back. Saying so is the
	// difference between a considered design and an apparently broken setting.
	const aheadOfStable = $derived(status?.channel === 'prerelease');
</script>

<section class="updates">
	<header>
		<h3>Updates</h3>
		<button class="check-btn" onclick={check} disabled={loading}>
			<Icon icon={loading ? 'ri:loader-4-line' : 'ri:refresh-line'} width="14" />
			<span>{loading ? 'Checking…' : 'Check again'}</span>
		</button>
	</header>

	{#if status}
		<dl class="facts">
			<dt>Running</dt>
			<dd>{status.current}</dd>
			<dt>Channel</dt>
			<dd>
				<div class="channel-picker">
					<button
						class="channel"
						class:selected={status.channel === 'stable'}
						disabled={switching}
						onclick={() => switchChannel('stable')}
					>
						Main <span class="hint">recommended</span>
					</button>
					<button
						class="channel"
						class:selected={status.channel === 'prerelease'}
						disabled={switching}
						onclick={() => switchChannel('prerelease')}
					>
						Nightly
					</button>
				</div>
			</dd>
		</dl>

		{#if status.check_error}
			<!-- "Couldn't check" and "up to date" are very different claims, and
			     showing the second when the first is true is the kind of lie that
			     leaves a box unpatched. -->
			<p class="state error">
				<Icon icon="ri:error-warning-line" width="14" />
				Couldn't check for updates — {status.check_error}
			</p>
		{:else if status.update_available}
			<div class="state available">
				<p>
					<Icon icon="ri:download-cloud-line" width="14" />
					{#if status.latest}<strong>{status.latest}</strong> is available{:else}An update
						is available{/if}
				</p>
				<p class="how">Run this on the box to install it:</p>
				<code>sudo virtues upgrade</code>
				<p class="warn">
					The box restarts and runs migrations, which disconnects every
					device using it — not just this one.
				</p>
			</div>
		{:else}
			<p class="state ok">
				<Icon icon="ri:check-line" width="14" />
				Up to date
			</p>
		{/if}

		{#if aheadOfStable}
			<p class="note">
				On Nightly you're usually ahead of the last stable release. Switching
				back to Main stops new prereleases; it doesn't move the box backwards,
				so nothing changes until stable catches up.
			</p>
		{/if}
	{:else if !loading}
		<p class="state error">Couldn't reach the box for update status.</p>
	{/if}
</section>

<style>
	.updates {
		display: flex;
		flex-direction: column;
		gap: 12px;
		padding: 16px 0;
	}

	header {
		display: flex;
		align-items: center;
		justify-content: space-between;
	}

	h3 {
		font-size: 14px;
		font-weight: 600;
		margin: 0;
	}

	.check-btn {
		display: inline-flex;
		align-items: center;
		gap: 5px;
		padding: 4px 8px;
		border: 1px solid var(--color-border);
		border-radius: 6px;
		background: none;
		cursor: pointer;
		font-size: 12px;
		color: var(--color-foreground-muted);
	}

	.check-btn:hover:not(:disabled) {
		background: color-mix(in srgb, var(--color-foreground) 6%, transparent);
		color: var(--color-foreground);
	}

	.facts {
		display: grid;
		grid-template-columns: auto 1fr;
		gap: 8px 16px;
		align-items: center;
		margin: 0;
		font-size: 13px;
	}

	dt {
		color: var(--color-foreground-subtle);
	}

	dd {
		margin: 0;
	}

	.channel-picker {
		display: inline-flex;
		gap: 4px;
	}

	.channel {
		display: inline-flex;
		align-items: baseline;
		gap: 5px;
		padding: 4px 10px;
		border: 1px solid var(--color-border);
		border-radius: 6px;
		background: none;
		cursor: pointer;
		font-size: 12px;
		color: var(--color-foreground-muted);
	}

	.channel.selected {
		border-color: var(--color-primary);
		color: var(--color-foreground);
		background: var(--primary-subtle);
	}

	.hint {
		font-size: 10px;
		color: var(--color-foreground-subtle);
	}

	.state {
		display: flex;
		align-items: center;
		gap: 6px;
		font-size: 13px;
		margin: 0;
	}

	.state.ok {
		color: var(--success);
	}

	.state.error {
		color: var(--error);
	}

	.available {
		display: flex;
		flex-direction: column;
		gap: 6px;
		padding: 12px;
		border: 1px solid var(--color-border);
		border-radius: 8px;
		background: var(--color-surface-elevated);
		font-size: 13px;
	}

	.available p {
		display: flex;
		align-items: center;
		gap: 6px;
		margin: 0;
	}

	.available code {
		display: block;
		padding: 6px 10px;
		border-radius: 6px;
		background: var(--color-background);
		font-family: var(--font-mono, monospace);
		font-size: 12px;
	}

	.how,
	.warn,
	.note {
		color: var(--color-foreground-subtle);
		font-size: 12px;
	}

	.note {
		margin: 0;
		line-height: 1.5;
	}
</style>
