<script lang="ts">
	/**
	 * Settings · Profile: the basics the assistant and your pages work from. Each
	 * field saves on its own when you leave it (`Input autoSave`), through the
	 * profile endpoint.
	 */
	import { onMount } from 'svelte';
	import { invalidate } from '$app/navigation';
	import Input from '$lib/components/Input.svelte';
	import Button from '$lib/components/Button.svelte';
	import SettingRow from '$lib/components/settings/SettingRow.svelte';
	import { getProfile, updateProfile, type Profile } from '$lib/api/client';
	import { findSetting, titleOf } from '$lib/settings';

	let { assistantName }: { assistantName: string } = $props();

	type Field = 'full_name' | 'preferred_name' | 'birth_date' | 'occupation' | 'employer' | 'home_timezone';

	let values = $state<Record<Field, string>>({
		full_name: '',
		preferred_name: '',
		birth_date: '',
		occupation: '',
		employer: '',
		home_timezone: '',
	});
	let phase = $state<'loading' | 'ready' | 'error'>('loading');

	async function load() {
		phase = 'loading';
		try {
			const p = await getProfile();
			values = {
				full_name: p.full_name ?? '',
				preferred_name: p.preferred_name ?? '',
				birth_date: p.birth_date ? p.birth_date.split('T')[0] : '',
				occupation: p.occupation ?? '',
				employer: p.employer ?? '',
				home_timezone: p.home_timezone ?? '',
			};
			phase = 'ready';
		} catch {
			phase = 'error';
		}
	}

	onMount(load);

	// Clearing a field saves it empty: the server stores an empty text field
	// as nothing, and takes `null` to clear the birth date.
	async function save(field: Field, value: string | number) {
		const text = String(value ?? '').trim();
		const stored = field === 'birth_date' ? text || null : text;
		await updateProfile({ [field]: stored } as Partial<Profile>);
		invalidate('/api/profile');
	}

	const label = (id: string) => titleOf(findSetting(id)?.title ?? id, assistantName);
</script>

{#if phase === 'error'}
	<!-- Empty fields here would be read as "nothing on file", and saving from
	     them would overwrite what the server holds. -->
	<div class="load-error" role="alert">
		<p>Your server couldn't load your profile.</p>
		<Button variant="secondary" size="sm" onclick={load}>Try again</Button>
	</div>
{:else}
	<SettingRow id="full-name" label={label('full-name')} forId="profile-full-name">
		<div class="field">
			<Input id="profile-full-name" placeholder="Add" bind:value={values.full_name} autocomplete="name" disabled={phase !== 'ready'} autoSave onSave={(v) => save('full_name', v)} />
		</div>
	</SettingRow>
	<SettingRow id="preferred-name" label={label('preferred-name')} forId="profile-preferred-name">
		<div class="field">
			<Input id="profile-preferred-name" placeholder="Add" bind:value={values.preferred_name} autocomplete="nickname" disabled={phase !== 'ready'} autoSave onSave={(v) => save('preferred_name', v)} />
		</div>
	</SettingRow>
	<SettingRow id="birthday" label={label('birthday')} forId="profile-birthday">
		<div class="field">
			<Input id="profile-birthday" type="date" bind:value={values.birth_date} disabled={phase !== 'ready'} autoSave onSave={(v) => save('birth_date', v)} />
		</div>
	</SettingRow>
	<SettingRow id="occupation" label={label('occupation')} forId="profile-occupation">
		<div class="field">
			<Input id="profile-occupation" placeholder="Add" bind:value={values.occupation} autocomplete="organization-title" disabled={phase !== 'ready'} autoSave onSave={(v) => save('occupation', v)} />
		</div>
	</SettingRow>
	<SettingRow id="employer" label={label('employer')} help="Leave it blank if you work for yourself." forId="profile-employer">
		<div class="field">
			<Input id="profile-employer" placeholder="Add" bind:value={values.employer} autocomplete="organization" disabled={phase !== 'ready'} autoSave onSave={(v) => save('employer', v)} />
		</div>
	</SettingRow>
	<SettingRow id="home" label={label('home')} help="Your time zone, like Europe/Lisbon. Your days are counted in it." forId="profile-home">
		<div class="field">
			<Input id="profile-home" bind:value={values.home_timezone} placeholder="Europe/Lisbon" disabled={phase !== 'ready'} autoSave onSave={(v) => save('home_timezone', v)} />
		</div>
	</SettingRow>
{/if}

<style>
	.field {
		width: 280px;
	}

	/* Quiet fields: the value reads as text at the row's end, and the box
	   shows on hover and while you edit. */
	.field :global(.input-wrapper:not(.hovered):not(.focused):not(.error) .input-field) {
		border-color: transparent;
		background: transparent;
		text-align: right;
	}

	.field :global(.input-wrapper.disabled .input-field) {
		background: transparent;
	}

	/* "Add" sits where a value would, at the end of the row. */
	.field :global(.input-wrapper:not(.focused) .placeholder) {
		left: auto;
		right: 36px;
	}

	.load-error {
		display: flex;
		flex-direction: column;
		align-items: flex-start;
		gap: 8px;
		padding: 12px 0;
	}

	.load-error p {
		margin: 0;
		font-size: 13px;
		color: var(--color-error);
	}
</style>
