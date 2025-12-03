<script lang="ts">
	import { onMount } from 'svelte';
	import { browser } from '$app/environment';
	import { env } from '$env/dynamic/public';

	interface PlaceResult {
		formatted_address: string;
		latitude: number;
		longitude: number;
		google_place_id?: string;
	}

	interface Props {
		value?: string;
		placeholder?: string;
		onSelect: (place: PlaceResult) => void;
		class?: string;
	}

	let { value = '', placeholder = 'Enter an address...', onSelect, class: className = '' }: Props = $props();

	const apiKey = env.PUBLIC_GOOGLE_MAPS_API_KEY || '';

	let inputElement: HTMLInputElement;
	// eslint-disable-next-line @typescript-eslint/no-explicit-any
	let autocomplete: any = null;
	let isLoaded = $state(false);

	// Helper to access google maps from window
	function getGoogleMaps(): any {
		return (window as any).google?.maps;
	}

	// Load Google Maps script
	function loadGoogleMaps(): Promise<void> {
		return new Promise((resolve, reject) => {
			if (getGoogleMaps()?.places) {
				resolve();
				return;
			}

			// Check if script is already loading
			const existingScript = document.querySelector('script[src*="maps.googleapis.com"]');
			if (existingScript) {
				existingScript.addEventListener('load', () => resolve());
				return;
			}

			const script = document.createElement('script');
			script.src = `https://maps.googleapis.com/maps/api/js?key=${apiKey}&libraries=places`;
			script.async = true;
			script.defer = true;
			script.onload = () => resolve();
			script.onerror = () => reject(new Error('Failed to load Google Maps'));
			document.head.appendChild(script);
		});
	}

	onMount(async () => {
		if (!browser || !apiKey) {
			console.warn('Google Maps API key not configured');
			return;
		}

		try {
			await loadGoogleMaps();
			isLoaded = true;

			const maps = getGoogleMaps();
			if (maps?.places) {
				autocomplete = new maps.places.Autocomplete(inputElement, {
					types: ['address'],
					fields: ['formatted_address', 'geometry', 'place_id']
				});

				// Handle place selection
				autocomplete.addListener('place_changed', () => {
					const place = autocomplete?.getPlace();
					if (place?.formatted_address && place?.geometry?.location) {
						const result: PlaceResult = {
							formatted_address: place.formatted_address,
							latitude: place.geometry.location.lat(),
							longitude: place.geometry.location.lng(),
							google_place_id: place.place_id
						};
						onSelect(result);
					}
				});
			}
		} catch (e) {
			console.error('Failed to initialize Google Places:', e);
		}
	});
</script>

<input
	bind:this={inputElement}
	type="text"
	{value}
	{placeholder}
	disabled={!isLoaded && browser && !!apiKey}
	class={className}
/>

{#if browser && !apiKey}
	<p class="text-xs text-warning mt-1">Google Maps API key not configured</p>
{/if}
