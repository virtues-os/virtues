<script lang="ts">
	/**
	 * Settings · Your server · Updates (/virtues/updates): whether your server
	 * is current, installing the next release, and which releases it takes
	 * (`GET /api/system/update`, `api/updates.rs`).
	 *
	 * The page leads with one sentence for the state the server reports: an
	 * install under way, a check that failed, an install that rolled back, an
	 * early build ahead of the newest release, a release ready, or up to date.
	 *
	 * It checks on open rather than on a timer. A background poll would mean
	 * the server calling out to GitHub on its own initiative, which a machine
	 * holding someone's whole life should not do unprompted.
	 *
	 * Installing restarts the server: the binary is swapped and migrations run,
	 * and every connected device drops, not only this one. The request cannot
	 * report a result, because the process serving it is the one being
	 * replaced, so the page watches the server go away and come back, and
	 * success is the build stamp changing (`watchForServer`).
	 *
	 * "What each part runs" lists the server, the screens and the app, which
	 * each carry their own version and can differ.
	 */
	import { onMount, onDestroy } from 'svelte';
	import SettingsPage from './SettingsPage.svelte';
	import SettingRow from './SettingRow.svelte';
	import Switch from './Switch.svelte';
	import Command from './Command.svelte';
	import Fold from './Fold.svelte';
	import Button from '$lib/components/Button.svelte';
	import { getUpdateStatus, setUpdateChannel, applyUpdate, type UpdateStatus } from '$lib/api/client';
	import { autoUpdateOf, setAutoUpdate, type AutoUpdateStatus } from '$lib/api/autoUpdate';
	import { confirmAction } from '$lib/stores/dialog.svelte';
	import { reloadApp } from '$lib/reload';
	import { lightPendingSetting } from '$lib/settings/focus';
	import { BUILD, buildLabel } from '$lib/build';
	import { formatDate } from '$lib/utils/dateUtils';
	import { shellIdentity, describeOtaCheck, runsOwnCopy, stagedBundle, type ShellIdentity } from '$lib/tauri/bridge';

	const GUIDE = 'https://virtues.com/docs/operate/upgrading';
	const RELEASED = 'Releases that have been tested.';
	const EARLY = 'Builds before they’re released. They can break.';

	let status = $state<UpdateStatus | null>(null);
	let loading = $state(true);
	let loadFailed = $state(false);
	let auto = $state<AutoUpdateStatus | null>(null);
	let autoSaving = $state(false);
	let autoError = $state<string | null>(null);
	let switching = $state(false);
	let channelError = $state<string | null>(null);
	/** Bumped when a choice doesn't hold (cancelled, or refused), so the
	 *  radios redraw on the channel the server still has. */
	let channelKey = $state(0);
	let applyError = $state<string | null>(null);

	/** null when idle. Otherwise the server is being replaced under this page. */
	let restart = $state<{ phase: 'going' | 'back'; error?: string; detail?: string } | null>(null);
	let watchTimer: ReturnType<typeof setTimeout> | null = null;
	/** Set when the page goes away, so a poll already waiting on the server
	 *  does not schedule another (or reload whatever page you are on). */
	let destroyed = false;

	// What each part runs.
	let shell = $state<ShellIdentity | null>(null);
	let serverBuild = $state<{ version: string; commit: string; builtAt: string } | null>(null);
	const ownCopy = runsOwnCopy();

	async function check() {
		loading = true;
		try {
			status = await getUpdateStatus();
			// An older server doesn't report it; the row just doesn't show.
			auto = autoUpdateOf(status);
			loadFailed = false;
		} catch {
			loadFailed = true;
		} finally {
			loading = false;
		}
	}

	async function loadParts() {
		shell = await shellIdentity();
		try {
			const res = await fetch('/health', { cache: 'no-store' });
			if (res.ok) {
				const d = await res.json();
				serverBuild = { version: d.version || '', commit: d.commit || '', builtAt: d.built_at || '' };
			}
		} catch {
			// The row says "Not known". Never this bundle's own commit in its
			// place: the parts are separate things allowed to differ.
		}
	}

	onMount(() => {
		void check();
		void loadParts();
		return lightPendingSetting();
	});

	onDestroy(() => {
		destroyed = true;
		if (watchTimer) clearTimeout(watchTimer);
	});

	/** `v0.1.10-3f2a9c1` -> `v0.1.10`: the slot id carries the commit for disk, not for people. */
	const releaseName = (slot: string) => slot.replace(/-[0-9a-f]{7,}$/, '');

	const day = (iso: string) => new Date(iso).toLocaleDateString('en-US', { month: 'long', day: 'numeric' });
	const hourLabel = (h: number) => new Date(2000, 0, 1, h).toLocaleTimeString('en-US', { hour: 'numeric' });

	/** The box fetches stable releases on a schedule, so by the time anyone
	 *  opens this page the update is usually already downloaded. */
	const staged = $derived(status?.staged ?? null);
	const early = $derived(status?.channel === 'prerelease');
	/** The newest automatic install, when it rolled back and the release it
	 *  tried is still the one on offer. */
	const failedInstall = $derived(
		auto?.install && !auto.install.ok && status?.update_available ? auto.install : null,
	);

	const lead = $derived.by((): { sentence: string; sub: string } | null => {
		if (restart) {
			if (restart.error) return { sentence: restart.error, sub: '' };
			if (restart.phase === 'back') return { sentence: 'Your server is back.', sub: 'Reloading this page.' };
			return {
				sentence: 'Installing the update.',
				sub: 'Your server is restarting and will be back in a minute or two. Other devices drop too, and reconnect on their own.',
			};
		}
		if (loadFailed) {
			return { sentence: "Your server couldn't check for updates.", sub: 'It didn’t answer. Try again in a moment.' };
		}
		if (!status) return null;
		if (status.check_error) {
			return {
				sentence: "Your server couldn't check for updates.",
				sub: 'The check stopped with the error below. Try again in a few minutes.',
			};
		}
		const running = status.running_version;
		const latest = status.latest ?? 'The update';
		if (failedInstall) {
			return {
				sentence: `Your server couldn’t install ${releaseName(failedInstall.to)}.`,
				sub: `It tried on ${day(failedInstall.at)} and went back to ${failedInstall.from}, so nothing changed. It tries again when a newer release comes out, or you can install it now.`,
			};
		}
		if (status.running_ahead) {
			return {
				sentence: 'Your server runs an early build.',
				sub: `It runs ${running}, which is ahead of the newest release${status.latest ? `, ${status.latest}` : ''}. There is nothing to install until a release catches up.`,
			};
		}
		if (status.update_available) {
			return staged
				? {
						sentence: `${latest} is ready to install.`,
						sub: `Your server runs ${running} and has already downloaded ${latest}. Installing restarts your server, which takes under a minute. Every device connected to it drops and reconnects on its own, and you lose nothing.`,
					}
				: {
						sentence: `${latest} is available.`,
						sub: `Your server runs ${running}. Installing downloads it and restarts your server, which takes a minute or two. Every device connected to it drops and reconnects on its own, and you lose nothing.`,
					};
		}
		return {
			sentence: 'Your server is up to date.',
			sub: `It runs ${running}, the newest ${early ? 'early build' : 'release'}.`,
		};
	});

	const canInstall = $derived(!restart && !!status?.update_available && !status.check_error && !status.running_ahead);

	/** The server's own words, under the sentence that needs them. */
	const errorDetail = $derived(
		restart?.detail ?? (status?.check_error && !restart ? status.check_error : null) ?? (failedInstall && !restart ? failedInstall.error : null),
	);

	const autoHelp = $derived.by(() => {
		if (autoError) return `Your server couldn’t change this: ${autoError}. Try again.`;
		if (!auto) return '';
		let help = `Around ${hourLabel(auto.hour)} your server installs the next update and restarts. Your devices reconnect on their own.`;
		if (auto.problem) help += ` ${auto.problem}`;
		else if (auto.install?.ok) help += ` Last time was ${day(auto.install.at)}, when it installed ${releaseName(auto.install.to)}.`;
		else if (auto.install) help += ` Last time was ${day(auto.install.at)}, when it couldn’t install ${releaseName(auto.install.to)}.`;
		return help;
	});

	async function toggleAuto() {
		if (!auto || autoSaving) return;
		autoSaving = true;
		autoError = null;
		try {
			auto = await setAutoUpdate(!auto.enabled);
		} catch (err) {
			autoError = err instanceof Error ? err.message : String(err);
		} finally {
			autoSaving = false;
		}
	}

	async function chooseChannel(channel: 'stable' | 'prerelease') {
		if (switching || status?.channel === channel) return;
		// Early ships builds nobody has released to the machine holding your
		// record, so choosing it asks once. Going back to Released doesn't.
		if (channel === 'prerelease') {
			const ok = await confirmAction({
				title: 'Take early updates?',
				body: 'Early builds come before they’re released, and they can break. Switching back later keeps the version you have until a release catches up.',
				confirmLabel: 'Take early updates',
				cancelLabel: 'Keep released',
			});
			if (!ok) {
				channelKey += 1;
				return;
			}
		}
		switching = true;
		channelError = null;
		try {
			await setUpdateChannel(channel);
			await check();
		} catch {
			channelError = "Your server couldn't change which updates it takes. It's still on the one shown. Try again.";
			channelKey += 1;
		} finally {
			switching = false;
		}
	}

	async function install() {
		if (restart || !status) return;
		applyError = null;
		const latest = status.latest ?? 'this update';
		const ok = await confirmAction({
			title: `Install ${latest}?`,
			body: staged
				? 'Your server runs its migrations and restarts, which takes well under a minute. Every device connected to it drops, phones and other browsers included, not just this window. You lose nothing, and they reconnect on their own.'
				: 'Your server downloads the release, swaps its binary and runs migrations, so it stops serving for a minute or two. Every device connected to it drops, phones and other browsers included, not just this window. You lose nothing, and they reconnect on their own.',
			confirmLabel: 'Install and restart',
			cancelLabel: 'Not now',
		});
		if (!ok) return;
		try {
			// The build running now, so the watcher can tell a server that came
			// back on the new release from one that rolled back.
			const before = await probe();
			await applyUpdate();
			restart = { phase: 'going' };
			watchForServer(before?.build ?? null);
		} catch (err) {
			// The server's own words. A bare "update failed" is what sends
			// someone to SSH in to find out what happened.
			applyError = err instanceof Error ? err.message : String(err);
		}
	}

	/** `/health`, and the build the server stamps on every response. `null` when it doesn't answer. */
	async function probe(): Promise<{ ok: boolean; build: string | null } | null> {
		try {
			const res = await fetch('/health', { cache: 'no-store' });
			return { ok: res.ok, build: res.headers.get('x-virtues-box-build') };
		} catch {
			return null;
		}
	}

	/** The server's record of an install that started after `since`, if it
	 *  failed. `virtues activate` writes it whether this page or the night
	 *  started the install. */
	async function failedInstallSince(since: number) {
		try {
			const record = autoUpdateOf(await getUpdateStatus())?.install;
			if (record && !record.ok && new Date(record.at).getTime() >= since) return record;
		} catch {
			// No record to read; the caller says what it can without one.
		}
		return null;
	}

	/**
	 * Watch the server through the restart. Going down and coming back is not
	 * success on its own: an activation that fails flips the server back to
	 * the release it had and restarts that, which looks the same from here.
	 * So success is the build stamp changing. A server that comes back on the
	 * same build failed, and its record of the attempt says why. A server that
	 * never goes down is still downloading, or refused before it stopped; that
	 * is a failure only once it records one, or the wait runs out.
	 */
	function watchForServer(before: string | null) {
		const startedAt = Date.now();
		// Long enough for a download and a migration on a slow server; short
		// enough that a stuck upgrade doesn't spin forever.
		const LIMIT_MS = 10 * 60 * 1000;
		let sawItGo = false;

		const failed = (record: { to: string; error: string | null } | null) => {
			restart = {
				phase: 'going',
				error: record
					? `Your server couldn’t install ${releaseName(record.to)}, so it went back to the version it had. Nothing changed, and you can try again.`
					: "Your server didn't install the update. It's still running the version it had, and nothing changed. Try again in a few minutes.",
				detail: record?.error ?? undefined,
			};
			void check();
		};

		const poll = async () => {
			if (destroyed) return;
			const now = await probe();
			if (destroyed) return;
			const alive = !!now?.ok;
			// Older servers send no stamp; for them, coming back is all there is.
			const moved = before === null || (!!now?.build && now.build !== before);
			if (!alive) {
				sawItGo = true;
				restart = { phase: 'going' };
			} else if (moved) {
				// The served files changed underneath this page, so it reloads.
				restart = { phase: 'back' };
				setTimeout(() => reloadApp(), 1200);
				return;
			} else if (sawItGo) {
				failed(await failedInstallSince(startedAt));
				return;
			} else {
				const refused = await failedInstallSince(startedAt);
				if (refused) {
					failed(refused);
					return;
				}
			}
			if (Date.now() - startedAt > LIMIT_MS) {
				failed(null);
				return;
			}
			watchTimer = setTimeout(poll, 2000);
		};
		watchTimer = setTimeout(poll, 2000);
	}

	// ─── What each part runs ────────────────────────────────────────────────
	const stagedScreens = $derived(shell && ownCopy ? stagedBundle(shell) : null);
	const otaNote = $derived(shell ? describeOtaCheck(shell) : null);
	const partsSub = $derived(
		shell
			? 'Your server, the screens you’re looking at, and this app each have a version.'
			: 'Your server and the screens you’re looking at each have a version.',
	);

	function builtOn(iso: string): string {
		return formatDate(iso, { year: 'numeric', month: 'long', day: 'numeric', hour: 'numeric', minute: '2-digit' });
	}
