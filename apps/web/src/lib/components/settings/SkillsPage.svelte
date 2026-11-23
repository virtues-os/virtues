<script lang="ts">
	/**
	 * Settings · Skills (/virtues/assistant/skills): what your assistant knows
	 * how to do the same way each time.
	 *
	 * A skill has no door of its own. The / menu in a chat starts one, and the
	 * assistant picks one up when a request calls for it. This page is where
	 * they are all listed: the ones you made, which you can change or delete
	 * from their page, and the ones that came with your server, which you can
	 * turn off. A row opens the skill's page. It is the same list as the /
	 * menu, in the same order: a row says the `/name` that starts it, and one
	 * turned off is not in the menu. The page also lists the skills only the
	 * assistant picks up, which `/` does not start.
	 */
	import { onMount } from 'svelte';
	import SettingsPage from './SettingsPage.svelte';
	import LinkRow from './LinkRow.svelte';
	import LoadError from './LoadError.svelte';
	import type { Applet } from '$lib/api/client';
	import { appletsStore } from '$lib/stores/applets.svelte';
	import { skills as menu } from '$lib/stores/skills.svelte';
	import { isYours, pageOf } from '$lib/applets/kind';

	let loaded = $state(false);

	async function load() {
		await Promise.all([appletsStore.load(), menu.load()]);
		loaded = true;
	}

	// What `/` types to start each one, from the menu.
	const slashName = $derived(new Map(menu.items.flatMap((s) => (s.applet_id ? [[s.applet_id, s.name] as const] : []))));

	/** A row's value: off, or the name `/` starts it by. */
	function valueOf(a: Applet): string | undefined {
		if (!a.enabled) return 'Off';
		const slash = slashName.get(a.id);
		return slash ? `/${slash}` : undefined;
	}

	onMount(() => {
		void load();
	});

	const skills = $derived(
		appletsStore.list.filter((a) => !a.archived_at && a.kind === 'skill').sort((a, b) => a.name.localeCompare(b.name))
	);
	const yours = $derived(skills.filter(isYours));
	const shipped = $derived(skills.filter((a) => !isYours(a)));
	const failed = $derived(loaded && !!appletsStore.error && appletsStore.list.length === 0);
</script>

{#snippet rows(list: Applet[])}
	{#each list as a (a.id)}
		<LinkRow
			id={a.id}
			label={a.name}
			help={a.description ?? undefined}
			href={pageOf(a)}
			value={valueOf(a)}
			tabLabel={a.name}
		/>
	{/each}
{/snippet}

<SettingsPage
	title="Skills"
	lede="What your assistant knows how to do the same way each time. Type / in a chat to start one."
	back={{ route: '/virtues/assistant/tools', label: 'What it can do' }}
>
	{#if failed}
		<LoadError message="Your server couldn’t send its skills." onretry={load} />
	{:else if !loaded}
		<p class="settings-sub">Asking your server about its skills…</p>
	{:else}
		<section class="group">
			<h2 class="settings-label">Yours</h2>
			{#if yours.length}
				{@render rows(yours)}
			{:else}
				<p class="group-sub">
					You have made none yet. Ask for one in a chat, such as “a packing list I can start whenever I travel”.
				</p>
			{/if}
		</section>

		{#if shipped.length}
			<section class="group">
				<h2 class="settings-label">Came with your server</h2>
				{@render rows(shipped)}
			</section>
		{/if}
	{/if}
</SettingsPage>

<style>
	.group {
		padding: 24px 0 8px;
	}
	.group-sub {
		margin: 0;
		max-width: 60ch;
		font-size: 13px;
		line-height: 1.5;
		color: var(--color-foreground-muted);
	}
</style>
