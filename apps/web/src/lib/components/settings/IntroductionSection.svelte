<!--
  Settings, at the foot of the Updates and network page: run the
  introduction again.

  It deletes nothing, so it runs in one click. Getting started's steps are
  derived from rows on every read, so a step that is done comes back as done;
  the only stored fact is the skip list, and this clears it, so a step set
  aside comes back.

  Its name stays apart from "Start over" just above it, which unpairs every
  device.
-->
<script lang="ts">
	import { goto } from '$app/navigation';
	import Icon from '$lib/components/Icon.svelte';
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

<div class="field">
	<button type="button" class="again" disabled={busy} onclick={run}>
		{busy ? 'Opening…' : 'Run the introduction again'}
		<Icon icon="mynaui:arrow-right" width="14" />
	</button>
	<span class="field-hint">Opens the introduction, with any step you set aside. Nothing is deleted.</span>

	{#if error}
		<span class="err">{error}</span>
	{/if}
</div>

<style>
	.field {
		display: flex;
		flex-direction: column;
		gap: 6px;
	}

	/* The serif of the letter link above it — the two are one subject, and a
	   sans button beside a serif link reads as two unrelated controls. */
	.again {
		display: inline-flex;
		align-items: center;
		gap: 0.35rem;
		width: fit-content;
		padding: 0;
		background: none;
		border: none;
		font-family: var(--font-serif-ui);
		font-size: 15px;
		color: var(--color-foreground);
		cursor: pointer;
	}

	.again:hover {
		color: var(--color-primary);
	}

	.again:disabled {
		color: var(--color-foreground-muted);
		cursor: default;
	}

	.field-hint {
		font-size: 12px;
		color: var(--color-foreground-subtle);
	}

	.err {
		font-size: 12px;
		color: var(--color-error);
	}
</style>
