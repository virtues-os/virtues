<script lang="ts">
	import { DEFAULT_ASSISTANT_NAME } from '$lib/settings';
	/**
	 * Suggested questions turn the built-in applet that writes them on or off
	 * (`applets/suggested_questions`), which is also what the server checks
	 * before offering one (`api::suggestions::current`), so off means no
	 * hourly run and no question in a new chat.
	 */
	import { onMount } from 'svelte';
	import Switch from '$lib/components/settings/Switch.svelte';
	import LoadError from '$lib/components/settings/LoadError.svelte';
	import { getApplet, patchApplet, ApiError } from '$lib/api/client';

	let { assistantName = DEFAULT_ASSISTANT_NAME }: { assistantName?: string } = $props();

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
		<h2 class="settings-label">Speaking up</h2>
		<div class="row">
			<div>
				<p class="name">Suggested questions</p>
				<p class="sec-hint">
					When you open a new chat, {assistantName} offers a question about what is happening in
					your day. It writes them once an hour from your calendar, email, messages and pages.
				</p>
			</div>
			<Switch checked={enabled} label="Suggested questions" disabled={!loaded || saving} onchange={() => toggle()} />
		</div>
		{#if loadError}
			<LoadError message="Your server couldn't read this setting." onretry={load} />
		{:else if error}
			<p class="error">{error}</p>
		{/if}
	</section>
{/if}

<style>
	.name {
		margin: 0;
		font-size: 15px;
		color: var(--color-foreground);
	}

	.sec-hint {
		margin: 4px 0 0;
		max-width: 60ch;
		font-size: 13px;
		line-height: 1.45;
		color: var(--color-foreground-muted);
	}

	.row {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 24px;
		min-height: 56px;
	}

	.error {
		margin: 8px 0 0;
		font-size: 12px;
		color: var(--color-error);
	}
</style>
