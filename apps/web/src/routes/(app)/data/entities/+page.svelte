<script lang="ts">
	import { page } from '$app/stores';
	import { goto, invalidateAll } from '$app/navigation';
	import { Button, Page } from '$lib';
	import PlacesAutocomplete from '$lib/components/PlacesAutocomplete.svelte';
	import 'iconify-icon';

	interface Place {
		id?: string;
		label: string;
		formatted_address: string;
		latitude: number;
		longitude: number;
		google_place_id?: string;
	}

	let { data } = $props();

	// Tab state from URL
	const activeTab = $derived($page.url.searchParams.get('tab') || 'places');
	const isOnboarding = $derived($page.url.searchParams.get('onboarding') === 'true');

	// Initialize places from load data
	const homePlace = $derived(
		data.locations.find((loc: Place) => loc.id === data.homePlaceId) || null
	);
	const otherPlaces = $derived(
		data.locations.filter((loc: Place) => loc.id !== data.homePlaceId)
	);

	// Places state for editing
	let saving = $state(false);
	let homeAddress = $state('');
	let existingHomePlace = $state<Place | null>(null);
	let places = $state<Place[]>([]);

	// Initialize from load data
	$effect(() => {
		if (homePlace) {
			existingHomePlace = homePlace;
		}
		places = otherPlaces;
	});

	// New place form
	let newPlaceLabel = $state('');
	let showAddForm = $state(false);

	async function handleHomeSelect(place: { formatted_address: string; latitude: number; longitude: number; google_place_id?: string }) {
		saving = true;
		try {
			const response = await fetch('/api/entities/places', {
				method: 'POST',
				headers: { 'Content-Type': 'application/json' },
				body: JSON.stringify({
					label: 'Home',
					formatted_address: place.formatted_address,
					latitude: place.latitude,
					longitude: place.longitude,
					google_place_id: place.google_place_id,
					set_as_home: true
				})
			});

			if (response.ok) {
				const result = await response.json();
				existingHomePlace = {
					id: result.id,
					label: 'Home',
					formatted_address: place.formatted_address,
					latitude: place.latitude,
					longitude: place.longitude,
					google_place_id: place.google_place_id
				};
				homeAddress = place.formatted_address;
				// Refresh data
				await invalidateAll();
			}
		} catch (error) {
			console.error('Failed to save home place:', error);
		} finally {
			saving = false;
		}
	}

	async function handleNewPlaceSelect(place: { formatted_address: string; latitude: number; longitude: number; google_place_id?: string }) {
		if (!newPlaceLabel.trim()) return;

		saving = true;
		try {
			const response = await fetch('/api/entities/places', {
				method: 'POST',
				headers: { 'Content-Type': 'application/json' },
				body: JSON.stringify({
					label: newPlaceLabel.trim(),
					formatted_address: place.formatted_address,
					latitude: place.latitude,
					longitude: place.longitude,
					google_place_id: place.google_place_id
				})
			});

			if (response.ok) {
				const result = await response.json();
				places = [...places, {
					id: result.id,
					label: newPlaceLabel.trim(),
					formatted_address: place.formatted_address,
					latitude: place.latitude,
					longitude: place.longitude,
					google_place_id: place.google_place_id
				}];
				newPlaceLabel = '';
				showAddForm = false;
			}
		} catch (error) {
			console.error('Failed to save place:', error);
		} finally {
			saving = false;
		}
	}

	async function removePlace(index: number) {
		const place = places[index];
		if (!place.id) {
			places = places.filter((_, i) => i !== index);
			return;
		}

		saving = true;
		try {
			const response = await fetch(`/api/entities/places/${place.id}`, {
				method: 'DELETE'
			});

			if (response.ok) {
				places = places.filter((_, i) => i !== index);
			}
		} catch (error) {
			console.error('Failed to delete place:', error);
		} finally {
			saving = false;
		}
	}

	function setTab(tab: string) {
		const url = new URL($page.url);
		url.searchParams.set('tab', tab);
		goto(url.toString(), { replaceState: true });
	}

	async function completeOnboardingStep() {
		saving = true;
		try {
			goto('/');
		} finally {
			saving = false;
		}
	}
</script>

<svelte:head>
	<title>Entities | Virtues</title>
</svelte:head>

