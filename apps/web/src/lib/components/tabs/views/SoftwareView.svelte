<!--
  Software: what each part of the app runs on this server.

  A server, a UI bundle, and a native shell each carry their own version and
  have no reason to agree, so this page lists all three. Installing updates,
  the channel and automatic installs are on the Settings page (UpdateSection),
  so the version question has one answer here and the update controls one
  place there.
-->
<script lang="ts">
	import { onMount } from 'svelte';
	import SettingsPage from '$lib/components/settings/SettingsPage.svelte';
	import { formatDate } from '$lib/utils/dateUtils';
	import { BUILD, buildLabel } from '$lib/build';
	import {
		shellIdentity,
		describeOtaCheck,
		runsOwnCopy,
		stagedBundle,
		type ShellIdentity
	} from '$lib/tauri/bridge';

	let shell = $state<ShellIdentity | null>(null);
	const ownCopy = runsOwnCopy();
	const staged = $derived(shell && ownCopy ? stagedBundle(shell) : null);
	const otaNote = $derived(shell ? describeOtaCheck(shell) : null);
	let version = $state('');
	let commit = $state('');
	let builtAt = $state('');

	onMount(async () => {
		shell = await shellIdentity();
		try {
			const r = await fetch('/health');
			if (r.ok) {
				const d = await r.json();
				version = d.version || '';
				commit = d.commit || '';
				builtAt = d.built_at || '';
			}
		} catch {
			// Silence, not a substitute. Both of these used to fall back to
			// __BUILD_COMMIT__ — this BUNDLE's commit — and print it in the row
			// labelled "Server", on the one page whose whole subject is that
			// the three artifacts are separate things allowed to differ. A box
			// that cannot be asked leaves its row empty; the dashes are honest
			// and the reader can see which artifact failed to answer.
		}
	});

	function formatBuildTime(iso: string): string {
		if (!iso) return '';
		return formatDate(iso, {
			year: 'numeric',
			month: 'long',
			day: 'numeric',
			hour: 'numeric',
			minute: '2-digit'
		});
	}
</script>

<SettingsPage
	title="Your server"
	lede="The version each part of the app runs. They can differ. Updates are in Updates and network."
	back={{ route: '/virtues/devices', label: 'Devices' }}
>
	<section class="artifacts">
		<dl class="ledger">
			<!-- The version first, the same string Updates shows, so the three
			     parts read against each other in one column. -->
			<dt>Server</dt>
			<dd class="mono">
				{version || '—'}
				<span class="dim">
					{#if builtAt}· built {formatBuildTime(builtAt)}{/if}
					{#if commit}· {commit.slice(0, 12)}{/if}
				</span>
			</dd>

			<!--
				"Interface" is the bundle this page is running. When it came over the
				air the shell knows its content hash and we show that, because two
				bundles can report the same version (every dev build says "dev") while
				being different builds. "Bundled" is a claim about the app's own copy
				(the Mac and the phone): a browser and the Windows and Linux apps load
				the copy the box serves, so there it says so.
			-->
			<dt>Interface</dt>
			<dd class="mono">
				{buildLabel(BUILD)}
				<span class="dim">
					· {shell && ownCopy
						? shell.servingBundle
							? `ota ${shell.servingBundle.slice(0, 8)}`
							: 'bundled'
						: 'from your server'}
				</span>
				{#if staged}
					<span class="staged">
						This app downloaded update {staged.slice(0, 8)} and switches to it the next time it reloads.
					</span>
				{/if}
			</dd>

			<!--
				"App" only renders inside the native shell — in a browser there is no
				third artifact to name, and an em-dash there would imply one exists.
			-->
			{#if shell}
				<dt>App</dt>
				<dd class="mono">
					{shell.appVersion}
				</dd>
			{/if}
		</dl>

		<!--
			Only speaks when there is something to say. The loud case is a shell too
			old for the bundle the box offers: everything is working correctly and
			the user still sees stale UI, which without a reason on screen reads as
			OTA being broken.
		-->
		{#if otaNote}
			<p class="ota-note">{otaNote}</p>
		{/if}
	</section>
</SettingsPage>

<style>
	.artifacts {
		margin-top: 2.5rem;
		padding-top: 1.5rem;
		border-top: 1px solid var(--color-border);
	}

	.ledger {
		display: grid;
		grid-template-columns: auto 1fr;
		gap: 8px 16px;
		align-items: baseline;
		margin: 14px 0 0;
		font-size: 13px;
	}

	dt {
		color: var(--color-foreground-subtle);
	}

	dd {
		margin: 0;
	}

	.mono {
		font-family: var(--font-mono, ui-monospace, monospace);
		font-size: 12px;
	}

	.dim {
		color: var(--color-foreground-subtle);
	}

	.staged {
		display: block;
		margin-top: 4px;
		font-family: var(--font-sans, inherit);
		color: var(--color-foreground-muted);
	}

	.ota-note {
		margin: 12px 0 0;
		font-size: 12px;
		line-height: 1.5;
		color: var(--color-foreground-muted);
		max-width: 60ch;
	}
</style>
