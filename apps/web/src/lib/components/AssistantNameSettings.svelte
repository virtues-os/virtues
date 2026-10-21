<script lang="ts">
	// By path, not the `$lib` barrel, which the component tests cannot load.
	import Button from '$lib/components/Button.svelte';
	import Input from '$lib/components/Input.svelte';
	import { updateAssistantProfile } from '$lib/api/client';

	/** The server's own fallback (`assistant_profile::get_assistant_name`). */
	const DEFAULT_NAME = 'Ari';
	/** The name goes into every prompt ("You are {name}"), and the server sets no limit. */
	const MAX_CHARS = 100;

	let {
		name = $bindable(DEFAULT_NAME),
		disabled = false,
	}: {
		/** The saved name. Changes only once the server has it. */
		name?: string;
		disabled?: boolean;
	} = $props();

	// Empty until the server has answered: the default shown as a value would
	// claim a name the server may not hold.
	let draft = $derived(disabled ? '' : name);
	let error = $state<string | null>(null);
	// The field is held while a save is out, so nothing typed meanwhile is
	// replaced when the saved name comes back.
	let saving = $state(false);

	function normalize(value: string | number) {
		// An empty name is not "no name": the server would store '' and every
		// prompt would read "You are .". Clearing the field goes back to the
		// default by name, since the profile PATCH treats null as "leave alone".
		return String(value).trim().slice(0, MAX_CHARS) || DEFAULT_NAME;
	}

	/** Saves what the field holds. A failure leaves the person's words in the
	 *  field for Try again, and rejects so the Input marks itself unsaved. */
	async function commit(value: string | number) {
		const next = normalize(value);
		draft = next;
		if (next === name) {
			error = null;
			return;
		}

		const previous = name;
		saving = true;
		try {
			await updateAssistantProfile({ assistant_name: next });
			name = next;
			error = null;
		} catch (e) {
			error = `Your server couldn't save the name. It kept ${previous}.`;
			throw e;
		} finally {
			saving = false;
		}
	}

	function retry() {
		// The error line already says what failed.
		commit(draft).catch(() => {});
	}

	// `Input` has no `maxlength`, so the cap is held here.
	function clamp() {
		if (draft.length > MAX_CHARS) draft = draft.slice(0, MAX_CHARS);
	}

	function onkeydown(e: KeyboardEvent) {
		const field = e.target as HTMLInputElement;
		if (e.key === 'Enter') {
			e.preventDefault();
			field.blur();
		} else if (e.key === 'Escape') {
			draft = name;
			error = null;
			field.blur();
		}
	}
</script>

<section>
	<h2 class="settings-label"><label for="assistant-name">Name</label></h2>
	<!-- The field is name-sized; the error line under it is not. -->
	<div class="name-field">
		<Input
			id="assistant-name"
			bind:value={draft}
			placeholder={DEFAULT_NAME}
			autocomplete="off"
			autoSave
			onSave={commit}
			oninput={clamp}
			{onkeydown}
			disabled={disabled || saving}
			error={!!error}
		/>
	</div>
	{#if error}
		<p class="field-error" role="alert">{error}</p>
		<div>
			<Button variant="secondary" size="sm" loading={saving} onclick={retry}>
				Try again
			</Button>
		</div>
	{/if}
</section>

<style>
	section {
		display: flex;
		flex-direction: column;
		gap: 12px;
	}

	.name-field {
		width: 20ch;
		max-width: 100%;
	}

	.field-error {
		margin: 0;
		max-width: 60ch;
		font-size: 12px;
		color: var(--color-error);
	}
</style>