<Page>
	<div class="max-w-2xl">
		<!-- Onboarding header -->
		{#if isOnboarding}
			<p class="text-sm text-primary mb-6">Step 2 of 4</p>
		{/if}

		<!-- Header -->
		<div class="mb-8">
			<h1 class="text-2xl font-serif font-medium text-foreground mb-2">
				Entities
			</h1>
			<p class="text-foreground-muted">
				Places, people, and things that matter to you.
			</p>
		</div>

		<!-- Tab navigation -->
		<div class="flex gap-6 border-b border-border mb-8">
			<button
				onclick={() => setTab('places')}
				class="pb-2 text-sm font-medium border-b-2 -mb-px transition-colors {activeTab === 'places' ? 'border-foreground text-foreground' : 'border-transparent text-foreground-muted hover:text-foreground'}"
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

		<!-- Tab content -->
		{#if activeTab === 'places'}
			<div class="space-y-8">
				<!-- Home section -->
				<section>
					<h2 class="text-sm font-medium text-foreground-muted uppercase tracking-wide mb-3">Home</h2>
					{#if existingHomePlace}
						<div class="flex items-center justify-between py-2">
							<span class="text-foreground">{existingHomePlace.formatted_address}</span>
							<button
								type="button"
								onclick={() => existingHomePlace = null}
								class="text-sm text-foreground-muted hover:text-foreground"
							>
								Edit
							</button>
						</div>
					{:else}
						<PlacesAutocomplete
							value={homeAddress}
							placeholder="Search for your home address..."
							onSelect={handleHomeSelect}
							class="w-full px-3 py-2 bg-surface border border-border rounded focus:outline-none focus:ring-1 focus:ring-primary focus:border-primary"
						/>
					{/if}
				</section>

				<!-- Other places section -->
				<section>
					<div class="flex items-center justify-between mb-3">
						<h2 class="text-sm font-medium text-foreground-muted uppercase tracking-wide">Other Places</h2>
						{#if !showAddForm}
							<button
								type="button"
								onclick={() => showAddForm = true}
								class="text-sm text-primary hover:underline"
							>
								+ Add
							</button>
						{/if}
					</div>

					{#if places.length > 0}
						<ul class="divide-y divide-border">
							{#each places as place, index}
								<li class="flex items-center justify-between py-3">
									<div>
										<p class="text-foreground font-medium">{place.label}</p>
										<p class="text-sm text-foreground-muted">{place.formatted_address}</p>
									</div>
									<button
										type="button"
										onclick={() => removePlace(index)}
										disabled={saving}
										class="text-sm text-foreground-muted hover:text-error disabled:opacity-50"
									>
										Remove
									</button>
								</li>
							{/each}
						</ul>
					{:else if !showAddForm}
						<p class="text-foreground-subtle py-2">No places added yet.</p>
					{/if}

					{#if showAddForm}
						<div class="space-y-3 py-3 border-t border-border mt-3">
							<div>
								<label for="place-label" class="block text-sm text-foreground-muted mb-1">Label</label>
								<input
									id="place-label"
									type="text"
									bind:value={newPlaceLabel}
									placeholder="Work, Gym, etc."
									class="w-full px-3 py-2 bg-surface border border-border rounded focus:outline-none focus:ring-1 focus:ring-primary focus:border-primary"
								/>
							</div>
							<div>
								<label for="place-address" class="block text-sm text-foreground-muted mb-1">Address</label>
								<PlacesAutocomplete
									placeholder="Search for address..."
									onSelect={handleNewPlaceSelect}
									class="w-full px-3 py-2 bg-surface border border-border rounded focus:outline-none focus:ring-1 focus:ring-primary focus:border-primary"
								/>
							</div>
							<button
								type="button"
								onclick={() => { showAddForm = false; newPlaceLabel = ''; }}
								class="text-sm text-foreground-muted hover:text-foreground"
							>
								Cancel
							</button>
						</div>
					{/if}
				</section>

				<!-- Onboarding actions -->
				{#if isOnboarding}
					<div class="flex items-center justify-between pt-6 border-t border-border">
						<a
							href="/onboarding"
							class="text-sm text-foreground-muted hover:text-foreground"
						>
							Back
						</a>
						<Button
							variant="primary"
							onclick={completeOnboardingStep}
							disabled={saving}
						>
							{saving ? 'Saving...' : 'Continue'}
						</Button>
					</div>
				{/if}
			</div>
		{:else if activeTab === 'people'}
			<div class="py-8">
				<p class="text-foreground-muted">Coming soon.</p>
			</div>
		{:else if activeTab === 'things'}
			<div class="py-8">
				<p class="text-foreground-muted">Coming soon.</p>
			</div>
		{/if}
	</div>
</Page>
