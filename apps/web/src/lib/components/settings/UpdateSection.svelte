<script lang="ts">
	/**
	 * Settings → Box → Updates.
	 *
	 * Checks on open rather than on a timer: a background poll means the box
	 * making periodic outbound calls to GitHub on its own initiative, which
	 * isn't something an appliance holding someone's whole life should do
	 * unprompted.
	 *
	 * Applying restarts the box. That is not a page reload — the binary is
	 * swapped and migrations run, and every connected device drops, not just
	 * the one that pressed the button. Hence the confirm that names the blast
	 * radius, and the waiting state afterwards: the request cannot report a
	 * result, because the process serving it is the process being replaced.
	 * The only honest signal is the box going away and coming back.
	 */
	import { onMount, onDestroy } from 'svelte';
	import Icon from '$lib/components/Icon.svelte';
	import {
		getUpdateStatus,
		setUpdateChannel,
		applyUpdate,
		type UpdateStatus
	} from '$lib/api/client';
	import { confirmAction } from '$lib/stores/dialog.svelte';

	let status = $state<UpdateStatus | null>(null);
	let loading = $state(true);
	let switching = $state(false);

	/** null = idle. Otherwise the box is being replaced under us. */
	let restart = $state<{ phase: 'going' | 'back'; error?: string } | null>(null);
	let applyError = $state<string | null>(null);
	let watchTimer: ReturnType<typeof setTimeout> | null = null;

	onDestroy(() => {
		if (watchTimer) clearTimeout(watchTimer);
	});

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

	async function install() {
		if (restart) return;
		applyError = null;

		// Two genuinely different waits, so two different warnings. On the
		// stable channel the box fetches releases ahead of time, which turns
		// this from a download into a restart — quoting "a minute or two" for
		// something that takes fifteen seconds teaches people to discount the
		// next estimate, and the estimate matters most when it's the long one.
		const ok = await confirmAction({
			title: status?.latest ? `Install ${status.latest}?` : 'Install this update?',
			body: staged
				? 'This release is already downloaded. The box runs its migrations and ' +
					'restarts, which takes well under a minute. Every device connected to ' +
					'it drops — phones and other browsers included, not just this window. ' +
					'Nothing is lost; they reconnect on their own.'
				: 'The box downloads the release, swaps its binary and runs migrations, ' +
					'so it stops serving for a minute or two. Every device connected to it ' +
					'drops — phones and other browsers included, not just this window. ' +
					'Nothing is lost; they reconnect on their own.',
			confirmLabel: 'Install and restart',
			cancelLabel: 'Not now'
		});
		if (!ok) return;

		try {
			await applyUpdate();
			restart = { phase: 'going' };
			watchForBox();
		} catch (err) {
			// The box's own words. A bare "update failed" is what sends someone
			// to SSH into the box to find out what actually happened.
			applyError = err instanceof Error ? err.message : String(err);
		}
	}

	/**
	 * Watch the box through the restart.
	 *
	 * `/health` is the probe rather than the update endpoint: it's the cheapest
	 * thing that proves the new binary is genuinely serving, and it's what the
	 * box itself considers "up".
	 *
	 * Two phases, because the interesting failure is invisible otherwise. A box
	 * that never goes down means the upgrade didn't start — reporting "back up"
	 * because it answered immediately would be a false success. So we wait to
	 * see it leave before we accept it returning.
	 */
	function watchForBox() {
		const startedAt = Date.now();
		// Long enough to cover a migration on a slow box; short enough that a
		// genuinely bricked upgrade doesn't spin forever.
		const LIMIT_MS = 10 * 60 * 1000;
		let sawItGo = false;

		const poll = async () => {
			if (Date.now() - startedAt > LIMIT_MS) {
				restart = {
					phase: 'going',
					error:
						"The box hasn't come back after ten minutes. It may have rolled " +
						'itself back — check `journalctl -u virtues-upgrade` on the box.'
				};
				return;
			}

			let alive = false;
			try {
				const res = await fetch('/health', { cache: 'no-store' });
				alive = res.ok;
			} catch {
				alive = false;
			}

			if (!alive) {
				sawItGo = true;
				restart = { phase: 'going' };
			} else if (sawItGo) {
				// Back, and on the new build. A reload is genuinely right here:
				// the served assets have changed underneath this tab.
				restart = { phase: 'back' };
				setTimeout(() => location.reload(), 1200);
				return;
			}

			watchTimer = setTimeout(poll, 2000);
		};

		watchTimer = setTimeout(poll, 2000);
	}

	onMount(check);

	// A box on prerelease is normally *ahead* of the newest stable tag, and the
	// upgrade path refuses to move backwards. Switching to stable therefore means
	// "stop taking prereleases" — it doesn't roll anything back. Saying so is the
	// difference between a considered design and an apparently broken setting.
	const aheadOfStable = $derived(status?.channel === 'prerelease');

	// The box fetches stable releases on a schedule, so by the time anyone opens
	// this screen the update is usually already here.
	const staged = $derived(status?.staged ?? null);
