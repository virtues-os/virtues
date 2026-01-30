<script lang="ts">
	import type { Tab, FallbackView } from '$lib/tabs/types';
	import { spaceStore } from '$lib/stores/space.svelte';
	import { Page, Input } from '$lib';
	import ThemePicker from '$lib/components/ThemePicker.svelte';
	import Icon from '$lib/components/Icon.svelte';
	import { onMount } from 'svelte';
	import { getTheme, applyTheme, setTheme, type Theme, isValidTheme } from '$lib/utils/theme';
	import { invalidate } from '$app/navigation';

	let { tab, active }: { tab: Tab; active: boolean } = $props();

	let loading = $state(true);
	let currentTheme = $state<Theme>(getTheme());
	let fallbackView = $state<FallbackView>('empty');

	// Profile fields
	let fullName = $state('');
	let preferredName = $state('');
	let birthDate = $state('');
	let heightCm = $state('');
	let weightKg = $state('');
	let ethnicity = $state('');
	let occupation = $state('');
	let employer = $state('');

	async function handleThemeChange(newTheme: Theme) {
		currentTheme = newTheme;

		try {
			setTheme(newTheme);
			await fetch('/api/profile', {
				method: 'PUT',
				headers: { 'Content-Type': 'application/json' },
				body: JSON.stringify({ theme: newTheme })
			});
			invalidate('/api/profile');
		} catch (error) {
			console.error('Failed to save theme preference:', error);
		}
	}

	onMount(async () => {
		fallbackView = spaceStore.fallbackPreference;
		await loadProfile();
		await loadAssistantProfile();
	});

	async function loadAssistantProfile() {
		try {
			const response = await fetch('/api/assistant-profile');
			if (response.ok) {
				const profile = await response.json();
				if (profile.ui_preferences?.fallbackView) {
					fallbackView = profile.ui_preferences.fallbackView;
				}
			}
		} catch (error) {
			console.error('Failed to load assistant profile:', error);
		}
	}

	async function handleFallbackChange(newFallback: FallbackView) {
		fallbackView = newFallback;
		spaceStore.setFallbackPreference(newFallback);

		try {
			const profileRes = await fetch('/api/assistant-profile');
			let existingPrefs = {};
			if (profileRes.ok) {
				const profile = await profileRes.json();
				existingPrefs = profile.ui_preferences || {};
			}

			await fetch('/api/assistant-profile', {
				method: 'PUT',
				headers: { 'Content-Type': 'application/json' },
				body: JSON.stringify({
					ui_preferences: {
						...existingPrefs,
						fallbackView: newFallback
					}
				})
			});
		} catch (error) {
			console.error('Failed to save fallback preference:', error);
		}
	}

	const fallbackOptions: { id: FallbackView; icon: string; title: string }[] = [
		{ id: 'empty', icon: 'ri:checkbox-blank-line', title: 'Discovery' },
		{ id: 'chat', icon: 'ri:chat-1-line', title: 'New Chat' },
		{ id: 'conway', icon: 'ri:seedling-line', title: 'Zen Garden' },
		{ id: 'dog-jump', icon: 'ri:run-line', title: 'Dog Jump' },
		{ id: 'wiki-today', icon: 'ri:calendar-line', title: 'Today' }
	];

	async function loadProfile() {
		loading = true;
		try {
			const response = await fetch('/api/profile');
			if (response.ok) {
				const profile = await response.json();

				fullName = profile.full_name || '';
				preferredName = profile.preferred_name || '';
				birthDate = profile.birth_date ? profile.birth_date.split('T')[0] : '';
				heightCm = profile.height_cm || '';
				weightKg = profile.weight_kg || '';
				ethnicity = profile.ethnicity || '';
				occupation = profile.occupation || '';
				employer = profile.employer || '';

				if (profile.theme && isValidTheme(profile.theme)) {
					currentTheme = profile.theme as Theme;
					// Don't call setTheme here as it would trigger a DB write of the same value
					applyTheme(currentTheme);
				}
			}
		} catch (error) {
			console.error('Failed to load profile:', error);
		} finally {
			loading = false;
		}
	}

	async function saveField(field: string, value: string | number | null) {
		try {
			const response = await fetch('/api/profile', {
				method: 'PUT',
				headers: { 'Content-Type': 'application/json' },
				body: JSON.stringify({ [field]: value })
			});

			if (!response.ok) {
				throw new Error(`Failed to save ${field}`);
			}

			invalidate('/api/profile');
		} catch (error) {
			console.error(`Failed to save ${field}:`, error);
			throw error;
		}
	}
</script>

