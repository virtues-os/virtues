<script lang="ts">
	/**
	 * Suggested questions turn the built-in applet that writes them on or off
	 * (`applets/suggested_questions`), which is also what the server checks
	 * before offering one (`api::suggestions::current`), so off means no
	 * hourly run and no question in a new chat.
	 */
	import { onMount } from 'svelte';
	import Switch from '$lib/components/settings/Switch.svelte';
	import { getApplet, patchApplet, ApiError } from '$lib/api/client';

	let { assistantName = 'Ari' }: { assistantName?: string } = $props();

	const APPLET_ID = 'applet_suggested_questions';

	let enabled = $state(false);
	let loaded = $state(false);
	let missing = $state(false);
	let saving = $state(false);
	let error = $state<string | null>(null);

	let loadError = $state(false);

	async function load() {
		loadError = false;
		try {
			enabled = (await getApplet(APPLET_ID)).enabled;
			loaded = true;
		} catch (e) {
			// A server older than suggestions has no such applet, so there is
			// nothing to switch. Any other failure is said, with Try again.
			if (e instanceof ApiError && e.status === 404) missing = true;
			else loadError = true;
		}
	}

	onMount(load);

	async function toggle() {
		saving = true;
		error = null;
		try {
			enabled = (await patchApplet(APPLET_ID, { enabled: !enabled })).enabled;
		} catch {
			error = "Your server couldn't change this setting. Try again.";
		} finally {
			saving = false;
		}
	}
</script>

{#if !missing}
	<section>
		<div class="row">
			<div>
				<h2 class="settings-label">Suggested questions</h2>
				<p class="sec-hint">
					When you open a new chat, {assistantName} offers a question about what is happening in
					your day. It writes them once an hour from your calendar, email, messages and pages.
				</p>
			</div>
			<Switch checked={enabled} label="Suggested questions" disabled={!loaded || saving} onchange={() => toggle()} />
		</div>
		{#if loadError}
			<p class="error">Your server couldn't read this setting. <button type="button" class="again" onclick={load}>Try again</button></p>
		{:else if error}
			<p class="error">{error}</p>
		{/if}
	</section>
{/if}

<style>
	/* The same hint as the other Assistant sections (RequestLimitsSettings). */
	.sec-hint {
		margin: 12px 0 0;
		max-width: 60ch;
		font-size: 12px;
		color: var(--color-foreground-subtle);
	}

	.row {
		display: flex;
		align-items: flex-start;
		justify-content: space-between;
		gap: 24px;
	}

	.error {
		margin: 8px 0 0;
		font-size: 12px;
		color: var(--color-error);
	}

	.again {
		border: none;
		background: none;
		padding: 0;
		font: inherit;
		color: var(--color-primary);
		cursor: pointer;
	}
</style>
