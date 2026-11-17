<script lang="ts">
	/**
	 * Settings' landing page (agents/plan/settings-search-plan.md): the
	 * question, the search field, what needs you and what changed lately,
	 * then a link to every page (`LANDING` in lib/settings).
	 *
	 * Typing lists the matching settings from every page in place of the
	 * rest, grouped by the page each lives on. ↑ and ↓ pick one and Return
	 * opens it on its page. Return with none picked asks the assistant to
	 * draft the change (DraftCard); Esc closes the draft, then clears the
	 * field.
	 */
	import { onMount, tick } from 'svelte';
	import { reducedMotion } from '$lib/stores/motion.svelte';
	import ReopenSection from './ReopenSection.svelte';
	import IntroductionSection from './IntroductionSection.svelte';
	import ChangedLately from './ChangedLately.svelte';
	import DraftCard from './DraftCard.svelte';
	import { getAssistantProfile } from '$lib/api/client';
	import { isAppleKeyboard } from '$lib/utils/platform';
	import { LANDING, SECTIONS, matchSettings, titleOf, type Setting } from '$lib/settings';
	import { openSetting, openSettingsPage } from '$lib/settings/focus';

	let name = $state('Ari');

	onMount(async () => {
		try {
			const p = await getAssistantProfile<{ assistant_name?: string | null }>();
			name = p.assistant_name?.trim() || 'Ari';
		} catch {
			// The default name stands in; nothing on this page saves it.
		}
	});

	const spoken = $derived(name.charAt(0).toUpperCase() + name.slice(1));

	// ─── Search ────────────────────────────────────────────────────────────
	let query = $state('');
	let picked = $state(-1);
	const typing = $derived(query.trim().length > 0);

	/** Matches grouped by the section they live in, the best match's group first. */
	const groups = $derived.by(() => {
		if (!typing) return [];
		const order: string[] = [];
		const bySection = new Map<string, Setting[]>();
		for (const setting of matchSettings(query, spoken)) {
			if (!bySection.has(setting.section)) {
				bySection.set(setting.section, []);
				order.push(setting.section);
			}
			bySection.get(setting.section)!.push(setting);
		}
		return order.map((sectionId) => ({
			title: titleOf(SECTIONS.find((s) => s.id === sectionId)?.title ?? '', spoken),
			settings: bySection.get(sectionId)!,
		}));
	});
	const results = $derived(groups.flatMap((g) => g.settings));

	$effect(() => {
		void query;
		picked = -1;
	});

	function open(setting: Setting) {
		// Start over is on this page, at its foot.
		if (setting.route === '/virtues') {
			query = '';
			void tick().then(() =>
				document.getElementById(`setting-${setting.id}`)?.scrollIntoView({ behavior: reducedMotion() ? 'auto' : 'smooth', block: 'center' }),
			);
			return;
		}
		openSetting(setting.id, spoken);
	}

	// ─── Return asks the assistant ─────────────────────────────────────────
	// The text Return sent, while its draft card is open; the key gives each
	// Return a fresh card.
	let draftText = $state<string | null>(null);
	let draftKey = $state(0);
	// Bumped after a change is applied or undone, so Changed lately reloads.
	let version = $state(0);

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
		version += 1;
	}

	function editFirst(settingId: string) {
		draftText = null;
		query = '';
		openSetting(settingId, spoken);
	}
</script>

