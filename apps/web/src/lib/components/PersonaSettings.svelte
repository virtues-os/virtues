<script lang="ts">
	import { onMount } from 'svelte';
	import { personaStore, type Persona } from '$lib/stores/personas.svelte';
	import Icon from '$lib/components/Icon.svelte';
	import Modal from '$lib/components/Modal.svelte';

	// Active persona state (from assistant profile)
	let activePersonaId = $state<string | null>(null);

	// Modal state
	let modalOpen = $state(false);
	let editingPersona = $state<Persona | null>(null);
	let isCreating = $state(false);

	// Form state
	let formTitle = $state('');
	let formContent = $state('');
	let saving = $state(false);

	onMount(async () => {
		await personaStore.init();
		await loadActivePersona();
	});

	async function loadActivePersona() {
		try {
			const res = await fetch('/api/assistant-profile');
			if (res.ok) {
				const profile = await res.json();
				activePersonaId = profile.persona || null;
			}
		} catch (error) {
			console.error('Failed to load active persona:', error);
		}
	}

	async function setActivePersona(personaId: string) {
		const previousId = activePersonaId;
		activePersonaId = personaId; // Optimistic update

		try {
			const res = await fetch('/api/assistant-profile', {
				method: 'PUT',
				headers: { 'Content-Type': 'application/json' },
				body: JSON.stringify({ persona: personaId })
			});
			if (!res.ok) {
				activePersonaId = previousId; // Rollback on error
				console.error('Failed to set active persona');
			}
		} catch (error) {
			activePersonaId = previousId; // Rollback on error
			console.error('Failed to set active persona:', error);
		}
	}

	function openCreate() {
		isCreating = true;
		editingPersona = null;
		formTitle = '';
		formContent = '';
		modalOpen = true;
	}

	function openEdit(persona: Persona) {
		isCreating = false;
		editingPersona = persona;
		formTitle = persona.title;
		formContent = persona.content;
		modalOpen = true;
	}

	function closeModal() {
		modalOpen = false;
		editingPersona = null;
		isCreating = false;
		formTitle = '';
		formContent = '';
	}

	async function handleSave() {
		if (!formTitle.trim() || !formContent.trim()) return;

		saving = true;

		if (isCreating) {
			const persona = await personaStore.create(formTitle.trim(), formContent.trim());
			if (persona) closeModal();
		} else if (editingPersona) {
			const success = await personaStore.update(editingPersona.id, {
				title: formTitle.trim(),
				content: formContent.trim()
			});
			if (success) closeModal();
		}

		saving = false;
	}

	async function handleDelete() {
		if (!editingPersona) return;

		const action = editingPersona.is_system ? 'hide' : 'delete';
		if (!confirm(`Are you sure you want to ${action} "${editingPersona.title}"?`)) return;

		await personaStore.hide(editingPersona.id);
		closeModal();
	}

	async function handleReset() {
		if (confirm('Reset all personas to defaults? This removes custom personas and restores hidden ones.')) {
			await personaStore.reset();
		}
	}

	const modalTitle = $derived(isCreating ? 'New Persona' : 'Edit Persona');
	const canSave = $derived(formTitle.trim() && formContent.trim());
</script>

