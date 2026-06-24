<script lang="ts">
	import type { Tab } from '$lib/tabs/types';
	import type { Thing } from '$lib/api/client';
	import { onMount } from 'svelte';
	import Icon from '$lib/components/Icon.svelte';
	import { thingsStore } from '$lib/stores/things.svelte';
	import { iconPickerStore } from '$lib/stores/iconPicker.svelte';

	let { tab, active: _active }: { tab: Tab; active: boolean } = $props();

	// Parse thing id from tab.route: "/thing/thg_xxx"
	const thingId = $derived.by(() => {
		const match = tab.route.match(/^\/thing\/(thg_[^/]+)$/);
		return match?.[1] ?? null;
	});

	let detail = $state<Thing | null>(null);
	let loading = $state(false);
	let error = $state<string | null>(null);

	// Local editable fields (for inline rename/description edit)
	let editingName = $state(false);
	let nameDraft = $state('');
	let editingDescription = $state(false);
	let descriptionDraft = $state('');

	async function loadDetail(force = false) {
		if (!thingId) return;
		loading = true;
		error = null;
		try {
			detail = await thingsStore.loadDetail(thingId, force);
		} catch (e) {
			console.error('[ThingDetailView] Failed to load thing:', e);
			error = e instanceof Error ? e.message : 'Failed to load thing';
			detail = null;
		} finally {
			loading = false;
		}
	}

	onMount(() => {
		loadDetail();
	});

	$effect(() => {
		// Reload when the tab switches to a different thing
		if (thingId) {
			loadDetail();
		}
	});

	function startRenameName() {
		if (!detail) return;
		nameDraft = detail.name;
		editingName = true;
	}

	async function commitRenameName() {
		if (!detail) return;
		const name = nameDraft.trim();
		if (!name || name === detail.name) {
			editingName = false;
			return;
		}
		try {
			const updated = await thingsStore.update(detail.id, { name });
			detail = { ...detail, ...updated };
		} catch (e) {
			console.error('[ThingDetailView] Failed to rename thing:', e);
		} finally {
			editingName = false;
		}
	}

	function startEditDescription() {
		if (!detail) return;
		descriptionDraft = detail.description ?? '';
		editingDescription = true;
	}

	async function commitEditDescription() {
		if (!detail) return;
		const trimmed = descriptionDraft.trim();
		const value: string | null = trimmed === '' ? null : trimmed;
		if (value === (detail.description ?? null)) {
			editingDescription = false;
			return;
		}
		try {
			const updated = await thingsStore.update(detail.id, { description: value });
			detail = { ...detail, ...updated };
		} catch (e) {
			console.error('[ThingDetailView] Failed to update description:', e);
		} finally {
			editingDescription = false;
		}
	}

	function changeProjectIcon() {
		if (!detail) return;
		iconPickerStore.show(detail.icon ?? null, async (icon) => {
			try {
				const updated = await thingsStore.update(detail!.id, { icon });
				detail = { ...detail!, ...updated };
			} catch (e) {
				console.error('[ThingDetailView] Failed to change icon:', e);
			}
		});
	}
</script>

<div class="thing-detail">
	{#if !thingId}
		<div class="status error">Invalid thing route: {tab.route}</div>
	{:else if loading && !detail}
		<div class="status">Loading…</div>
	{:else if error}
		<div class="status error">Failed to load thing: {error}</div>
	{:else if detail}
		<header class="header">
			<button type="button" class="icon-btn" onclick={changeProjectIcon} title="Change icon">
				<Icon icon={detail.icon || 'ri:folder-open-line'} width="28" />
			</button>
			<div class="title-block">
				{#if editingName}
					<!-- svelte-ignore a11y_autofocus -->
					<input
						type="text"
						class="name-input"
						bind:value={nameDraft}
						autofocus
						onkeydown={(e) => {
							if (e.key === 'Enter') commitRenameName();
							else if (e.key === 'Escape') editingName = false;
						}}
						onblur={commitRenameName}
					/>
				{:else}
					<!-- svelte-ignore a11y_click_events_have_key_events -->
					<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
					<h1 onclick={startRenameName}>{detail.name}</h1>
				{/if}

				{#if editingDescription}
					<!-- svelte-ignore a11y_autofocus -->
					<input
						type="text"
						class="description-input"
						bind:value={descriptionDraft}
						placeholder="What's this thing about?"
						autofocus
						onkeydown={(e) => {
							if (e.key === 'Enter') commitEditDescription();
							else if (e.key === 'Escape') editingDescription = false;
						}}
						onblur={commitEditDescription}
					/>
				{:else if detail.description}
					<!-- svelte-ignore a11y_click_events_have_key_events -->
					<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
					<p class="description" onclick={startEditDescription}>{detail.description}</p>
				{:else}
					<button type="button" class="description-add" onclick={startEditDescription}>
						Add description…
					</button>
				{/if}
			</div>
		</header>
	{/if}
</div>

<style>
	@reference "../../../../app.css";

	.thing-detail {
		display: flex;
		flex-direction: column;
		height: 100%;
		min-height: 0;
	}

	.header {
		display: flex;
		align-items: flex-start;
		gap: 1rem;
		padding: 2rem 2rem 1.25rem;
		border-bottom: 1px solid var(--color-border, #e5e7eb);
		flex-shrink: 0;
	}

	.icon-btn {
		width: 48px;
		height: 48px;
		display: flex;
		align-items: center;
		justify-content: center;
		border: 1px solid var(--color-border, #e5e7eb);
		border-radius: 8px;
		background: var(--color-surface, #fff);
		cursor: pointer;
		flex-shrink: 0;
	}
	.icon-btn:hover {
		background: var(--color-surface-hover, #f9fafb);
	}

	.title-block {
		flex: 1;
		min-width: 0;
		display: flex;
		flex-direction: column;
		gap: 0.25rem;
	}

	.title-block h1 {
		font-size: 1.5rem;
		font-weight: 600;
		margin: 0;
		color: var(--color-foreground, inherit);
		cursor: text;
	}

	.name-input {
		font: inherit;
		font-size: 1.5rem;
		font-weight: 600;
		border: 1px solid var(--color-border, #e5e7eb);
		border-radius: 6px;
		padding: 0.125rem 0.375rem;
		background: var(--color-surface, #fff);
		color: var(--color-foreground, inherit);
		width: 100%;
	}

	.description {
		font-size: 0.875rem;
		color: var(--color-foreground-muted, #6b7280);
		margin: 0;
		cursor: text;
	}

	.description-input {
		font: inherit;
		font-size: 0.875rem;
		border: 1px solid var(--color-border, #e5e7eb);
		border-radius: 6px;
		padding: 0.25rem 0.5rem;
		background: var(--color-surface, #fff);
		width: 100%;
	}

	.description-add {
		align-self: flex-start;
		font-size: 0.8125rem;
		color: var(--color-foreground-subtle, #9ca3af);
		background: transparent;
		border: none;
		padding: 0;
		cursor: pointer;
	}
	.description-add:hover {
		color: var(--color-foreground-muted, #6b7280);
	}

	.status {
		padding: 2rem 1rem;
		text-align: center;
		color: var(--color-foreground-muted, #6b7280);
		font-size: 0.8125rem;
		max-width: 44ch;
		margin: 0 auto;
		line-height: 1.5;
	}
	.status.error {
		color: #b91c1c;
	}
</style>
