<script lang="ts">
	import Button from '$lib/components/Button.svelte';
	import Modal from '$lib/components/Modal.svelte';
	import { appletsStore } from '$lib/stores/applets.svelte';
	import { deleteApplet, getAppletData, type Applet, type AppletData } from '$lib/api/client';
	import { KIND_WORD, startsItself } from '$lib/applets/kind';
	import { skills } from '$lib/stores/skills.svelte';

	/**
	 * Deleting an applet, from its page's More menu. Rare and the one thing
	 * here that can't be undone, so it asks, says what goes with it, and
	 * offers to keep the data.
	 */
	let {
		action,
		open = $bindable(false),
		onDeleted
	}: {
		action: Applet;
		open?: boolean;
		onDeleted: () => void;
	} = $props();

	let err = $state<string | null>(null);
	let deleteData = $state<AppletData | null>(null);
	let dropData = $state(false);
	let deleting = $state(false);

	// Loads the applet's owned tables so the user can decide whether to also
	// drop its data (default: keep, since data outlives the applet).
	$effect(() => {
		if (!open) return;
		dropData = false;
		deleteData = null;
		err = null;
		// A failed read leaves out the offer to delete its data, and the
		// data is kept, which is the default anyway.
		void getAppletData(action.id)
			.then((d) => (deleteData = d))
			.catch((e) => console.error('Reading what data it keeps failed:', e));
		// Whether `/` starts it, for the sentence below.
		void skills.load();
	});

	async function doDelete() {
		deleting = true;
		err = null;
		try {
			await deleteApplet(action.id, dropData);
			open = false;
			void appletsStore.load();
			onDeleted();
		} catch (e) {
			err = e instanceof Error ? e.message : String(e);
		} finally {
			deleting = false;
		}
	}

	// The dialog deletes a site, a skill or a task, and says which.
	const word = $derived(KIND_WORD[action.kind].toLowerCase());

	// What goes with it besides itself: the name `/` starts it by, and the
	// refresh of a site that keeps itself current.
	const alsoGoes = $derived.by(() => {
		const parts: string[] = [];
		const slash = skills.items.find((s) => s.applet_id === action.id)?.name;
		if (slash) parts.push(`/${slash} leaves the / menu`);
		if (action.kind === 'site' && startsItself(action)) parts.push('its refresh stops');
		if (parts.length === 0) return null;
		const said = parts.join(' and ');
		return `${said.charAt(0).toUpperCase()}${said.slice(1)}.`;
	});
</script>

<Modal {open} onClose={() => (open = false)} title="Delete {word}" width="sm">
	<div class="del">
		<p>Delete <strong>{action.name}</strong>? This removes the {word} and can't be undone.</p>
		{#if alsoGoes}<p>{alsoGoes}</p>{/if}
		{#if deleteData && deleteData.tables.length > 0}
			<label class="drop-opt">
				<input type="checkbox" bind:checked={dropData} />
				<span>
					Also permanently delete its data
					<span class="dim"
						>({deleteData.tables.length}
						{deleteData.tables.length === 1 ? 'table' : 'tables'} of its own)</span
					>
				</span>
			</label>
			<ul class="tbl-list">
				{#each deleteData.tables as t (t)}
					<li><code>{t}</code></li>
				{/each}
			</ul>
			{#if !dropData}
				<p class="dim">Your server keeps its data, which can outlive the {word}.</p>
			{/if}
		{/if}
		{#if err}
			<p class="error-msg">{err}</p>
		{/if}
	</div>
	{#snippet footer()}
		<Button variant="ghost" onclick={() => (open = false)} disabled={deleting}>Cancel</Button>
		<Button variant="danger" onclick={doDelete} disabled={deleting}>
			{deleting ? 'Deleting…' : dropData ? `Delete ${word} and data` : `Delete ${word}`}
		</Button>
	{/snippet}
</Modal>

<style>
	.del {
		display: flex;
		flex-direction: column;
		gap: 12px;
		font-size: 14px;
	}
	.del p {
		margin: 0;
	}
	.drop-opt {
		display: flex;
		align-items: flex-start;
		gap: 8px;
		cursor: pointer;
	}
	.tbl-list {
		margin: 0;
		padding-left: 24px;
		max-height: 8rem;
		overflow-y: auto;
	}
	.del code {
		font-size: 13px;
	}
	.dim {
		color: var(--color-foreground-subtle);
	}
	.error-msg {
		margin: 0;
		font-size: 13px;
		color: var(--color-error);
	}
</style>
