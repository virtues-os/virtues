<!--
  Settings · Profile, at the foot: run the introduction again. It walks
  through setup (your names, your connected accounts, your subscription), and
  Profile is where your names live.

  It deletes nothing, so it runs in one click. Getting started's steps are
  derived from rows on every read, so a step that is done comes back as done;
  the only stored fact is the skip list, and this clears it, so a step set
  aside comes back.
-->
<script lang="ts">
	import { goto } from '$app/navigation';
	import Button from '$lib/components/Button.svelte';
	import {
		getGettingStarted,
		skipGettingStartedStep,
		skipOnboarding,
	} from '$lib/api/client';

	let busy = $state(false);
	let error = $state<string | null>(null);

	async function run() {
		if (busy) return;
		busy = true;
		error = null;
		try {
			// The steps first, so a failure here leaves the gate alone rather
			// than dropping someone into a letter whose walk did not reopen.
			// Absent is fine: an older box has no getting-started endpoint, and
			// the letter is still worth replaying on one.
			try {
				const gs = await getGettingStarted();
				for (const step of gs.steps) {
					if (step.status === 'skipped') await skipGettingStartedStep(step.id, false);
				}
			} catch {
				/* no getting-started on this box — the letter still replays */
			}
			// `onboarding` is what the app-shell guard reads; Setup's close
			// sets it back to `active`, so this cannot strand anyone. From the
			// first step: every step is done or reopened, so all are reachable.
			// From Welcome, so the walk replays with the letter in it; it went
			// to the Server receipt, past both.
			await skipOnboarding(false);
			await goto('/onboarding/welcome');
		} catch (e) {
			error = e instanceof Error ? e.message : String(e);
			busy = false;
		}
	}
</script>

<section>
	<h2 class="settings-label">The introduction</h2>
	<p class="sub">
		Walks you through setup again: your names, your connected accounts and your subscription. Any step you set aside comes
		back, and nothing is deleted.
	</p>
	{#if error}
		<p class="err" role="alert">The introduction didn't open. {error}</p>
	{/if}
	<Button variant="secondary" loading={busy} onclick={run}>Run the introduction again</Button>
</section>

<style>
	section {
		display: flex;
		flex-direction: column;
		align-items: flex-start;
		gap: 12px;
	}

	/* The page's heading style, set on .settings-label by SettingsPage. */
	h2 {
		margin-bottom: 0;
	}

	.sub {
		margin: 0;
		max-width: 60ch;
		font-size: 14px;
		line-height: 1.5;
		color: var(--color-foreground-muted);
	}

	.err {
		margin: 0;
		font-size: 13px;
		color: var(--color-error);
	}
</style>