<div class="bg-surface border border-border rounded-lg overflow-hidden">
	<div class="flex items-center justify-between px-4 py-3 border-b border-border">
		<h2 class="text-sm font-medium text-foreground">AI Personas</h2>
		<button
			class="text-xs text-foreground-muted hover:text-foreground hover:bg-surface-elevated px-2 py-1 rounded transition-colors"
			onclick={handleReset}
		>
			Reset to defaults
		</button>
	</div>

	{#if personaStore.loading && !personaStore.initialized}
		<div class="text-center py-6 text-sm text-foreground-muted">Loading personas...</div>
	{:else if personaStore.error}
		<div class="text-center py-6 text-sm text-red-500">{personaStore.error}</div>
	{:else}
		<div class="flex flex-col divide-y divide-border">
			{#each personaStore.personas as persona (persona.id)}
				{@const isActive = activePersonaId === persona.id}
				<div class="flex items-center gap-2 hover:bg-surface-elevated transition-colors">
					<!-- Active indicator / Select button -->
					<button
						class="flex items-center justify-center w-10 h-full py-3 shrink-0 transition-colors"
						onclick={() => setActivePersona(persona.id)}
						title={isActive ? 'Active persona' : 'Set as active'}
					>
						{#if isActive}
							<Icon icon="ri:checkbox-circle-fill" width="18" class="text-primary" />
						{:else}
							<Icon icon="ri:checkbox-blank-circle-line" width="18" class="text-foreground-subtle hover:text-foreground-muted" />
						{/if}
					</button>

					<!-- Persona info (clickable to edit) -->
					<button
						class="flex-1 flex items-center justify-between py-3 pr-3 text-left"
						onclick={() => openEdit(persona)}
					>
						<div class="flex items-center gap-2">
							<span class="text-sm font-medium text-foreground">{persona.title}</span>
							{#if persona.is_system}
								<span class="text-[10px] uppercase tracking-wide px-1.5 py-0.5 rounded bg-surface-elevated text-foreground-muted">
									System
								</span>
							{:else}
								<span class="text-[10px] uppercase tracking-wide px-1.5 py-0.5 rounded bg-primary/10 text-primary">
									Custom
								</span>
							{/if}
						</div>
						<Icon icon="ri:arrow-right-s-line" width="18" class="text-foreground-subtle" />
					</button>
				</div>
			{/each}

			<!-- Add button -->
			<button
				class="flex items-center gap-2 px-4 py-3 text-sm text-primary hover:bg-surface-elevated transition-colors w-full"
				onclick={openCreate}
			>
				<Icon icon="ri:add-line" width="16" />
				<span>Add Persona</span>
			</button>
		</div>

		{#if personaStore.personas.length === 0}
			<div class="text-center py-6 text-sm text-foreground-muted">
				No personas found. Click "Add Persona" to create one.
			</div>
		{/if}
	{/if}
</div>

<Modal open={modalOpen} onClose={closeModal} title={modalTitle} width="md">
	{#snippet children()}
		<div class="space-y-4">
			<div class="flex flex-col gap-1.5">
				<label class="modal-label" for="persona-title">Name</label>
				<input
					id="persona-title"
					type="text"
					class="modal-input"
					bind:value={formTitle}
					placeholder="e.g., Creative Writer"
				/>
			</div>

			<div class="flex flex-col gap-1.5">
				<label class="modal-label" for="persona-content">
					System Prompt Guidelines
					<span class="font-normal text-foreground-subtle ml-1">(use {'{user_name}'} for personalization)</span>
				</label>
				<textarea
					id="persona-content"
					class="modal-input font-mono text-sm leading-relaxed resize-y min-h-[120px]"
					bind:value={formContent}
					placeholder="- Be creative and imaginative&#10;- Use vivid descriptions&#10;- Help {'{user_name}'} express ideas"
					rows="6"
				></textarea>
			</div>
		</div>
	{/snippet}

	{#snippet footer()}
		<div class="flex items-center justify-between w-full">
			{#if editingPersona}
				<button
					class="modal-btn border border-red-500 text-red-500 hover:bg-red-500 hover:text-white"
					onclick={handleDelete}
				>
					{editingPersona.is_system ? 'Hide' : 'Delete'}
				</button>
			{:else}
				<div></div>
			{/if}
			<div class="flex items-center gap-2">
				<button class="modal-btn modal-btn-secondary" onclick={closeModal}>
					Cancel
				</button>
				<button
					class="modal-btn modal-btn-primary"
					onclick={handleSave}
					disabled={!canSave || saving}
				>
					{saving ? 'Saving...' : 'Save'}
				</button>
			</div>
		</div>
	{/snippet}
</Modal>
