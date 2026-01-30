<script lang="ts">
	import type { FallbackView } from '$lib/tabs/types';
	import { spaceStore } from '$lib/stores/space.svelte';
	import Icon from '$lib/components/Icon.svelte';

	let saving = $state(false);

	async function selectOption(option: FallbackView) {
		saving = true;
		
		// Save to localStorage via store
		spaceStore.setFallbackPreference(option);
		
		// Also save to backend profile for persistence across devices
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
						fallbackView: option
					}
				})
			});
		} catch (e) {
			console.warn('[DiscoveryPage] Failed to save preference to backend:', e);
		}

		saving = false;

		// Open the selected view (unless 'empty' which keeps this page)
		if (option === 'chat') {
			spaceStore.openTabFromRoute('/');
		} else if (option === 'conway') {
			spaceStore.openTabFromRoute('/life');
		} else if (option === 'dog-jump') {
			spaceStore.openTabFromRoute('/jump');
		} else if (option === 'wiki-today') {
			const today = new Date().toISOString().split('T')[0];
			spaceStore.openTabFromRoute(`/wiki/${today}`);
		}
	}

	const options: { id: FallbackView; icon: string; title: string }[] = [
		{ id: 'chat', icon: 'ri:chat-1-line', title: 'New Chat' },
		{ id: 'conway', icon: 'ri:seedling-line', title: 'Zen Garden' },
		{ id: 'dog-jump', icon: 'ri:run-line', title: 'Dog Jump' },
		{ id: 'wiki-today', icon: 'ri:calendar-line', title: 'Today' },
		{ id: 'empty', icon: 'ri:checkbox-blank-line', title: 'Empty' }
	];
</script>

<div class="discovery-page">
	<div class="content">
		<p class="subtitle">Set your default homepage</p>

		<div class="options-grid">
			{#each options as option}
				<button
					class="option-card"
					onclick={() => selectOption(option.id)}
					disabled={saving}
				>
					<Icon icon={option.icon} width="18"/>
					<span>{option.title}</span>
				</button>
			{/each}
		</div>
	</div>
</div>

<style>
	.discovery-page {
		display: flex;
		align-items: center;
		justify-content: center;
		height: 100%;
		padding: 2rem;
		background: var(--color-background);
	}

	.content {
		text-align: center;
	}

	.subtitle {
		color: var(--color-foreground-muted);
		font-size: 0.85rem;
		margin-bottom: 1rem;
	}

	.options-grid {
		display: grid;
		grid-template-columns: repeat(2, 1fr);
		gap: 0.5rem;
	}

	.option-card {
		display: flex;
		align-items: center;
		gap: 0.5rem;
		padding: 0.6rem 1rem;
		border: 1px solid var(--color-border);
		border-radius: 8px;
		background: var(--color-surface);
		cursor: pointer;
		transition: all 0.15s ease;
		font-size: 0.85rem;
		color: var(--color-foreground-muted);
	}

	.option-card:hover {
		border-color: var(--color-primary);
		color: var(--color-foreground);
	}

	.option-card:disabled {
		opacity: 0.5;
		cursor: not-allowed;
	}
</style>
