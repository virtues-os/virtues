<script lang="ts">
	import type { Tab } from '$lib/stores/windowTabs.svelte';
	import { Button, Page, Input } from '$lib';
	import PlacesAutocomplete from '$lib/components/PlacesAutocomplete.svelte';
	import 'iconify-icon';
	import { onMount } from 'svelte';

	let { tab, active }: { tab: Tab; active: boolean } = $props();

	interface Place {
		id?: string;
		label: string;
		formatted_address: string;
		latitude: number;
		longitude: number;
		google_place_id?: string;
	}

	let activeTabSection = $state('places');
	let places = $state<Place[]>([]);
	let homePlaceId = $state<string | null>(null);
	let loading = $state(true);
	let saving = $state(false);
	let showAddForm = $state(false);
	let editingPlaceId = $state<string | null>(null);
	let newPlaceLabel = $state('');
	let newPlaceAddress = $state('');
	let newPlaceData = $state<{
		formatted_address: string;
		latitude: number;
		longitude: number;
		google_place_id?: string;
	} | null>(null);

	onMount(async () => {
		await loadData();
	});

	async function loadData() {
		loading = true;
		try {
			const [placesRes, profileRes] = await Promise.all([
				fetch('/api/entities/places'),
				fetch('/api/profile')
			]);
			if (placesRes.ok) {
				places = await placesRes.json();
			}
			if (profileRes.ok) {
				const profile = await profileRes.json();
				homePlaceId = profile.home_place_id || null;
			}
		} catch (error) {
			console.error('Failed to load data:', error);
		} finally {
			loading = false;
		}
	}

	async function handleNewPlaceSelect(place: {
		formatted_address: string;
		latitude: number;
		longitude: number;
		google_place_id?: string;
	}) {
		newPlaceAddress = place.formatted_address;
		newPlaceData = place;
	}

	async function addPlace() {
		if (!newPlaceLabel.trim() || !newPlaceData) return;

		saving = true;
		try {
			const response = await fetch('/api/entities/places', {
				method: 'POST',
				headers: { 'Content-Type': 'application/json' },
				body: JSON.stringify({
					label: newPlaceLabel.trim(),
					formatted_address: newPlaceData.formatted_address,
					latitude: newPlaceData.latitude,
					longitude: newPlaceData.longitude,
					google_place_id: newPlaceData.google_place_id
				})
			});

			if (response.ok) {
				const result = await response.json();
				places = [
					...places,
					{
						id: result.id,
						label: newPlaceLabel.trim(),
						formatted_address: newPlaceData.formatted_address,
						latitude: newPlaceData.latitude,
						longitude: newPlaceData.longitude,
						google_place_id: newPlaceData.google_place_id
					}
				];
				newPlaceLabel = '';
				newPlaceAddress = '';
				newPlaceData = null;
				showAddForm = false;
			}
		} catch (error) {
			console.error('Failed to save place:', error);
		} finally {
			saving = false;
		}
	}

	async function removePlace(placeId: string, placeLabel: string) {
		const confirmed = confirm(
			`Are you sure you want to remove "${placeLabel}"? This action cannot be undone.`
		);
		if (!confirmed) return;

		saving = true;
		try {
			const response = await fetch(`/api/entities/places/${placeId}`, {
				method: 'DELETE'
			});

			if (response.ok) {
				places = places.filter((p) => p.id !== placeId);
				if (homePlaceId === placeId) {
					homePlaceId = null;
					await fetch('/api/profile', {
						method: 'PUT',
						headers: { 'Content-Type': 'application/json' },
						body: JSON.stringify({ home_place_id: null })
					});
				}
			}
		} catch (error) {
			console.error('Failed to delete place:', error);
		} finally {
			saving = false;
		}
	}

	async function togglePrimaryResidence(placeId: string) {
		const newHomePlaceId = homePlaceId === placeId ? null : placeId;

		saving = true;
		try {
			const response = await fetch('/api/profile', {
				method: 'PUT',
				headers: { 'Content-Type': 'application/json' },
				body: JSON.stringify({ home_place_id: newHomePlaceId })
			});

			if (response.ok) {
				homePlaceId = newHomePlaceId;
			}
		} catch (error) {
			console.error('Failed to update primary residence:', error);
		} finally {
			saving = false;
		}
	}

	function startEditPlace(place: Place) {
		editingPlaceId = place.id || null;
		newPlaceLabel = place.label;
		newPlaceAddress = place.formatted_address;
		newPlaceData = {
			formatted_address: place.formatted_address,
			latitude: place.latitude,
			longitude: place.longitude,
			google_place_id: place.google_place_id
		};
		showAddForm = false;
	}

	function cancelEdit() {
		editingPlaceId = null;
		newPlaceLabel = '';
		newPlaceAddress = '';
		newPlaceData = null;
	}

	async function saveEditPlace() {
		if (!editingPlaceId || !newPlaceLabel.trim() || !newPlaceData) return;

		const placeId = editingPlaceId;
		const label = newPlaceLabel.trim();
		const placeData = newPlaceData;

		saving = true;
		try {
			const response = await fetch(`/api/entities/places/${placeId}`, {
				method: 'PUT',
				headers: { 'Content-Type': 'application/json' },
				body: JSON.stringify({
					label,
					formatted_address: placeData.formatted_address,
					latitude: placeData.latitude,
					longitude: placeData.longitude,
					google_place_id: placeData.google_place_id
				})
			});

			if (response.ok) {
				places = places.map((p) =>
					p.id === placeId
						? {
								...p,
								label,
								formatted_address: placeData.formatted_address,
								latitude: placeData.latitude,
								longitude: placeData.longitude,
								google_place_id: placeData.google_place_id
							}
						: p
				);
				cancelEdit();
			}
		} catch (error) {
			console.error('Failed to update place:', error);
		} finally {
			saving = false;
		}
	}

	function setTab(tabName: string) {
		activeTabSection = tabName;
	}
