<script lang="ts">
	import { browser } from '$app/environment';

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

	interface Props {
		value?: string;
		placeholder?: string;
		onSelect: (place: PlaceResult) => void;
		class?: string;
		success?: boolean;
	}

	let { value = $bindable(''), placeholder = 'Enter an address...', onSelect, class: className = '', success = false }: Props = $props();

	let predictions = $state<Prediction[]>([]);
	let isLoading = $state(false);
	let showDropdown = $state(false);
	let selectedIndex = $state(-1);
	let inputElement: HTMLInputElement;
	let sessionToken = $state(crypto.randomUUID());
	let debounceTimer: ReturnType<typeof setTimeout>;

	// Debounced search
	function handleInput() {
		clearTimeout(debounceTimer);
		selectedIndex = -1;

		if (!value || value.length < 3) {
			predictions = [];
			showDropdown = false;
			return;
		}

		debounceTimer = setTimeout(() => {
			fetchPredictions(value);
		}, 300);
	}

	async function fetchPredictions(query: string) {
		if (!browser) return;

		isLoading = true;
		try {
			const params = new URLSearchParams({
				query,
				session_token: sessionToken
			});

			const response = await fetch(`/api/places/autocomplete?${params}`);
			if (!response.ok) {
				throw new Error('Failed to fetch predictions');
			}

			const data = await response.json();
			predictions = data.predictions || [];
			showDropdown = predictions.length > 0;
		} catch (e) {
			console.error('Places autocomplete error:', e);
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
				session_token: sessionToken
			});

			const response = await fetch(`/api/places/details?${params}`);
			if (!response.ok) {
				throw new Error('Failed to fetch place details');
			}

			const data = await response.json();

			const result: PlaceResult = {
				formatted_address: data.formatted_address,
				latitude: data.latitude,
				longitude: data.longitude,
				google_place_id: data.place_id
			};

			onSelect(result);

			// Generate new session token for next search
			sessionToken = crypto.randomUUID();
		} catch (e) {
			console.error('Place details error:', e);
		} finally {
			isLoading = false;
		}
	}

	function handleKeydown(event: KeyboardEvent) {
		if (!showDropdown || predictions.length === 0) return;

		switch (event.key) {
			case 'ArrowDown':
				event.preventDefault();
				selectedIndex = Math.min(selectedIndex + 1, predictions.length - 1);
				break;
			case 'ArrowUp':
				event.preventDefault();
				selectedIndex = Math.max(selectedIndex - 1, 0);
				break;
			case 'Enter':
				event.preventDefault();
				if (selectedIndex >= 0 && selectedIndex < predictions.length) {
					selectPrediction(predictions[selectedIndex]);
				}
				break;
			case 'Escape':
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

<div class="relative w-full">
	<input
		bind:this={inputElement}
		bind:value
		type="text"
		{placeholder}
		class={className}
		oninput={handleInput}
		onkeydown={handleKeydown}
		onblur={handleBlur}
		onfocus={handleFocus}
		autocomplete="off"
		role="combobox"
		aria-expanded={showDropdown}
		aria-autocomplete="list"
		aria-controls="places-listbox"
	/>

	{#if isLoading}
		<div class="absolute right-3 top-1/2 -translate-y-1/2">
			<div class="w-4 h-4 border-2 border-muted border-t-primary rounded-full animate-spin"></div>
		</div>
	{:else if success}
		<div class="absolute right-3 top-1/2 -translate-y-1/2 text-green-500">
			<svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
				<path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M5 13l4 4L19 7" />
			</svg>
		</div>
	{/if}

	{#if showDropdown && predictions.length > 0}
		<ul
			id="places-listbox"
			role="listbox"
			class="absolute z-50 w-full mt-1 bg-background border border-border rounded-md shadow-lg max-h-60 overflow-auto"
		>
			{#each predictions as prediction, index}
				<li
					role="option"
					aria-selected={index === selectedIndex}
					class="px-3 py-2 cursor-pointer hover:bg-muted transition-colors {index === selectedIndex ? 'bg-muted' : ''}"
					onmousedown={() => selectPrediction(prediction)}
				>
					<div class="font-medium text-sm">{prediction.main_text}</div>
					{#if prediction.secondary_text}
						<div class="text-xs text-muted-foreground">{prediction.secondary_text}</div>
					{/if}
				</li>
			{/each}
		</ul>
	{/if}
</div>