<Page>
	<div class="max-w-3xl">
		<div class="mb-8">
			<h1 class="text-3xl font-serif font-medium text-foreground mb-2">Profile</h1>
			<p class="text-foreground-muted">Manage your personal information and preferences</p>
		</div>

		{#if loading}
			<div class="text-center py-12 text-foreground-subtle">Loading profile...</div>
		{:else}
			<div class="space-y-6 mb-6">
				<div class="bg-surface border border-border rounded-lg p-6">
					<h2 class="text-lg font-medium text-foreground mb-4">Appearance</h2>
					<ThemePicker value={currentTheme} onchange={handleThemeChange} />
				</div>

				<div class="bg-surface border border-border rounded-lg p-6">
					<h2 class="text-lg font-medium text-foreground mb-2">Homepage</h2>
					<p class="text-sm text-foreground-muted mb-4">What to show when all tabs are closed</p>
					<div class="fallback-options">
						{#each fallbackOptions as option}
							<button
								class="fallback-option"
								class:active={fallbackView === option.id}
								onclick={() => handleFallbackChange(option.id)}
							>
								<Icon icon={option.icon} width="16"/>
								<span>{option.title}</span>
							</button>
						{/each}
					</div>
				</div>
			</div>

			<div class="space-y-6">
				<div class="bg-surface border border-border rounded-lg p-6">
					<h2 class="text-lg font-medium text-foreground mb-4">Identity</h2>
					<div class="space-y-4">
						<div>
							<label for="fullName" class="block text-sm font-medium text-foreground-muted mb-2">
								Full Name
							</label>
							<Input
								type="text"
								id="fullName"
								bind:value={fullName}
								placeholder="Your full legal name"
								autoSave
								onSave={(val) => saveField('full_name', val || null)}
							/>
						</div>

						<div>
							<label for="preferredName" class="block text-sm font-medium text-foreground-muted mb-2">
								Preferred Name
							</label>
							<Input
								type="text"
								id="preferredName"
								bind:value={preferredName}
								placeholder="How should the assistant address you?"
								autoSave
								onSave={(val) => saveField('preferred_name', val || null)}
							/>
						</div>

						<div>
							<label for="birthDate" class="block text-sm font-medium text-foreground-muted mb-2">
								Birth Date
							</label>
							<Input
								type="date"
								id="birthDate"
								bind:value={birthDate}
								autoSave
								onSave={(val) => saveField('birth_date', val || null)}
							/>
						</div>
					</div>
				</div>

				<div class="bg-surface border border-border rounded-lg p-6">
					<h2 class="text-lg font-medium text-foreground mb-4">Physical Details</h2>
					<div class="space-y-4">
						<div class="grid grid-cols-2 gap-4">
							<div>
								<label for="heightCm" class="block text-sm font-medium text-foreground-muted mb-2">
									Height (cm)
								</label>
								<Input
									type="number"
									step="0.01"
									id="heightCm"
									bind:value={heightCm}
									placeholder="175.5"
									autoSave
									onSave={(val) => {
										const num = parseFloat(String(val ?? ''));
										saveField('height_cm', isNaN(num) ? null : num);
									}}
								/>
							</div>

							<div>
								<label for="weightKg" class="block text-sm font-medium text-foreground-muted mb-2">
									Weight (kg)
								</label>
								<Input
									type="number"
									step="0.01"
									id="weightKg"
									bind:value={weightKg}
									placeholder="70.5"
									autoSave
									onSave={(val) => {
										const num = parseFloat(String(val ?? ''));
										saveField('weight_kg', isNaN(num) ? null : num);
									}}
								/>
							</div>
						</div>

						<div>
							<label for="ethnicity" class="block text-sm font-medium text-foreground-muted mb-2">
								Ethnicity
							</label>
							<Input
								type="text"
								id="ethnicity"
								bind:value={ethnicity}
								placeholder="Optional"
								autoSave
								onSave={(val) => saveField('ethnicity', val || null)}
							/>
						</div>
					</div>
				</div>

				<div class="bg-surface border border-border rounded-lg p-6">
					<h2 class="text-lg font-medium text-foreground mb-4">Work Information</h2>
					<div class="space-y-4">
						<div>
							<label for="occupation" class="block text-sm font-medium text-foreground-muted mb-2">
								Occupation
							</label>
							<Input
								type="text"
								id="occupation"
								bind:value={occupation}
								placeholder="Software Engineer, Designer, Student, etc."
								autoSave
								onSave={(val) => saveField('occupation', val || null)}
							/>
						</div>

						<div>
							<label for="employer" class="block text-sm font-medium text-foreground-muted mb-2">
								Employer
							</label>
							<Input
								type="text"
								id="employer"
								bind:value={employer}
								placeholder="Company name (optional)"
								autoSave
								onSave={(val) => saveField('employer', val || null)}
							/>
							<p class="text-xs text-foreground-subtle mt-1">
								Leave blank if self-employed or not applicable
							</p>
						</div>
					</div>
				</div>
			</div>
		{/if}
	</div>
</Page>

<style>
	.fallback-options {
		display: flex;
		flex-wrap: wrap;
		gap: 0.5rem;
	}

	.fallback-option {
		display: flex;
		align-items: center;
		gap: 0.4rem;
		padding: 0.5rem 0.75rem;
		border: 1px solid var(--color-border);
		border-radius: 6px;
		background: var(--color-surface);
		color: var(--color-foreground-muted);
		font-size: 0.85rem;
		cursor: pointer;
		transition: all 0.15s ease;
	}

	.fallback-option:hover {
		border-color: var(--color-foreground-muted);
		color: var(--color-foreground);
	}

	.fallback-option.active {
		border-color: var(--color-primary);
		background: color-mix(in srgb, var(--color-primary) 10%, transparent);
		color: var(--color-primary);
	}
</style>
