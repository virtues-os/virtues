<script lang="ts">
	import { onMount } from 'svelte';
	import { Button, Page } from '$lib';
	import { getAssistantProfile } from '$lib/api/client';
	import AssistantNameSettings from '$lib/components/AssistantNameSettings.svelte';
	import StyleNotesSettings from '$lib/components/StyleNotesSettings.svelte';
	import RulesSettings from '$lib/components/RulesSettings.svelte';
	import MemorySettings from '$lib/components/MemorySettings.svelte';
	import ModelSettings from '$lib/components/ModelSettings.svelte';

	// The profile is read once here: the name feeds every section's copy, so
	// a rename shows in all of them as soon as the server has it.
	let name = $state('Ari');
	let styleNotes = $state('');
	let profile = $state<'loading' | 'ready' | 'error'>('loading');

	async function load() {
		profile = 'loading';
		try {
			const p = await getAssistantProfile<{
				assistant_name?: string | null;
				style_notes?: string | null;
			}>();
			name = p.assistant_name?.trim() || 'Ari';
			styleNotes = p.style_notes ?? '';
			profile = 'ready';
		} catch {
			profile = 'error';
		}
	}

	onMount(load);

	// The copy names the assistant mid-sentence and at the start of one, so it
	// takes a capital whatever case the owner typed it in.
	const spoken = $derived(name.charAt(0).toUpperCase() + name.slice(1));
</script>

<Page
	title="Assistant"
	description="How your assistant talks to you, what it keeps in mind, and the models it runs on."
	maxWidth="wide"
>
	<div class="assistant-settings">
		{#if profile === 'error'}
			<!-- Name and notes would otherwise show defaults the server may not
			     hold, and a save from there would overwrite what it does. -->
			<div class="load-error">
				<p>Your server couldn't load your assistant's name and notes.</p>
				<div>
					<Button variant="secondary" size="sm" onclick={load}>Try again</Button>
				</div>
			</div>
		{/if}
		<AssistantNameSettings bind:name disabled={profile !== 'ready'} />
		<StyleNotesSettings
			bind:notes={styleNotes}
			assistantName={spoken}
			disabled={profile !== 'ready'}
		/>
		<RulesSettings assistantName={spoken} />
		<MemorySettings assistantName={spoken} />
		<ModelSettings assistantName={spoken} />
	</div>
</Page>

<style>
	/* Settings are forms; a field running the pane's full width is harder to
	   read, not roomier. */
	.assistant-settings {
		display: flex;
		flex-direction: column;
		gap: 32px;
		max-width: 40rem;
	}

	.load-error {
		display: flex;
		flex-direction: column;
		gap: 12px;
	}

	.load-error p {
		margin: 0;
		max-width: 60ch;
		font-size: 12px;
		color: var(--color-error);
	}
</style>
