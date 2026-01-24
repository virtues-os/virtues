<script lang="ts">
	import { getContext, onMount } from "svelte";
	import { invalidate } from "$app/navigation";
	import { Input, Button } from "$lib";
	import PlacesAutocomplete from "$lib/components/PlacesAutocomplete.svelte";

	interface Place {
		id?: string;
		label: string;
		address: string;
		latitude?: number;
		longitude?: number;
		google_place_id?: string;
	}

	// API returns formatted_address, we use address internally
	interface ApiLocation {
		id?: string;
		label: string;
		formatted_address?: string;
		address?: string;
		latitude?: number;
		longitude?: number;
		google_place_id?: string;
	}

	// Get onboarding context to control continue button and register data
	const { setCanContinue, registerStepData } = getContext<{
		setCanContinue: (value: boolean) => void;
		registerStepData: (data: Record<string, unknown>) => void;
	}>("onboarding");

	// Helper to save a place via API
	async function savePlace(place: Place, setAsHome = false): Promise<string> {
		const response = await fetch("/api/entities/places", {
			method: "POST",
			headers: { "Content-Type": "application/json" },
			body: JSON.stringify({
				label: place.label,
				formatted_address: place.address,
				latitude: place.latitude,
				longitude: place.longitude,
				google_place_id: place.google_place_id,
				set_as_home: setAsHome,
			}),
		});
		if (!response.ok) throw new Error("Failed to save place");
		const data = await response.json();
		// Invalidate places data so it reloads on navigation
		invalidate("/api/entities/places");
		return data.id;
	}

	// Helper to delete a place via API
	async function deletePlace(id: string): Promise<void> {
		const response = await fetch(`/api/entities/places/${id}`, {
			method: "DELETE",
		});
		if (!response.ok) throw new Error("Failed to delete place");
		// Invalidate places data so it reloads on navigation
		invalidate("/api/entities/places");
	}

	// Transform API locations to internal format
	function toPlace(loc: ApiLocation): Place {
		return {
			id: loc.id,
			label: loc.label,
			address: loc.formatted_address || loc.address || "",
			latitude: loc.latitude,
			longitude: loc.longitude,
			google_place_id: loc.google_place_id,
		};
	}

	// State for places - will be loaded on mount
	let homeAddress = $state("");
	let homePlace = $state<Place | null>(null);
	let additionalPlaces = $state<Place[]>([]);
	let isLoading = $state(true);

	// Load places data on mount
	onMount(async () => {
		try {
			const [placesRes, profileRes] = await Promise.all([
				fetch("/api/entities/places"),
				fetch("/api/profile"),
			]);

			const places = placesRes.ok ? await placesRes.json() : [];
			const profile = profileRes.ok ? await profileRes.json() : null;
			const homePlaceId = profile?.home_place_id;

			// Transform API response to our format
			const locations: ApiLocation[] = places.map(
				(p: {
					id: string;
					name: string;
					address?: string;
					latitude: number;
					longitude: number;
					metadata?: {
						google_place_id?: string;
					};
				}) => ({
					id: p.id,
					label: p.name,
					formatted_address: p.address || "",
					latitude: p.latitude,
					longitude: p.longitude,
					google_place_id: p.metadata?.google_place_id,
				}),
			);

			const homeLocation = locations.find(
				(l) => l.id === homePlaceId || l.label === "Home",
			);
			const otherLocations = locations.filter(
				(l) => l.id !== homePlaceId && l.label !== "Home",
			);

			if (homeLocation) {
				homeAddress =
					homeLocation.formatted_address ||
					homeLocation.address ||
					"";
				homePlace = toPlace(homeLocation);
			}
			additionalPlaces = otherLocations.map(toPlace);
		} catch (e) {
			console.error("Failed to load places:", e);
		} finally {
			isLoading = false;
		}
	});
	let newPlaceLabel = $state("");
	let newPlaceAddress = $state("");
	let newPlaceData = $state<{
		latitude: number;
		longitude: number;
		google_place_id?: string;
	} | null>(null);
	let isSavingHome = $state(false);
	let isSavingPlace = $state(false);

	// Update canContinue and register data whenever places change
	$effect(() => {
		setCanContinue(!!homePlace);
		registerStepData({ homePlace, additionalPlaces });
	});

	async function handleHomeSelect(place: {
		formatted_address: string;
		latitude: number;
		longitude: number;
		google_place_id?: string;
	}) {
		homeAddress = place.formatted_address;
		const newHomePlace: Place = {
			label: "Home",
			address: place.formatted_address,
			latitude: place.latitude,
			longitude: place.longitude,
			google_place_id: place.google_place_id,
		};

		// Save immediately
		isSavingHome = true;
		try {
			const id = await savePlace(newHomePlace, true);
			homePlace = { ...newHomePlace, id };
		} catch (e) {
			console.error("Failed to save home:", e);
		} finally {
			isSavingHome = false;
		}
	}

	async function handleNewPlaceSelect(place: {
		formatted_address: string;
		latitude: number;
		longitude: number;
		google_place_id?: string;
	}) {
		newPlaceAddress = place.formatted_address;
		newPlaceData = {
			latitude: place.latitude,
			longitude: place.longitude,
			google_place_id: place.google_place_id,
		};

		// Auto-save if label is already filled
		if (newPlaceLabel.trim()) {
			await addPlace();
		}
	}

	async function handleLabelBlur() {
		// Auto-save if both fields are filled
		if (newPlaceLabel.trim() && newPlaceData) {
			await addPlace();
		}
	}

	async function addPlace() {
		if (!newPlaceLabel.trim() || !newPlaceData || isSavingPlace) return;

		const newPlace: Place = {
			label: newPlaceLabel.trim(),
			address: newPlaceAddress.trim(),
			latitude: newPlaceData.latitude,
			longitude: newPlaceData.longitude,
			google_place_id: newPlaceData.google_place_id,
		};

		// Save immediately
		isSavingPlace = true;
		try {
			const id = await savePlace(newPlace, false);
			additionalPlaces = [...additionalPlaces, { ...newPlace, id }];
			newPlaceLabel = "";
			newPlaceAddress = "";
			newPlaceData = null;
		} catch (e) {
			console.error("Failed to save place:", e);
		} finally {
			isSavingPlace = false;
		}
	}

	async function removePlace(index: number) {
		const place = additionalPlaces[index];
		if (place.id) {
			try {
				await deletePlace(place.id);
			} catch (e) {
				console.error("Failed to delete place:", e);
				return;
			}
		}
		additionalPlaces = additionalPlaces.filter((_, i) => i !== index);
	}
