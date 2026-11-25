<script lang="ts">
	import { usePageWidth } from '$lib/stores/pageWidth.svelte';
	import { DEFAULT_ASSISTANT_NAME } from '$lib/settings';
	/**
	 * Settings' landing page: the title, the search field, a site map with
	 * a button for every page, one line each for your assistant, your account
	 * and your server (`LANDING` in lib/settings), plus this device where it
	 * has a page of its own (`deviceLanding`), and what changed lately. It
	 * fits on one screen. Each line's name is a title; its pages are its
	 * links.
	 *
	 * Typing lists the matching settings from every page in place of the
	 * rest, grouped by the page each lives on. ↑ and ↓ pick one and Return
	 * opens it on its page. Return with none picked asks the settings agent
	 * (DraftCard), which answers with a card. Editing the text closes the
	 * card; Esc closes it, then clears the field.
	 */
	import { onMount } from 'svelte';
	import ChangedLately from './ChangedLately.svelte';
	import DraftCard from './DraftCard.svelte';
	import SearchField from '$lib/components/SearchField.svelte';
	import { getAssistantProfile, listDevices } from '$lib/api/client';
	import {
		LANDING,
		deviceLanding,
		matchSettings,
		settingPlace,
		titleOf,
		type DeviceHere,
		type LandingGroup,
		type LandingLink,
		type Setting,
	} from '$lib/settings';
	import { isIOS, isMacOS, isTauri } from '$lib/utils/platform';
	import { openSetting, openSettingsPage } from '$lib/settings/focus';
	import { windowShellStore } from '$lib/stores/window-shell.svelte';

	let name = $state(DEFAULT_ASSISTANT_NAME);

	/** The device this page is open on, once known. Null on the server's own
	 *  console, which has no line: Your server covers that machine. */
	let here = $state<DeviceHere | null>(null);

	onMount(() => {
		void getAssistantProfile<{ assistant_name?: string | null }>()
			.then((p) => (name = p.assistant_name?.trim() || DEFAULT_ASSISTANT_NAME))
			.catch(() => {
				// The default name stands in; nothing on this page saves it.
			});
		if (isIOS) here = 'ios-app';
		else if (isTauri) here = isMacOS ? 'mac-app' : 'desktop-app';
		else {
			// A browser is a device only on a paired computer, where it counts
			// as that computer's app. On the console it is the server itself.
			void listDevices<{ devices: { kind: string; is_current: boolean }[] }>()
				.then((r) => {
					const current = r.devices.find((d) => d.is_current);
					here = current && current.kind === 'desktop_app' ? 'browser' : null;
				})
				.catch(() => {
					// absent-ok: without the list, the line stays off; This
					// device is still listed on Devices.
				});
		}
	});

	const siteMap: readonly LandingGroup[] = $derived(here ? [...LANDING, deviceLanding(here)] : LANDING);
	const searchPlatform = $derived(here === 'ios-app' ? 'ios' : here === 'mac-app' || here === 'browser' ? 'mac' : null);

	function openLink(link: LandingLink) {
		if (link.setting) openSetting(link.setting, spoken);
		// Sources has its own page outside Settings, named after itself.
		else if (!link.route.startsWith('/virtues')) windowShellStore.navigate(link.route, { label: link.label });
		else openSettingsPage(link.route, spoken);
	}

	const spoken = $derived(name.charAt(0).toUpperCase() + name.slice(1));

	// ─── Search ────────────────────────────────────────────────────────────
	let query = $state('');
	let picked = $state(-1);
	const typing = $derived(query.trim().length > 0);

	/** Matches grouped by the page each opens on, the best match's group first. */
	const groups = $derived.by(() => {
		if (!typing) return [];
		const byPlace = new Map<string, Setting[]>();
		for (const setting of matchSettings(query, spoken, searchPlatform)) {
			const place = settingPlace(setting, spoken);
			if (!byPlace.has(place)) byPlace.set(place, []);
			byPlace.get(place)!.push(setting);
		}
		return [...byPlace].map(([title, settings]) => ({ title, settings }));
	});
	const results = $derived(groups.flatMap((g) => g.settings));

	$effect(() => {
		void query;
		picked = -1;
	});

	function open(setting: Setting) {
		openSetting(setting.id, spoken);
	}

	// ─── Return asks the assistant ─────────────────────────────────────────
	// The text Return sent, while its draft card is open; the key gives each
	// Return a fresh card.
	let draftText = $state<string | null>(null);
	let draftKey = $state(0);
	// Bumped after a change is applied or undone, so Changed lately reloads.
	let version = $state(0);
	/** Set after an apply, so Changed lately lights its newest row. */
	let justApplied = $state(false);

	// Editing the text makes the card's answer stale, so it closes.
	$effect(() => {
		if (draftText !== null && query.trim() !== draftText) draftText = null;
	});

	function onSearchKey(e: KeyboardEvent) {
		if (e.key === 'ArrowDown' && results.length) {
			picked = Math.min(results.length - 1, picked + 1);
			e.preventDefault();
		} else if (e.key === 'ArrowUp' && results.length) {
			picked = Math.max(-1, picked - 1);
			e.preventDefault();
		} else if (e.key === 'Enter' && typing) {
			e.preventDefault();
			if (picked >= 0 && results[picked]) open(results[picked]);
			else {
				draftText = query.trim();
				draftKey += 1;
			}
		} else if (e.key === 'Escape' && (draftText || query)) {
			if (draftText) draftText = null;
			else query = '';
			e.preventDefault();
			e.stopPropagation();
		}
	}

	function draftApplied() {
		draftText = null;
		query = '';
		justApplied = true;
		version += 1;
	}

	function openFromCard(settingId: string) {
		draftText = null;
		query = '';
		openSetting(settingId, spoken);
	}

	// Settings' reading column, the width every section page shares.
	const width = usePageWidth(() => 'reading', { reading: 'var(--page-settings)' });
