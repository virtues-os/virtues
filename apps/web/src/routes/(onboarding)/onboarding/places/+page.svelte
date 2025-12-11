<script lang="ts">
	import { getContext } from "svelte";
	import PlacesAutocomplete from "$lib/components/PlacesAutocomplete.svelte";

	interface Place {
		label: string;
		address: string;
		latitude?: number;
		longitude?: number;
		google_place_id?: string;
	}

	// API returns formatted_address, we use address internally
	interface ApiLocation {
		label: string;
		formatted_address?: string;
		address?: string;
		latitude?: number;
		longitude?: number;
		google_place_id?: string;
	}

	// Get onboarding context to control continue button and register data
	const { setCanContinue, registerStepData, initialData } = getContext<{
		setCanContinue: (value: boolean) => void;
		registerStepData: (data: Record<string, unknown>) => void;
		initialData: { locations?: ApiLocation[] };
	}>("onboarding");

	// Transform API locations to internal format
	function toPlace(loc: ApiLocation): Place {
		return {
			label: loc.label,
			address: loc.formatted_address || loc.address || "",
			latitude: loc.latitude,
			longitude: loc.longitude,
			google_place_id: loc.google_place_id,
		};
	}

	// Extract home and additional places from locations array
	const locations = initialData?.locations || [];
	const homeLocation = locations.find((l) => l.label === "Home");
	const otherLocations = locations.filter((l) => l.label !== "Home");

	let homeAddress = $state(
		homeLocation?.formatted_address || homeLocation?.address || "",
	);
	let homePlace = $state<Place | null>(
		homeLocation ? toPlace(homeLocation) : null,
	);
	let additionalPlaces = $state<Place[]>(otherLocations.map(toPlace));
	let newPlaceLabel = $state("");
	let newPlaceAddress = $state("");
	let newPlaceData = $state<{ latitude: number; longitude: number; google_place_id?: string } | null>(null);

	// Update canContinue and register data whenever places change
	$effect(() => {
		setCanContinue(!!homePlace);
		registerStepData({ homePlace, additionalPlaces });
	});

	function handleHomeSelect(place: {
		formatted_address: string;
		latitude: number;
		longitude: number;
		google_place_id?: string;
	}) {
		homeAddress = place.formatted_address;
		homePlace = {
			label: "Home",
			address: place.formatted_address,
			latitude: place.latitude,
			longitude: place.longitude,
			google_place_id: place.google_place_id,
		};
	}

	function handleNewPlaceSelect(place: {
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
	}

	function addPlace() {
		if (!newPlaceLabel.trim() || !newPlaceAddress.trim()) return;

		additionalPlaces = [
			...additionalPlaces,
			{
				label: newPlaceLabel.trim(),
				address: newPlaceAddress.trim(),
				latitude: newPlaceData?.latitude,
				longitude: newPlaceData?.longitude,
				google_place_id: newPlaceData?.google_place_id,
			},
		];
		newPlaceLabel = "";
		newPlaceAddress = "";
		newPlaceData = null;
	}

	function removePlace(index: number) {
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
					value={homeAddress}
					placeholder="Start typing your address..."
					onSelect={handleHomeSelect}
					success={!!homePlace}
					class="w-full p-3 bg-surface border rounded-lg text-foreground placeholder:text-foreground-subtle focus:outline-none transition-colors {homePlace ? 'border-green-500' : 'border-border focus:border-foreground'}"
				/>
			</label>
		</div>
	</section>

	<section class="mt-8">
		<h2>Other Places</h2>
		<p class="mt-2 text-foreground-muted">
			Optionally add other significant places like your workplace, gym, or
			favorite coffee shop.
		</p>

		{#if additionalPlaces.length > 0}
			<div class="flex flex-col gap-3 mt-4">
				{#each additionalPlaces as place, index}
					<div
						class="flex items-center justify-between p-3 bg-surface border border-border rounded-lg"
					>
						<div class="flex flex-col gap-0.5">
							<span class="text-sm font-medium text-foreground"
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
				<label class="flex flex-col gap-2 w-32">
					<span class="text-sm text-foreground-subtle">Label</span>
					<input
						type="text"
						class="w-full p-3 bg-surface border border-border rounded-lg text-foreground placeholder:text-foreground-subtle focus:outline-none focus:border-foreground transition-colors"
						placeholder="e.g., Work"
						bind:value={newPlaceLabel}
					/>
				</label>
				<label class="flex flex-col gap-2 flex-1">
					<span class="text-sm text-foreground-subtle">Address</span>
					<PlacesAutocomplete
						value={newPlaceAddress}
						placeholder="Start typing an address..."
						onSelect={handleNewPlaceSelect}
						success={!!newPlaceData}
						class="w-full p-3 bg-surface border rounded-lg text-foreground placeholder:text-foreground-subtle focus:outline-none transition-colors {newPlaceData ? 'border-green-500' : 'border-border focus:border-foreground'}"
					/>
				</label>
			</div>
			<button
				type="button"
				onclick={addPlace}
				disabled={!newPlaceLabel.trim() || !newPlaceData}
				class="self-start px-4 py-2 text-sm font-medium rounded-lg bg-surface-elevated text-foreground hover:bg-accent/10 transition-colors cursor-pointer disabled:opacity-50 disabled:cursor-not-allowed"
			>
				+ Add Place
			</button>
		</div>
	</section>
</div>
