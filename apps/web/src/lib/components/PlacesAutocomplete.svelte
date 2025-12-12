<script lang="ts">
	import { browser } from "$app/environment";
	import { Input } from "$lib";

	interface PlaceResult {
		formatted_address: string;
		latitude: number;
		longitude: number;
		google_place_id?: string;
	}

	interface Prediction {
		place_id: string;
		description: string;
		main_text: string;
		secondary_text: string;
	}

	let {
		value = $bindable(""),
		placeholder = "Enter an address...",
		onSelect,
		disabled = false,
		success = false,
		class: className = "",
	} = $props<{
		value?: string;
		placeholder?: string;
		onSelect: (place: PlaceResult) => void;
		disabled?: boolean;
		success?: boolean;
		class?: string;
	}>();

	let predictions = $state<Prediction[]>([]);
	let isLoading = $state(false);
	let showDropdown = $state(false);
	let selectedIndex = $state(-1);
	let sessionToken = $state(crypto.randomUUID());
	let debounceTimer: ReturnType<typeof setTimeout>;

	// Debounced search
	function handleInput() {
		clearTimeout(debounceTimer);
		selectedIndex = -1;

		if (!value || String(value).length < 3) {
			predictions = [];
			showDropdown = false;
			return;
		}

		debounceTimer = setTimeout(() => {
			fetchPredictions(String(value));
		}, 300);
	}

	async function fetchPredictions(query: string) {
		if (!browser) return;

		isLoading = true;
		try {
			const params = new URLSearchParams({
				query,
				session_token: sessionToken,
			});

			const response = await fetch(`/api/places/autocomplete?${params}`);
			if (!response.ok) {
				throw new Error("Failed to fetch predictions");
			}

			const data = await response.json();
			predictions = data.predictions || [];
			showDropdown = predictions.length > 0;
		} catch (e) {
			console.error("Places autocomplete error:", e);
			predictions = [];
		} finally {
			isLoading = false;
		}
	}

	async function selectPrediction(prediction: Prediction) {
		isLoading = true;
		showDropdown = false;
		value = prediction.description;

		try {
			const params = new URLSearchParams({
				place_id: prediction.place_id,
				session_token: sessionToken,
			});

			const response = await fetch(`/api/places/details?${params}`);
			if (!response.ok) {
				throw new Error("Failed to fetch place details");
			}

			const data = await response.json();

			const result: PlaceResult = {
				formatted_address: data.formatted_address,
				latitude: data.latitude,
				longitude: data.longitude,
				google_place_id: data.place_id,
			};

			onSelect(result);

			// Generate new session token for next search
			sessionToken = crypto.randomUUID();
		} catch (e) {
			console.error("Place details error:", e);
		} finally {
			isLoading = false;
		}
	}

	function handleKeydown(event: KeyboardEvent) {
		if (!showDropdown || predictions.length === 0) return;

		switch (event.key) {
			case "ArrowDown":
				event.preventDefault();
				selectedIndex = Math.min(
					selectedIndex + 1,
					predictions.length - 1,
				);
				break;
			case "ArrowUp":
				event.preventDefault();
				selectedIndex = Math.max(selectedIndex - 1, 0);
				break;
			case "Enter":
				event.preventDefault();
				if (selectedIndex >= 0 && selectedIndex < predictions.length) {
					selectPrediction(predictions[selectedIndex]);
				}
				break;
			case "Escape":
				showDropdown = false;
				selectedIndex = -1;
				break;
		}
	}

	function handleBlur() {
		// Delay to allow click on dropdown item
		setTimeout(() => {
			showDropdown = false;
			selectedIndex = -1;
		}, 200);
	}

	function handleFocus() {
		if (predictions.length > 0) {
			showDropdown = true;
		}
	}
</script>

<div class="places-autocomplete {className}">
	<Input
		type="text"
		bind:value
		{placeholder}
		{disabled}
		{success}
		loading={isLoading}
		autocomplete="off"
		oninput={handleInput}
		onkeydown={handleKeydown}
		onblur={handleBlur}
		onfocus={handleFocus}
	/>

	{#if showDropdown && predictions.length > 0}
		<ul class="dropdown" role="listbox">
			{#each predictions as prediction, index}
				<li
					role="option"
					aria-selected={index === selectedIndex}
					class="dropdown-item"
					class:selected={index === selectedIndex}
					onmousedown={() => selectPrediction(prediction)}
				>
					<div class="main-text">{prediction.main_text}</div>
					{#if prediction.secondary_text}
						<div class="secondary-text">
							{prediction.secondary_text}
						</div>
					{/if}
				</li>
			{/each}
		</ul>
	{/if}
</div>

<style>
	.places-autocomplete {
		position: relative;
		width: 100%;
	}

	.dropdown {
		position: absolute;
		z-index: 50;
		width: 100%;
		margin-top: 4px;
		background: var(--color-surface);
		border: 1px solid var(--color-border);
		border-radius: 8px;
		box-shadow:
			0 4px 6px -1px rgb(0 0 0 / 0.1),
			0 2px 4px -2px rgb(0 0 0 / 0.1);
		max-height: 240px;
		overflow: auto;
		list-style: none;
		padding: 4px;
		margin: 4px 0 0 0;
	}

	.dropdown-item {
		padding: 8px 12px;
		cursor: pointer;
		border-radius: 6px;
		transition: background-color 0.15s ease;
	}

	.dropdown-item:hover,
	.dropdown-item.selected {
		background: var(--color-surface-elevated);
	}

	.main-text {
		font-family: var(--font-sans);
		font-size: 14px;
		font-weight: 500;
		color: var(--color-foreground);
	}

	.secondary-text {
		font-family: var(--font-sans);
		font-size: 12px;
		color: var(--color-foreground-subtle);
		margin-top: 2px;
	}
</style>