<div class="page">
	<h1>What do you want to change?</h1>
	<label class="search" class:typing>
		<svg viewBox="0 0 24 24" aria-hidden="true" width="20" height="20">
			<path fill="none" stroke="currentColor" stroke-linecap="round" stroke-width="1.6" d="M19 11.5a7.5 7.5 0 1 1-15 0a7.5 7.5 0 0 1 15 0m-2.107 5.42l3.08 3.08" />
		</svg>
		<input
			type="search"
			bind:value={query}
			placeholder="Find a setting, or say what you want changed"
			aria-label="Search settings"
			aria-controls="settings-results"
			autocomplete="off"
			spellcheck="false"
			onkeydown={onSearchKey}
		/>
		<!-- ⌘ , opens Settings from anywhere (the shortcut lives with the
		     others in UnifiedSidebar); once you type, Esc clears. -->
		{#if typing}<kbd>Esc</kbd>{:else}<kbd>{isAppleKeyboard ? '⌘ ,' : 'Ctrl ,'}</kbd>{/if}
	</label>

	{#if typing && !draftText}
		<p class="hint">↑ ↓ to pick a setting, and Return to open it. Or write what you want, like “keep answers short”, and Return asks {spoken} to draft it.</p>
	{/if}

	{#if draftText}
		{#key draftKey}
			<DraftCard text={draftText} assistantName={spoken} onapplied={draftApplied} ondismiss={() => (draftText = null)} onedit={editFirst} />
		{/key}
	{:else if typing}
		<div id="settings-results" class="results" role="listbox" aria-label="Matching settings">
			{#each groups as group (group.title)}
				<p class="label">{group.title}</p>
				{#each group.settings as setting (setting.id)}
					{@const index = results.indexOf(setting)}
					<button
						type="button"
						class="result"
						class:picked={index === picked}
						role="option"
						aria-selected={index === picked}
						onmouseenter={() => (picked = index)}
						onclick={() => open(setting)}
					>
						<span>{titleOf(setting.title, spoken)}</span>
						<span class="go" aria-hidden="true">
							<svg viewBox="0 0 24 24" width="15" height="15"><path fill="none" stroke="currentColor" stroke-linecap="round" stroke-linejoin="round" stroke-width="1.8" d="m9 18l6-6l-6-6" /></svg>
						</span>
					</button>
				{/each}
			{:else}
				<p class="nothing">No setting matches “{query.trim()}”. Press Return to ask {spoken}.</p>
			{/each}
		</div>
	{/if}

	{#if !typing && !draftText}
		{#key version}<ChangedLately assistantName={spoken} />{/key}

		<div class="groups">
			{#each LANDING as group (group.title)}
				<nav aria-label={group.title}>
					<h2 class="label">{group.title}</h2>
					{#each group.links as link (link.route)}
						<a
							href={link.route}
							onclick={(e) => {
								e.preventDefault();
								openSettingsPage(link.route, spoken);
							}}
						>
							<span class="link">{titleOf(link.label, spoken)}</span>
							<span class="blurb">{link.blurb}</span>
						</a>
					{/each}
				</nav>
			{/each}
		</div>

		<footer class="foot">
			<div id="setting-start-over"><ReopenSection /></div>
			<IntroductionSection />
		</footer>
	{/if}
</div>

<style>
	.page {
		max-width: var(--page-reading);
		margin: 0 auto;
		padding: 40px 32px 96px;
	}

	h1 {
		font-size: 2.5rem;
		font-weight: 400;
		line-height: 1.1;
		margin: 0 0 20px;
	}

	.search {
		display: flex;
		align-items: center;
		gap: 12px;
		height: 52px;
		padding: 0 16px;
		border: 1px solid var(--color-border);
		border-radius: var(--radius-lg);
		color: var(--color-foreground-muted);
		transition: border-color var(--motion-hover-out) var(--ease-premium);
	}

	.search:focus-within {
		border-color: var(--color-border-focus);
	}

	.search input {
		flex: 1;
		min-width: 0;
		border: none;
		outline: none;
		background: none;
		font: inherit;
		font-size: 16px;
		color: var(--color-foreground);
	}

	/* Esc clears it, and the hint beside the field says so. */
	.search input::-webkit-search-cancel-button {
		display: none;
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

	.results .label:not(:first-child) {
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

	.groups {
		display: grid;
		grid-template-columns: repeat(3, minmax(0, 1fr));
		gap: 32px;
		margin-top: 40px;
	}

	/* An h2 for the outline, drawn as the small label the other lists use. */
	.groups .label {
		margin-bottom: 8px;
		font-family: var(--font-sans);
		letter-spacing: normal;
	}

	.groups a {
		display: block;
		padding: 8px 0;
		text-decoration: none;
		color: var(--color-foreground);
	}

	.link {
		display: block;
		font-size: 15px;
		transition: color var(--motion-hover-out) var(--ease-premium);
	}

	.groups a:hover .link {
		color: var(--color-primary);
		transition-duration: var(--motion-hover-in);
	}

	.blurb {
		display: block;
		margin-top: 2px;
		font-size: 13px;
		color: var(--color-foreground-muted);
	}

	/* Start over draws its own rule above it. */
	.foot {
		margin-top: 48px;
		display: flex;
		flex-direction: column;
		gap: 28px;
	}

	@media (max-width: 640px) {
		.groups {
			grid-template-columns: minmax(0, 1fr);
			gap: 24px;
		}
	}

	@media (prefers-reduced-motion: reduce) {
		.search,
		.link {
			transition: none;
		}
	}
</style>
