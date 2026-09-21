<!--
  Settings → You → "Why this exists": run the introduction again.

  The letter has been re-readable from this page for a while (the link beside
  this). What had no door at all was replaying the WALK — putting the letter
  back in its gate position and reopening the steps that were set aside. It
  was reachable only by hand:

      POST /api/setup/skip-onboarding {"skipped": false}

  which is what we were doing one beta tester at a time.

  IT DELETES NOTHING, and the copy says so plainly, because every other
  "start over" in this app is destructive by comparison. Getting-started's
  steps are DERIVED from rows on every read, so a step that is genuinely done
  — a name and a birth date on file, a source flowing, a document written —
  comes straight back as done. The only stored fact is the skip list, and this
  clears it. Someone who set integrations aside in week one and now wants them
  gets the step back; nobody loses a thing.

  NAMED AWAY FROM "START OVER" DELIBERATELY. Settings → Devices already has a
  "Start over", and it means revoke every paired device. Two buttons with one
  name, one of which signs you out of your own box, is the kind of collision
  that only shows up in a support thread.
-->
<script lang="ts">
	import { goto } from '$app/navigation';
	import Icon from '$lib/components/Icon.svelte';
	import {
		getGettingStarted,
		skipGettingStartedStep,
		skipOnboarding,
	} from '$lib/api/client';

	let armed = $state(false);
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
			// `onboarding` is what the app-shell guard reads; the letter's own
			// exit sets it back to `active`, so this cannot strand anyone.
			await skipOnboarding(false);
			await goto('/founders-letter');
		} catch (e) {
			error = e instanceof Error ? e.message : String(e);
			busy = false;
			armed = false;
		}
	}
</script>

<div class="field">
	{#if !armed}
		<button type="button" class="again" onclick={() => (armed = true)}>
			Run the introduction again
			<Icon icon="ri:arrow-right-line" width="14" />
		</button>
		<span class="field-hint">
			Opens the letter, and reopens any step you set aside. This deletes nothing. Your
			record, your sources and your subscription are untouched.
		</span>
	{:else}
		<div class="confirm">
			<button type="button" class="again go" disabled={busy} onclick={run}>
				{busy ? 'Opening…' : 'Open the letter'}
			</button>
			<button type="button" class="cancel" disabled={busy} onclick={() => (armed = false)}>
				Cancel
			</button>
		</div>
		<span class="field-hint">You will land on the founder's letter, where you started.</span>
	{/if}

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

	.confirm {
		display: flex;
		align-items: center;
		gap: 12px;
	}

	.cancel {
		background: none;
		border: none;
		padding: 0;
		font-size: 13px;
		color: var(--color-foreground-muted);
		cursor: pointer;
	}

	.cancel:hover {
		color: var(--color-foreground);
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