</script>

<Page>
	<div class="max-w-3xl">
		<div class="mb-8">
			<h1 class="text-3xl font-serif font-medium text-foreground mb-2">Entities</h1>
			<p class="text-foreground-muted">Places, people, and things that matter to you.</p>
		</div>

		<div class="flex gap-6 border-b border-border mb-8">
			<button
				onclick={() => setTab('places')}
				class="pb-2 text-sm font-medium border-b-2 -mb-px transition-colors {activeTabSection === 'places'
					? 'border-foreground text-foreground'
					: 'border-transparent text-foreground-muted hover:text-foreground'}"
			>
				Places
			</button>
			<button
				disabled
				class="pb-2 text-sm font-medium border-b-2 -mb-px border-transparent text-foreground-subtle cursor-not-allowed"
			>
				People
			</button>
			<button
				disabled
				class="pb-2 text-sm font-medium border-b-2 -mb-px border-transparent text-foreground-subtle cursor-not-allowed"
			>
				Things
			</button>
		</div>

		{#if loading}
			<div class="text-center py-12 text-foreground-muted">Loading...</div>
		{:else if activeTabSection === 'places'}
			<div class="space-y-6">
				<div class="bg-surface border border-border rounded-lg p-6">
					<div class="flex items-center justify-between mb-4">
						<h2 class="text-lg font-medium text-foreground">Your Places</h2>
						{#if !showAddForm && places.length > 0}
							<Button variant="ghost" size="sm" onclick={() => (showAddForm = true)}>+ Add</Button>
						{/if}
					</div>

					{#if places.length > 0}
						<ul class="space-y-3">
							{#each places as place}
								{#if editingPlaceId === place.id}
									<li class="space-y-4 p-4 bg-surface-elevated rounded-lg">
										<div class="grid grid-cols-1 sm:grid-cols-3 gap-4">
											<div class="sm:col-span-2">
												<label for="edit-place-address" class="block text-sm font-medium text-foreground-muted mb-2">
													Address
												</label>
												<PlacesAutocomplete
													value={newPlaceAddress}
													placeholder="Search for address..."
													onSelect={handleNewPlaceSelect}
												/>
											</div>
											<div>
												<label for="edit-place-label" class="block text-sm font-medium text-foreground-muted mb-2">
													Label
												</label>
												<Input id="edit-place-label" type="text" bind:value={newPlaceLabel} placeholder="Home, Work, Gym..." />
											</div>
										</div>
										<div class="flex items-center gap-3">
											<Button variant="primary" size="sm" onclick={saveEditPlace} disabled={saving || !newPlaceLabel.trim() || !newPlaceData}>
												{saving ? 'Saving...' : 'Save'}
											</Button>
											<Button variant="ghost" size="sm" onclick={cancelEdit}>Cancel</Button>
										</div>
									</li>
								{:else}
									<li class="flex items-center justify-between">
										<div class="min-w-0 flex-1">
											<div class="flex items-center gap-2">
												<p class="text-foreground font-medium truncate">{place.label}</p>
												{#if homePlaceId === place.id}
													<span class="text-xs text-primary font-medium">Primary</span>
												{/if}
											</div>
											<p class="text-sm text-foreground-muted truncate">{place.formatted_address}</p>
										</div>
										<div class="flex items-center gap-1 shrink-0 ml-4">
											{#if homePlaceId !== place.id}
												<Button variant="ghost" size="sm" onclick={() => place.id && togglePrimaryResidence(place.id)} disabled={saving || !place.id}>
													Set as primary
												</Button>
											{/if}
											<Button variant="ghost" size="sm" onclick={() => startEditPlace(place)} disabled={saving}>Edit</Button>
											<Button variant="ghost" size="sm" onclick={() => place.id && removePlace(place.id, place.label)} disabled={saving}>Remove</Button>
										</div>
									</li>
								{/if}
							{/each}
						</ul>
					{:else if !showAddForm && !editingPlaceId}
						<p class="text-foreground-subtle">No places added yet. Add your first place to get started.</p>
					{/if}

					{#if (showAddForm || places.length === 0) && !editingPlaceId}
						<div class="space-y-4 p-4 bg-surface-elevated rounded-lg {places.length > 0 ? 'mt-3' : ''}">
							<div class="grid grid-cols-1 sm:grid-cols-3 gap-4">
								<div class="sm:col-span-2">
									<label for="place-address" class="block text-sm font-medium text-foreground-muted mb-2">Address</label>
									<PlacesAutocomplete value={newPlaceAddress} placeholder="Search for address..." onSelect={handleNewPlaceSelect} />
								</div>
								<div>
									<label for="place-label" class="block text-sm font-medium text-foreground-muted mb-2">Label</label>
									<Input id="place-label" type="text" bind:value={newPlaceLabel} placeholder="Home, Work, Gym..." />
								</div>
							</div>
							<div class="flex items-center gap-3">
								<Button variant="primary" size="sm" onclick={addPlace} disabled={saving || !newPlaceLabel.trim() || !newPlaceData}>
									{saving ? 'Saving...' : 'Add Place'}
								</Button>
								{#if places.length > 0}
									<Button
										variant="ghost"
										size="sm"
										onclick={() => {
											showAddForm = false;
											newPlaceLabel = '';
											newPlaceAddress = '';
											newPlaceData = null;
										}}
									>
										Cancel
									</Button>
								{/if}
							</div>
						</div>
					{/if}
				</div>
			</div>
		{:else if activeTabSection === 'people'}
			<div class="bg-surface border border-border rounded-lg p-6">
				<p class="text-foreground-muted">Coming soon.</p>
			</div>
		{:else if activeTabSection === 'things'}
			<div class="bg-surface border border-border rounded-lg p-6">
				<p class="text-foreground-muted">Coming soon.</p>
			</div>
		{/if}
	</div>
</Page>