</script>

<div class="page" style:max-width={width.css}>
	<h1>Settings</h1>
	<!-- Once you type, Esc clears. The wrapper keeps the field its own
	     height in this column. -->
	<div class="search">
	<SearchField
		bind:value={query}
		size="lg"
		placeholder="Search settings"
		label="Search settings"
		role="combobox"
		aria-expanded={typing && !draftText}
		aria-controls={typing && !draftText ? 'settings-results' : undefined}
		aria-activedescendant={picked >= 0 ? `settings-result-${picked}` : undefined}
		aria-autocomplete="list"
		onkeydown={onSearchKey}
	>
		{#snippet trailing()}
			{#if typing}<kbd>Esc</kbd>{/if}
		{/snippet}
	</SearchField>
	</div>

	{#if typing && !draftText}
		<p class="hint">↑ ↓ to pick a setting, and Return to open it. Or write what you want or ask a question, like “keep answers short”, and Return asks {spoken}.</p>
	{/if}

	{#if draftText}
		{#key draftKey}
			<DraftCard text={draftText} assistantName={spoken} matches={results} onapplied={draftApplied} ondismiss={() => (draftText = null)} onopen={openFromCard} />
		{/key}
	{:else if typing}
		<div id="settings-results" class="results" role="listbox" aria-label="Matching settings">
			{#each groups as group, gi (group.title)}
				<div role="group" aria-labelledby="settings-group-{gi}">
					<p class="label" id="settings-group-{gi}">{group.title}</p>
					{#each group.settings as setting (setting.id)}
						{@const index = results.indexOf(setting)}
						<button
							type="button"
							class="result"
							class:picked={index === picked}
							id="settings-result-{index}"
							role="option"
							aria-selected={index === picked}
							tabindex="-1"
							onmouseenter={() => (picked = index)}
							onclick={() => open(setting)}
						>
							<span>{titleOf(setting.title, spoken)}</span>
							<span class="go" aria-hidden="true">
								<svg viewBox="0 0 24 24" width="15" height="15"><path fill="none" stroke="currentColor" stroke-linecap="round" stroke-linejoin="round" stroke-width="1.8" d="m9 18l6-6l-6-6" /></svg>
							</span>
						</button>
					{/each}
				</div>
			{:else}
				<p class="nothing">No setting matches “{query.trim()}”. Press Return to ask {spoken}.</p>
			{/each}
		</div>
	{/if}

	{#if !typing && !draftText}
		<div class="map">
			{#each siteMap as group (group.title)}
				<nav aria-label={group.title}>
					<h2>{group.title}</h2>
					<div class="buttons">
						{#each group.links as link (link.label)}
							<a
								href={link.route}
								onclick={(e) => {
									e.preventDefault();
									openLink(link);
								}}
							>{link.label}</a>
						{/each}
					</div>
				</nav>
			{/each}
		</div>

		{#key version}<ChangedLately assistantName={spoken} lit={justApplied} />{/key}
	{/if}
</div>

<style>
	.page {
		box-sizing: border-box;
		margin: 0 auto;
		padding: 44px 32px 32px;
	}

	h1 {
		font-size: 2rem;
		font-weight: 400;
		line-height: 1.2;
		margin: 0 0 12px;
	}

	.search {
		display: flex;
	}

	kbd {
		font-family: inherit;
		font-size: 12px;
		padding: 0 6px;
		border: 1px solid var(--color-border);
		border-radius: var(--radius-sm);
		color: var(--color-foreground-muted);
	}

	.hint {
		margin: 12px 0 0;
		font-size: 13px;
		line-height: 1.5;
		color: var(--color-foreground-muted);
	}

	.label {
		margin: 0 0 4px;
		font-size: 12px;
		font-weight: 400;
		color: var(--color-foreground-muted);
	}

	.results {
		margin-top: 28px;
	}

	.results [role='group'] + [role='group'] .label {
		margin-top: 24px;
	}

	.result {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 24px;
		width: calc(100% + 24px);
		min-height: 48px;
		margin: 0 -12px;
		padding: 0 12px;
		border: none;
		border-radius: var(--radius-md);
		background: none;
		font: inherit;
		font-size: 15px;
		text-align: left;
		color: var(--color-foreground);
		cursor: pointer;
	}

	.result.picked {
		background: var(--color-surface-elevated);
	}

	.go {
		display: flex;
		color: var(--color-foreground-muted);
	}

	.nothing {
		margin: 0;
		font-size: 14px;
		color: var(--color-foreground-muted);
	}

	.map {
		display: flex;
		flex-direction: column;
		gap: 8px;
		margin-top: 40px;
	}

	.map nav {
		display: grid;
		grid-template-columns: 104px minmax(0, 1fr);
		align-items: start;
	}

	/* An h2 for the outline, drawn as the small label the other lists use. */
	.map h2 {
		margin: 0;
		padding-top: 4px;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
		font-family: var(--font-sans);
		font-size: 12px;
		font-weight: 400;
		letter-spacing: normal;
		color: var(--color-foreground-subtle);
	}

	.buttons {
		display: flex;
		flex-wrap: wrap;
		gap: 6px;
	}

	.buttons a {
		display: inline-flex;
		align-items: center;
		height: 26px;
		padding: 0 12px;
		border-radius: var(--radius-md);
		background: var(--color-surface-elevated);
		font-size: 13px;
		line-height: 1;
		text-decoration: none;
		color: var(--color-foreground);
		transition: background var(--motion-hover-out) var(--ease-premium);
	}

	.buttons a:hover {
		background: color-mix(in srgb, var(--wash-ink) 8%, var(--color-surface-elevated));
		transition-duration: var(--motion-hover-in);
	}

	@media (max-width: 640px) {
		.map nav {
			grid-template-columns: minmax(0, 1fr);
			gap: 6px;
		}
	}

	@media (prefers-reduced-motion: reduce) {
		.buttons a {
			transition: none;
		}
	}
</style>
