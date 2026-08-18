<!--
  Software — what this box runs, and how it moves forward.

  Split out of "Box" on 2026-08-17. The version story used to be told twice on
  one scroll: UpdateSection at the top said what release the box is on and
  offered to install the next one, and System's "About" chapter, five sections
  further down, printed Package / Built / Commit / Interface / App. Two answers
  to "what am I running" on one page is one answer too many, so the question
  now has a room and System keeps only what it can measure live.

  The three artifacts below are why this page is more than an Install button.
  A box, a UI bundle, and a native shell each carry their own version and have
  no reason to agree — on 2026-08-05 a phone ran visibly newer UI than the Mac
  beside it and the reason took ssh and a git log to find. With OTA moving the
  bundle independently of the app, that question gets asked more, not less.
-->
<script lang="ts">
	import { onMount } from 'svelte';
	import { Page } from '$lib';
	import { formatDate } from '$lib/utils/dateUtils';
	import UpdateSection from '$lib/components/settings/UpdateSection.svelte';
	import { BUILD, buildLabel } from '$lib/build';
	import { shellIdentity, describeOtaCheck, type ShellIdentity } from '$lib/tauri/bridge';

	// @ts-ignore — Vite compile-time constant (see vite.config.ts + app.d.ts)
	const BUILD_COMMIT: string = __BUILD_COMMIT__;

	let shell = $state<ShellIdentity | null>(null);
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
				commit = d.commit || BUILD_COMMIT;
				builtAt = d.built_at || '';
			}
		} catch {
			commit = BUILD_COMMIT;
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

<Page
	title="Software"
	description="The release this box runs, and the track it follows."
	maxWidth="wide"
>
	<UpdateSection />

	<section class="artifacts">
		<h3>Artifacts</h3>
		<p class="sub">
			Three pieces with three version numbers. They are allowed to differ —
			this is where you find out by how much.
		</p>

		<dl class="ledger">
			<!--
				The version goes FIRST, because the sentence above promises three
				version numbers and this row used to open with a build date — the
				one artifact of the three whose version was missing, on the page
				whose whole job is telling you which versions are in play. It is
				the same string "Running" shows above; repeated here so the three
				can be read against each other in one column.
			-->
			<dt>Box</dt>
			<dd class="mono">
				{version || '—'}
				<span class="dim">
					{#if builtAt}· built {formatBuildTime(builtAt)}{/if}
					{#if commit}· {commit.slice(0, 12)}{/if}
				</span>
			</dd>

			<!--
				"Interface" is this bundle. When it came over the air the shell knows
				its content hash and we show that, because two bundles can report the
				same version (every dev build says "dev") while being different builds.
			-->
			<dt>Interface</dt>
			<dd class="mono">
				{buildLabel(BUILD)}
				<!--
					"bundled" is a claim about a native shell — that this UI shipped
					inside the app rather than arriving over the air. In a plain
					browser there is no app for it to have shipped inside; the box
					served this bundle. Saying "bundled" there was a small lie that
					only became conspicuous once this page's entire subject was
					which artifact is which.
				-->
				<span class="dim">
					· {shell
						? shell.activeBundle
							? `ota ${shell.activeBundle.slice(0, 8)}`
							: 'bundled'
						: 'served by the box'}
				</span>
			</dd>

			<!--
				"App" only renders inside the native shell — in a browser there is no
				third artifact to name, and an em-dash there would imply one exists.
			-->
			{#if shell}
				<dt>App</dt>
				<dd class="mono">
					{shell.appVersion}
					<span class="dim"> · surface {shell.commandSurface}</span>
				</dd>
			{/if}
		</dl>

		<!--
			Only speaks when there is something to say. The loud case is a shell too
			old for the bundle the box offers: everything is working correctly and
			the user still sees stale UI, which without a reason on screen reads as
			OTA being broken.
		-->
		{#if shell && describeOtaCheck(shell.lastCheck)}
			<p class="ota-note">{describeOtaCheck(shell.lastCheck)}</p>
		{/if}
	</section>
</Page>

<style>
	.artifacts {
		margin-top: 2.5rem;
		padding-top: 1.5rem;
		border-top: 1px solid var(--color-border);
	}

	h3 {
		font-size: 14px;
		font-weight: 600;
		margin: 0;
	}

	.sub {
		margin: 6px 0 0;
		font-size: 13px;
		line-height: 1.5;
		color: var(--color-foreground-muted);
		max-width: 60ch;
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

	.ota-note {
		margin: 12px 0 0;
		font-size: 12px;
		line-height: 1.5;
		color: var(--color-foreground-muted);
		max-width: 60ch;
	}
</style>
