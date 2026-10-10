<script lang="ts">
	import Button from '$lib/components/Button.svelte';
	import Modal from '$lib/components/Modal.svelte';
	import { appletsStore } from '$lib/stores/applets.svelte';
	import { deleteApplet, getAppletData, type Applet, type AppletData } from '$lib/api/client';

	/**
	 * Deleting an applet, from its page's More menu. Rare and the one thing
	 * here that can't be undone, so it asks, and offers to keep the data.
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
		void getAppletData(action.id).then((d) => (deleteData = d));
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
</script>

<Modal {open} onClose={() => (open = false)} title="Delete applet" width="sm">
	<div class="del">
		<p>Delete <strong>{action.name}</strong>? This removes the applet and can't be undone.</p>
		{#if deleteData && deleteData.tables.length > 0}
			<label class="drop-opt">
				<input type="checkbox" bind:checked={dropData} />
				<span>
					Also permanently delete its data
					<span class="dim"
						>({deleteData.tables.length}
						{deleteData.tables.length === 1 ? 'table' : 'tables'} in
						<code>{deleteData.schema}</code>)</span
					>
				</span>
			</label>
			<ul class="tbl-list">
				{#each deleteData.tables as t (t)}
					<li><code>{t}</code></li>
				{/each}
			</ul>
			{#if !dropData}
				<p class="dim">Your server keeps its data, which can outlive the applet.</p>
			{/if}
		{/if}
		{#if err}
			<p class="error-msg">{err}</p>
		{/if}
	</div>
	{#snippet footer()}
		<Button variant="ghost" onclick={() => (open = false)} disabled={deleting}>Cancel</Button>
		<Button variant="danger" onclick={doDelete} disabled={deleting}>
			{deleting ? 'Deleting…' : dropData ? 'Delete applet and data' : 'Delete applet'}
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
