<script lang="ts">
	/**
	 * Settings · Profile: the basics the assistant and your pages work from,
	 * after a link to your story, the account you write in your own words.
	 *
	 * Every field is a plain box that saves on its own when you leave it
	 * (`Input autoSave`), through the profile endpoint. Location is a city;
	 * choosing one from the list also sets the time zone your days are counted in.
	 */
	import { onMount } from 'svelte';
	import { invalidate } from '$app/navigation';
	import Input from '$lib/components/Input.svelte';
	import LoadError from './LoadError.svelte';
	import Segmented from '$lib/components/Segmented.svelte';
	import SettingRow from '$lib/components/settings/SettingRow.svelte';
	import LinkRow from '$lib/components/settings/LinkRow.svelte';
	import LocationField from '$lib/components/settings/LocationField.svelte';
	import { zoneName } from '$lib/components/setup/cities';
	import { units } from '$lib/stores/units.svelte';
	import type { Units } from '$lib/utils/units';
	import { getProfile, updateProfile, type Profile } from '$lib/api/client';
	import { findSetting, titleOf } from '$lib/settings';

	let { assistantName }: { assistantName: string } = $props();

	type Field = 'full_name' | 'preferred_name' | 'birth_date' | 'occupation' | 'employer';

	let values = $state<Record<Field, string>>({
		full_name: '',
		preferred_name: '',
		birth_date: '',
		occupation: '',
		employer: '',
	});
	let home = $state({ city: '', zone: '' });
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
			};
			home = { city: p.home_city ?? '', zone: p.home_timezone ?? '' };
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

	async function saveHome(change: { home_city: string; home_timezone?: string }) {
		await updateProfile(change);
		home = { city: change.home_city, zone: change.home_timezone ?? home.zone };
		invalidate('/api/profile');
	}

	const label = (id: string) => titleOf(findSetting(id)?.title ?? id, assistantName);

	const homeHelp = $derived(
		home.zone.includes('/')
			? `The city you're usually in. Your days are counted in its time zone, ${zoneName(home.zone)}.`
			: "The city you're usually in. Your days are counted in its time zone.",
	);

	const UNITS = [
		{ value: 'us', label: 'US' },
		{ value: 'metric', label: 'Metric' },
	] as const satisfies readonly { value: Units; label: string }[];
	let unitsError = $state<string | null>(null);

	async function chooseUnits(next: Units) {
		const was = units.current;
		units.set(next);
		unitsError = null;
		try {
			await updateProfile({ units: next });
		} catch {
			units.set(was);
			unitsError = "Your server couldn't save your units. Try again.";
		}
	}
</script>

<section class="group">
	<h2 class="settings-label">Your story</h2>
	<LinkRow
		id="story"
		label="What you've written about your life"
		help="In your words, the account {assistantName} reads before anything else."
		href="/wiki/identity"
	/>
</section>

<section class="group">
	<h2 class="settings-label">You</h2>
	{#if phase === 'error'}
		<!-- Empty fields here would be read as "nothing on file", and saving from
		     them would overwrite what the server holds. -->
		<LoadError message="Your server couldn't load your profile." onretry={load} />
	{:else}
		<SettingRow id="full-name" label={label('full-name')} forId="profile-full-name">
			<Input id="profile-full-name" bind:value={values.full_name} autocomplete="name" disabled={phase !== 'ready'} autoSave onSave={(v) => save('full_name', v)} />
		</SettingRow>
		<SettingRow id="preferred-name" label={label('preferred-name')} forId="profile-preferred-name">
			<Input id="profile-preferred-name" bind:value={values.preferred_name} autocomplete="nickname" disabled={phase !== 'ready'} autoSave onSave={(v) => save('preferred_name', v)} />
		</SettingRow>
		<SettingRow id="birthday" label={label('birthday')} forId="profile-birthday">
			<div class="date">
				<Input id="profile-birthday" type="date" bind:value={values.birth_date} disabled={phase !== 'ready'} autoSave onSave={(v) => save('birth_date', v)} />
			</div>
		</SettingRow>
		<SettingRow id="occupation" label={label('occupation')} help="Leave the second box blank if you work for yourself." forId="profile-occupation">
			<div class="pair">
				<Input id="profile-occupation" placeholder="What you do" bind:value={values.occupation} autocomplete="organization-title" disabled={phase !== 'ready'} autoSave onSave={(v) => save('occupation', v)} />
				<span id="setting-employer" data-setting="employer" class="marker"></span>
				<Input id="profile-employer" placeholder="Where you work" bind:value={values.employer} autocomplete="organization" disabled={phase !== 'ready'} autoSave onSave={(v) => save('employer', v)} />
			</div>
		</SettingRow>
		<SettingRow id="home" label={label('home')} help={homeHelp} forId="profile-home">
			<LocationField city={home.city} zone={home.zone} disabled={phase !== 'ready'} onsave={saveHome} />
		</SettingRow>
		<SettingRow id="units" label={label('units')} help={unitsError ?? `For distances, weights and temperatures, here and in what ${assistantName} writes.`}>
			<Segmented id="profile-units" label={label('units')} value={units.current} options={UNITS} disabled={phase !== 'ready'} onchange={chooseUnits} />
		</SettingRow>
	{/if}
</section>

<style>
	.group {
		padding: 24px 0 8px;
	}

	/* A field fills the row's answer column. */
	.group :global(.input-container) {
		width: 100%;
	}

	/* Work: what you do and where, side by side. */
	.pair {
		display: grid;
		grid-template-columns: minmax(0, 1fr) minmax(0, 1fr);
		gap: 8px;
		width: 100%;
	}

	.marker {
		display: none;
	}

	.date {
		width: 180px;
	}

	@media (max-width: 560px) {
		.pair {
			grid-template-columns: minmax(0, 1fr);
		}
	}
</style>