</script>

<div class="markdown w-full max-w-xl mx-auto">
	<header>
		<h1 class="text-4xl!">Your Places</h1>
		<p class="mt-2 text-foreground-muted">
			Understanding where you live helps your Personal AI provide
			contextual insights about your daily life and travels.
		</p>
	</header>

	{#if isLoading}
		<div class="mt-8 text-center text-foreground-subtle">Loading...</div>
	{:else}
		<section class="mt-8">
			<h2>Home Address</h2>
			<p class="mt-2 text-foreground-muted">
				This is used to understand your local context.
			</p>
			<div class="flex flex-col gap-4 mt-4">
				<label class="flex flex-col gap-2">
					<span class="text-sm text-foreground-subtle"
						>Your home address</span
					>
					<PlacesAutocomplete
						bind:value={homeAddress}
						placeholder="Start typing your address..."
						onSelect={handleHomeSelect}
						success={!!homePlace}
					/>
				</label>
			</div>
		</section>

		<section class="mt-8">
			<h2>Other Places</h2>
			<p class="mt-2 text-foreground-muted">
				Optionally add other significant places like your workplace,
				gym, or favorite coffee shop.
			</p>

			{#if additionalPlaces.length > 0}
				<div class="flex flex-col gap-3 mt-4">
					{#each additionalPlaces as place, index}
						<div
							class="flex items-center justify-between p-3 bg-surface border border-border rounded-lg"
						>
							<div class="flex flex-col gap-0.5">
								<span
									class="text-sm font-medium text-foreground"
									>{place.label}</span
								>
								<span class="text-xs text-foreground-muted"
									>{place.address}</span
								>
							</div>
							<button
								type="button"
								onclick={() => removePlace(index)}
								class="p-1.5 text-foreground-muted hover:text-foreground transition-colors cursor-pointer"
								aria-label="Remove place"
							>
								<svg
									xmlns="http://www.w3.org/2000/svg"
									width="16"
									height="16"
									viewBox="0 0 24 24"
									fill="none"
									stroke="currentColor"
									stroke-width="2"
									stroke-linecap="round"
									stroke-linejoin="round"
								>
									<line x1="18" y1="6" x2="6" y2="18"></line>
									<line x1="6" y1="6" x2="18" y2="18"></line>
								</svg>
							</button>
						</div>
					{/each}
				</div>
			{/if}

			<div class="flex flex-col gap-4 mt-4">
				<div class="flex gap-3">
					<label class="flex flex-col gap-2 flex-1">
						<span class="text-sm text-foreground-subtle"
							>Address</span
						>
						<PlacesAutocomplete
							bind:value={newPlaceAddress}
							placeholder="Start typing an address..."
							onSelect={handleNewPlaceSelect}
							success={!!newPlaceData}
						/>
					</label>
					<label class="flex flex-col gap-2 w-32">
						<span class="text-sm text-foreground-subtle">Label</span
						>
						<Input
							type="text"
							placeholder="e.g., Work"
							bind:value={newPlaceLabel}
							disabled={!newPlaceData}
							success={!!newPlaceLabel.trim() && !!newPlaceData}
							onblur={handleLabelBlur}
						/>
					</label>
				</div>
				<Button
					variant="secondary"
					onclick={addPlace}
					disabled={!newPlaceLabel.trim() ||
						!newPlaceData ||
						isSavingPlace}
				>
					{isSavingPlace ? "Saving..." : "+ Add Place"}
				</Button>
			</div>
		</section>
	{/if}
</div>