</script>

<section class="updates">
	<header>
		<h3>Updates</h3>
		<button class="check-btn" onclick={check} disabled={loading}>
			<Icon icon={loading ? 'ri:loader-4-line' : 'ri:refresh-line'} width="14" />
			<span>{loading ? 'Checking…' : 'Check again'}</span>
		</button>
	</header>

	{#if restart}
		<!-- Takes over the section while the box is being replaced. Everything
		     below it is about a box that is, right now, not there. -->
		<div class="state restarting" class:failed={!!restart.error}>
			{#if restart.error}
				<p>
					<Icon icon="ri:error-warning-line" width="14" />
					{restart.error}
				</p>
			{:else if restart.phase === 'back'}
				<p>
					<Icon icon="ri:check-line" width="14" />
					Back up. Reloading…
				</p>
			{:else}
				<p>
					<Icon icon="ri:loader-4-line" width="14" class="spin" />
					Installing. The box is restarting and will be back in a minute or two.
				</p>
				<p class="how">
					Other devices are disconnected too; they reconnect on their own.
				</p>
			{/if}
		</div>
	{/if}

	{#if status}
		<dl class="facts">
			<dt>Running</dt>
			<dd class="running">
				<!-- RELEASE IDENTITY, not the build counter. This row used to
				     print `current` (the crate version), which is the same
				     string on every prerelease build — so a box on `edge` was
				     told it was running "0.3.0", the number of a release it is
				     ahead of. `codename::version()` exists precisely so every
				     surface says the same word; this one wasn't asking. -->
				<span class="ver">{status.running_version}</span>
				{#if status.running_channel && status.running_channel !== 'stable'}
					<span class="track">{status.running_channel}</span>
				{/if}
				<span class="counter">build {status.current}</span>
			</dd>
			<dt>Channel</dt>
			<dd>
				<!-- A SELECT, NOT A TOGGLE. Two buttons side by side read as a
				     preference with two equally good answers; one of these ships
				     unreviewed builds to the machine holding someone's record.
				     A select has a default and makes the other option a
				     deliberate reach, which is the honest shape of this
				     decision. -->
				<div class="channel-picker">
					<select
						id="channel"
						disabled={switching}
						value={status.channel}
						onchange={(e) =>
							switchChannel(e.currentTarget.value as 'stable' | 'prerelease')}
					>
						<option value="stable">Main — released builds</option>
						<option value="prerelease">Nightly — unreleased, may break</option>
					</select>
					{#if status.channel === 'prerelease'}
						<span class="risk">
							<Icon icon="ri:alert-line" width="13" />
							Unreviewed builds install on this box
						</span>
					{/if}
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
		{:else if status.running_ahead}
			<!-- Neither "up to date" nor "update available" is true here, and
			     both were lies: this box came off a later track than the channel
			     it follows, so the newest tag on that channel is probably behind
			     it. Say what is running, say what the channel holds, and let the
			     owner decide. -->
			<p class="state ahead">
				<Icon icon="ri:git-branch-line" width="14" />
				This box runs <strong>{status.running_version}</strong>, from the
				{status.running_channel} track — ahead of
				{status.latest ?? 'the latest release'} on Main. Nothing to install;
				Main will catch up.
			</p>
		{:else if status.update_available}
			<div class="state available">
				<p>
					<Icon icon={staged ? 'ri:check-double-line' : 'ri:download-cloud-line'} width="14" />
					{#if status.latest}<strong>{status.latest}</strong>{:else}An update{/if}
					{staged ? 'is downloaded and ready' : 'is available'}
				</p>
				<p class="warn">
					{#if staged}
						Already fetched and checked against this box, so installing is a
						restart — under a minute. It disconnects every device using the
						box, not just this one.
					{:else}
						The box downloads it, restarts and runs migrations, which
						disconnects every device using it — not just this one.
					{/if}
				</p>
				<button class="install-btn" onclick={install} disabled={!!restart}>
					<Icon icon={staged ? 'ri:restart-line' : 'ri:download-2-line'} width="14" />
					<span>{staged ? 'Install and restart' : 'Download and install'}</span>
				</button>
				{#if applyError}
					<!-- The box's own reason, verbatim. The common one is that this
					     is a dev checkout with no /usr/local/bin/virtues, which is
					     worth saying plainly rather than dressing up as a failure. -->
					<p class="apply-error">
						<Icon icon="ri:error-warning-line" width="14" />
						{applyError}
					</p>
					<p class="how">You can always run it on the box yourself:</p>
					<code>sudo virtues upgrade</code>
				{/if}
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
	/* Carries its own gutter, matching the Page shell's, because Settings
	   renders this section as a bare sibling of a Page-shelled view (`box` =
	   UpdateSection + SystemInfoView). With no horizontal padding it sat flush
	   to the window edge while the System page below it was inset — legible as
	   a mistake at any width, glaring at 375px. */
	.updates {
		display: flex;
		flex-direction: column;
		gap: 12px;
		padding: 16px 1.25rem;
	}

	@media (min-width: 768px) {
		.updates {
			padding: 16px 3rem;
		}
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

	.running {
		display: flex;
		align-items: baseline;
		flex-wrap: wrap;
		gap: 8px;
	}

	.ver {
		font-variant-numeric: tabular-nums;
	}

	/* The track is the fact people misread; give it a shape, not just a word. */
	.track {
		font-size: 11px;
		text-transform: uppercase;
		letter-spacing: 0.06em;
		padding: 1px 6px;
		border-radius: 999px;
		border: 1px solid var(--color-border);
		color: var(--color-foreground-muted);
	}

	.counter {
		font-size: 12px;
		color: var(--color-foreground-subtle);
	}

	.ahead {
		color: var(--color-foreground-muted);
	}

	.channel-picker {
		display: inline-flex;
		align-items: center;
		gap: 10px;
		flex-wrap: wrap;
	}

	.channel-picker select {
		font: inherit;
		font-size: 13px;
		padding: 4px 8px;
		border-radius: 6px;
		border: 1px solid var(--color-border);
		background: var(--color-background);
		color: var(--color-foreground);
	}

	.risk {
		display: inline-flex;
		align-items: center;
		gap: 4px;
		font-size: 12px;
		color: #d9a441;
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

	/* The one affirmative action in this section, so it carries the accent
	   rather than sitting as another outlined button beside "Check again". */
	.install-btn {
		display: inline-flex;
		align-items: center;
		justify-content: center;
		gap: 6px;
		align-self: flex-start;
		margin-top: 2px;
		padding: 6px 12px;
		border: 1px solid transparent;
		border-radius: 6px;
		background: var(--color-primary);
		color: var(--color-background);
		cursor: pointer;
		font-size: 13px;
		font-weight: 500;
	}

	.install-btn:hover:not(:disabled) {
		background: var(--primary-hover, var(--color-primary));
	}

	.install-btn:disabled {
		opacity: 0.55;
		cursor: default;
	}

	.apply-error {
		color: var(--error);
	}

	.restarting {
		flex-direction: column;
		align-items: flex-start;
		gap: 4px;
		padding: 12px;
		border: 1px solid var(--color-border);
		border-radius: 8px;
		background: var(--color-surface-elevated);
	}

	.restarting p {
		display: flex;
		align-items: center;
		gap: 6px;
		margin: 0;
	}

	.restarting.failed {
		color: var(--error);
		border-color: color-mix(in srgb, var(--error) 40%, transparent);
	}

	.restarting :global(.spin) {
		animation: spin 1s linear infinite;
	}

	@keyframes spin {
		to {
			transform: rotate(360deg);
		}
	}

	@media (prefers-reduced-motion: reduce) {
		.restarting :global(.spin) {
			animation: none;
		}
	}
</style>