</script>

<SettingsPage
	title="Updates"
	lede="New releases of your server, and which ones it takes."
	back={{ route: '/virtues/system', label: 'Your server' }}
>
	<div id="setting-updates">
		{#if lead}
			<p class="lead">{lead.sentence}</p>
			{#if lead.sub}<p class="sub">{lead.sub}</p>{/if}
		{:else if loading}
			<p class="sub">Checking for updates…</p>
		{/if}

		{#if errorDetail}
			<p class="said">Your server said:</p>
			<code class="error">{errorDetail}</code>
		{/if}

		{#if restart?.error}
			<div class="actions"><Button variant="secondary" onclick={() => (restart = null)}>Done</Button></div>
		{:else if !restart}
			<div class="actions">
				{#if canInstall}
					<Button variant="primary" onclick={install}>{staged ? 'Install and restart' : 'Download and install'}</Button>
				{/if}
				<Button variant="secondary" {loading} onclick={check}>Check again</Button>
			</div>
		{/if}

		{#if applyError}
			<p class="said">Your server said:</p>
			<code class="error">{applyError}</code>
			<p class="sub">You can also install it with this command on your server:</p>
			<div class="command"><Command command="sudo virtues upgrade" /></div>
		{/if}
	</div>

	{#if status}
		<section class="group">
			<h2 class="settings-label">How it updates</h2>
			{#if auto}
				<SettingRow id="auto-install" label="Install automatically" help={autoHelp}>
					<Switch checked={auto.enabled} label="Install updates automatically" disabled={autoSaving} onchange={() => toggleAuto()} />
				</SettingRow>
			{/if}
			<SettingRow id="update-channel" label="Which updates" help={channelError ?? undefined}>
				{#key channelKey}
				<div class="choices" role="radiogroup" aria-label="Which updates">
					<label class="choice">
						<input
							type="radio"
							name="update-channel"
							checked={!early}
							disabled={switching}
							onchange={() => chooseChannel('stable')}
						/>
						<span>
							<span class="choice-name">Released</span>
							<span class="choice-sub">
								{RELEASED}{early ? ' Switching back keeps the version you have until a release catches up.' : ''}
							</span>
						</span>
					</label>
					<label class="choice">
						<input
							type="radio"
							name="update-channel"
							checked={early}
							disabled={switching}
							onchange={() => chooseChannel('prerelease')}
						/>
						<span>
							<span class="choice-name">Early</span>
							<span class="choice-sub">{EARLY}</span>
						</span>
					</label>
				</div>
				{/key}
			</SettingRow>

			<div class="parts">
				<Fold label="What each part runs" sub={partsSub}>
					<dl class="facts">
						<dt>Server</dt>
						<dd>
							{serverBuild?.version || 'Not known'}
							{#if serverBuild?.builtAt || serverBuild?.commit}
								<span class="dim">
									{#if serverBuild.builtAt}Built {builtOn(serverBuild.builtAt)}.{/if}
									{#if serverBuild.commit}<code>{serverBuild.commit.slice(0, 12)}</code>{/if}
								</span>
							{/if}
						</dd>
						<!-- Two bundles can report the same version (every dev build
						     says "dev"), so a bundle that came over the air shows its
						     content hash. The Mac and the phone run their own copy of
						     the screens; a browser and the Windows and Linux apps load
						     the server's. -->
						<dt>Screens</dt>
						<dd>
							{buildLabel(BUILD)}
							<span class="dim">
								{shell && ownCopy
									? shell.servingBundle
										? `Updated over the air (${shell.servingBundle.slice(0, 8)}).`
										: 'The copy that came with this app.'
									: 'From your server.'}
							</span>
							{#if stagedScreens}
								<span class="dim">This app downloaded {stagedScreens.slice(0, 8)} and switches to it the next time it reloads.</span>
							{/if}
						</dd>
						{#if shell}
							<dt>App</dt>
							<dd>{shell.appVersion}</dd>
						{/if}
					</dl>
					{#if otaNote}<p class="note">{otaNote}</p>{/if}
				</Fold>
			</div>
		</section>

		<p class="aside">
			The manual’s <a href={GUIDE} target="_blank" rel="noopener noreferrer">Upgrading</a> page covers channels, pinning a
			version, and going back to an earlier one.
		</p>
	{/if}
</SettingsPage>

<style>
	.lead {
		margin: 32px 0 0;
		font-family: var(--font-serif);
		font-size: 28px;
		font-weight: 400;
		line-height: 1.25;
		color: var(--color-foreground);
	}

	.sub {
		margin: 8px 0 0;
		max-width: 60ch;
		font-size: 14px;
		line-height: 1.5;
		color: var(--color-foreground-muted);
	}

	.said {
		margin: 16px 0 0;
		font-size: 13px;
		color: var(--color-foreground-muted);
	}

	.error {
		display: block;
		margin-top: 4px;
		font-family: var(--font-mono);
		font-size: 12px;
		line-height: 1.45;
		color: var(--color-error);
		overflow-wrap: anywhere;
	}

	.actions {
		display: flex;
		flex-wrap: wrap;
		gap: 8px;
		margin-top: 16px;
	}

	.command {
		margin-top: 8px;
	}

	.group {
		padding: 24px 0 8px;
	}

	.choices {
		display: flex;
		flex-direction: column;
		gap: 4px;
	}

	.choice {
		display: grid;
		grid-template-columns: 16px minmax(0, 1fr);
		align-items: start;
		gap: 12px;
		padding: 4px 0;
		cursor: pointer;
	}

	.choice input {
		width: 16px;
		height: 16px;
		margin: 4px 0 0;
		accent-color: var(--color-primary);
		cursor: pointer;
	}

	.choice-name {
		display: block;
		font-size: 15px;
		color: var(--color-foreground);
	}

	.choice-sub {
		display: block;
		font-size: 12px;
		line-height: 1.45;
		color: var(--color-foreground-muted);
	}

	.parts {
		margin-top: 16px;
	}

	.facts {
		display: grid;
		grid-template-columns: 96px minmax(0, 1fr);
		gap: 8px 16px;
		margin: 0;
		font-size: 14px;
	}

	dt {
		color: var(--color-foreground-muted);
	}

	dd {
		margin: 0;
		color: var(--color-foreground);
		overflow-wrap: anywhere;
	}

	.dim {
		display: block;
		font-size: 12px;
		color: var(--color-foreground-muted);
	}

	code {
		font-family: var(--font-mono);
		font-size: 12px;
	}

	.note {
		margin: 12px 0 0;
		max-width: 60ch;
		font-size: 12px;
		line-height: 1.5;
		color: var(--color-foreground-muted);
	}

	.aside {
		margin: 32px 0 0;
		font-size: 13px;
		line-height: 1.5;
		color: var(--color-foreground-muted);
	}

	.aside a {
		color: var(--color-primary);
		text-decoration: none;
	}
</style>
