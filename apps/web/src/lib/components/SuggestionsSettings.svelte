<script lang="ts">
	/**
	 * What an empty new chat shows above the composer.
	 *
	 * The chat sky is `ui_preferences.chat_sky`, on unless it is `false`;
	 * ChatSky reads the same key.
	 *
	 * Suggested questions turn the built-in applet that writes them on or off
	 * (`applets/suggested_questions`), which is also what the server checks
	 * before offering one (`api::suggestions::current`), so off means no
	 * hourly run and no question in a new chat.
	 */
	import { onMount } from 'svelte';
	import { getApplet, getAssistantProfile, patchApplet, updateUiPreferences } from '$lib/api/client';

	let { assistantName = 'Ari' }: { assistantName?: string } = $props();

	const APPLET_ID = 'applet_suggested_questions';

	let enabled = $state(false);
	let loaded = $state(false);
	let missing = $state(false);
	let saving = $state(false);
	let error = $state<string | null>(null);

	let sky = $state(true);
	let skyLoaded = $state(false);
	let skySaving = $state(false);
	let skyError = $state<string | null>(null);

	onMount(() => {
		getAssistantProfile<{ ui_preferences?: Record<string, unknown> }>()
			.then((p) => {
				sky = p?.ui_preferences?.chat_sky !== false;
				skyLoaded = true;
			})
			.catch(() => (skyError = "Your server couldn't read this setting. Reload to try again."));
	});

	async function toggleSky() {
		skySaving = true;
		skyError = null;
		try {
			await updateUiPreferences({ chat_sky: !sky });
			sky = !sky;
		} catch {
			skyError = "Your server couldn't change this setting. Try again.";
		} finally {
			skySaving = false;
		}
	}

	onMount(async () => {
		try {
			enabled = (await getApplet(APPLET_ID)).enabled;
			loaded = true;
		} catch {
			// A server older than suggestions has no such applet, so there is
			// nothing to switch.
			missing = true;
		}
	});

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

<section>
	<div class="row">
		<div>
			<h2 class="settings-label">Chat sky</h2>
			<p class="sec-hint">
				When you open a new chat, a dot for each part of your day sits above the message box: places,
				calendar, workouts and last night. Hover one to see what it was.
			</p>
		</div>
		<button
			type="button"
			class="switch"
			class:on={sky}
			role="switch"
			aria-checked={sky}
			aria-label="Chat sky"
			disabled={!skyLoaded || skySaving}
			onclick={toggleSky}
		></button>
	</div>
	{#if skyError}
		<p class="error">{skyError}</p>
	{/if}
</section>

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
			<button
				type="button"
				class="switch"
				class:on={enabled}
				role="switch"
				aria-checked={enabled}
				aria-label="Suggested questions"
				disabled={!loaded || saving}
				onclick={toggle}
			></button>
		</div>
		{#if error}
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

	.switch {
		width: 44px;
		height: 26px;
		border-radius: var(--radius-full);
		background: var(--color-border);
		position: relative;
		flex: none;
		border: 0;
		cursor: pointer;
		transition: background 150ms ease;
		padding: 0;
	}

	.switch.on {
		background: var(--color-success);
	}

	.switch::after {
		content: '';
		position: absolute;
		top: 3px;
		left: 3px;
		width: 20px;
		height: 20px;
		border-radius: 50%;
		background: var(--color-background);
		transition: transform 150ms ease;
	}

	.switch.on::after {
		transform: translateX(18px);
	}

	.switch:focus-visible {
		outline: 2px solid var(--color-foreground);
		outline-offset: 2px;
	}

	.switch:disabled {
		opacity: 0.5;
		cursor: default;
	}

	@media (prefers-reduced-motion: reduce) {
		.switch,
		.switch::after {
			transition: none;
		}
	}

	.error {
		margin: 8px 0 0;
		font-size: 12px;
		color: var(--color-error);
	}
</style>
