<script lang="ts">
	/**
	 * Settings, on one page (agents/plan/settings-search-plan.md).
	 *
	 * Six sections from `lib/settings`, top to bottom. A setting you change
	 * sits in its row; anything that is a list, a chart, a table or a long
	 * form opens its own page from a row (LinkRow), and Back returns here.
	 *
	 * The search field filters the page in place: rows that don't match hide
	 * (each row asks the filter in context, lib/settings/filter.ts), sections
	 * with no match fold, and their names grey out in the bar. The bar sticks
	 * as you scroll; the title folds into it, and the section you're reading
	 * is underlined. Clicking a name scrolls there and sweeps the highlighter
	 * across its heading once.
	 */
	import { onMount, tick } from 'svelte';
	import { provideSettingsFilter } from '$lib/settings/filter';
	import { invalidate } from '$app/navigation';
	import Button from '$lib/components/Button.svelte';
	import ThemePicker from '$lib/components/ThemePicker.svelte';
	import AssistantNameSettings from '$lib/components/AssistantNameSettings.svelte';
	import StyleNotesSettings from '$lib/components/StyleNotesSettings.svelte';
	import RulesSettings from '$lib/components/RulesSettings.svelte';
	import ModelSettings from '$lib/components/ModelSettings.svelte';
	import SuggestionsSettings from '$lib/components/SuggestionsSettings.svelte';
	import RequestLimitsSettings from '$lib/components/RequestLimitsSettings.svelte';
	import UpdateSection from '$lib/components/settings/UpdateSection.svelte';
	import NetworkSection from '$lib/components/settings/NetworkSection.svelte';
	import ReopenSection from '$lib/components/settings/ReopenSection.svelte';
	import IntroductionSection from '$lib/components/settings/IntroductionSection.svelte';
	import SettingRow from '$lib/components/settings/SettingRow.svelte';
	import LinkRow from '$lib/components/settings/LinkRow.svelte';
	import YouSettings from '$lib/components/settings/YouSettings.svelte';
	import BillingSettings from '$lib/components/settings/BillingSettings.svelte';
	import ChangedLately from '$lib/components/settings/ChangedLately.svelte';
	import DraftCard from '$lib/components/settings/DraftCard.svelte';
	import { getAssistantProfile, getProfile, updateProfile } from '$lib/api/client';
	import { getTheme, isValidTheme, setTheme, type Theme } from '$lib/utils/theme';
	import { SECTIONS, SETTINGS, findSetting, matchingIds, titleOf, type SectionId } from '$lib/settings';

	// The assistant's profile is read once here: its name goes into every
	// section's copy, so a rename shows everywhere as soon as the server has it.
	let name = $state('Ari');
	let styleNotes = $state('');
	let assistant = $state<'loading' | 'ready' | 'error'>('loading');

	async function loadAssistant() {
		assistant = 'loading';
		try {
			const p = await getAssistantProfile<{ assistant_name?: string | null; style_notes?: string | null }>();
			name = p.assistant_name?.trim() || 'Ari';
			styleNotes = p.style_notes ?? '';
			assistant = 'ready';
		} catch {
			assistant = 'error';
		}
	}

	onMount(loadAssistant);

	// Copy names the assistant mid-sentence and at the start of one, so it
	// takes a capital whatever case the owner typed it in.
	const spoken = $derived(name.charAt(0).toUpperCase() + name.slice(1));

	let theme = $state<Theme>(getTheme());

	async function chooseTheme(next: Theme) {
		theme = next;
		setTheme(next);
		try {
			await updateProfile({ theme: next });
			invalidate('/api/profile');
		} catch {
			// The theme is already applied here; the next save or load corrects
			// the server's copy.
		}
	}

	const title = (id: string) => titleOf(findSetting(id)?.title ?? id, spoken);

	// Bumped after an undo from Changed lately, so the sections load again and
	// show the value that came back.
	let version = $state(0);

	/** An undo or an applied draft can change the assistant's name or the
	 *  theme, which this page holds itself, so both are read again with the
	 *  sections and Changed lately. */
	async function refreshSettings() {
		version += 1;
		void loadAssistant();
		try {
			const saved = (await getProfile()).theme;
			if (saved && isValidTheme(saved) && saved !== theme) {
				theme = saved as Theme;
				setTheme(theme);
			}
		} catch {
			// The theme stays as it is; the next load of Settings reads it.
		}
	}

	// ─── Filtering ─────────────────────────────────────────────────────────
	let query = $state('');
	const matches = $derived(matchingIds(query, spoken));
	const shows = (id: string) => !matches || matches.has(id);
	const anyShows = (...ids: string[]) => ids.some(shows);
	provideSettingsFilter({ shows });

	/** Pairing a device is a setting on the Devices page, not a row here. */
	const sectionShown = (id: SectionId) =>
		!matches || SETTINGS.some((s) => s.section === id && s.id !== 'pair-device' && matches!.has(s.id));
	const nothingMatches = $derived(!!matches && !shows('start-over') && !SECTIONS.some((s) => sectionShown(s.id)));

	// ─── Return asks the assistant ─────────────────────────────────────────
	// The text Return sent, while its draft card is open; the key gives each
	// Return a fresh card.
	let draftText = $state<string | null>(null);
	let draftKey = $state(0);

	function onSearchKey(e: KeyboardEvent) {
		if (e.key === 'Enter' && query.trim()) {
			draftText = query.trim();
			draftKey += 1;
			e.preventDefault();
		} else if (e.key === 'Escape' && (draftText || query)) {
			if (draftText) draftText = null;
			else query = '';
			e.preventDefault();
			e.stopPropagation();
		}
	}

	async function draftApplied() {
		draftText = null;
		query = '';
		await refreshSettings();
	}

	/** Edit first: close the draft, show every setting, and go to the row. */
	async function editFirst(settingId: string) {
		draftText = null;
		query = '';
		await tick();
		page?.querySelector(`#setting-${settingId}`)?.scrollIntoView({ behavior: reducedMotion() ? 'auto' : 'smooth', block: 'center' });
	}

	// ─── The bar: folding title, current section, jumping ──────────────────
	let page = $state<HTMLElement>();
	let titleEl = $state<HTMLElement>();
	let barEl = $state<HTMLElement>();
	let folded = $state(false);
	let current = $state<SectionId>('you');
	let lit = $state<SectionId | null>(null);
	let litTimer: ReturnType<typeof setTimeout> | undefined;

	const reducedMotion = () => window.matchMedia('(prefers-reduced-motion: reduce)').matches;

	function goTo(id: SectionId) {
		page?.querySelector(`#section-${id}`)?.scrollIntoView({ behavior: reducedMotion() ? 'auto' : 'smooth', block: 'start' });
		lit = id;
		clearTimeout(litTimer);
		litTimer = setTimeout(() => (lit = null), 1600);
	}

	onMount(() => {
		// The title folds into the bar once it scrolls out of view.
		const io = new IntersectionObserver(([entry]) => (folded = !entry.isIntersecting));
		if (titleEl) io.observe(titleEl);

		// The section you're reading is the last one whose top has passed
		// just under the bar.
		const scroller = page?.closest('.content') ?? window;
		let frame = 0;
		const track = () => {
			cancelAnimationFrame(frame);
			frame = requestAnimationFrame(() => {
				const line = (barEl?.getBoundingClientRect().bottom ?? 0) + 32;
				let next: SectionId = SECTIONS[0].id;
				for (const section of SECTIONS) {
					const el = page?.querySelector(`#section-${section.id}`);
					if (el && el.getBoundingClientRect().top <= line) next = section.id;
				}
				current = next;
			});
		};
		scroller.addEventListener('scroll', track, { passive: true });
		track();
		return () => {
			io.disconnect();
			scroller.removeEventListener('scroll', track);
			cancelAnimationFrame(frame);
			clearTimeout(litTimer);
		};
	});
</script>

<div class="page" bind:this={page}>
	<h1 bind:this={titleEl}>Settings</h1>
	<div class="bar" class:folded bind:this={barEl}>
		<span class="mini" aria-hidden="true">Settings</span>
		<label class="search">
			<svg viewBox="0 0 24 24" aria-hidden="true" width="16" height="16">
				<path fill="none" stroke="currentColor" stroke-linecap="round" stroke-width="1.7" d="M19 11.5a7.5 7.5 0 1 1-15 0a7.5 7.5 0 0 1 15 0m-2.107 5.42l3.08 3.08" />
			</svg>
			<input type="search" bind:value={query} placeholder="Find a setting, or say what you want changed" aria-label="Search settings" autocomplete="off" spellcheck="false" onkeydown={onSearchKey} />
		</label>
		<nav aria-label="Sections">
			{#each SECTIONS as section (section.id)}
				<button
					type="button"
					class:current={!matches && current === section.id}
					disabled={!sectionShown(section.id)}
					onclick={() => goTo(section.id)}>{titleOf(section.title, spoken)}</button
				>
			{/each}
		</nav>
	</div>

	{#if query.trim() && !draftText}
		<p class="hint"><kbd>Return</kbd> asks {spoken} to draft it.</p>
	{/if}

	{#if draftText}
		{#key draftKey}
			<DraftCard
				text={draftText}
				assistantName={spoken}
				onapplied={draftApplied}
				ondismiss={() => (draftText = null)}
				onedit={editFirst}
			/>
		{/key}
	{/if}

	{#if !matches}
		{#key version}<ChangedLately assistantName={spoken} onundo={refreshSettings} />{/key}
	{/if}

	{#if nothingMatches && !draftText}
		<p class="nothing">Nothing in Settings matches “{query.trim()}”. Esc shows every setting.</p>
	{/if}

	{#key version}
	{#each SECTIONS as section (section.id)}
		<section id="section-{section.id}" aria-labelledby="heading-{section.id}" class:filtered={!sectionShown(section.id)}>
			<h2 id="heading-{section.id}" class:lit={lit === section.id}><span class="hl">{titleOf(section.title, spoken)}</span></h2>
			<p class="lede">{titleOf(section.lede, spoken)}</p>
			<div class="rows">
				{#if section.id === 'you'}
					<YouSettings assistantName={spoken} />
				{:else if section.id === 'assistant'}
					{#if assistant === 'error' && !matches}
						<!-- Name and notes would otherwise show defaults the server may
						     not hold, and a save from there would overwrite what it does. -->
						<div class="load-error" role="alert">
							<p>Your server couldn't load {spoken}'s name and notes.</p>
							<Button variant="secondary" size="sm" onclick={loadAssistant}>Try again</Button>
						</div>
					{/if}
					<div class="block" id="setting-assistant-name" class:filtered={!shows('assistant-name')}><AssistantNameSettings bind:name disabled={assistant !== 'ready'} /></div>
					<div class="block" id="setting-style" class:filtered={!shows('style')}>
						<StyleNotesSettings bind:notes={styleNotes} assistantName={spoken} disabled={assistant !== 'ready'} />
					</div>
					<div class="block" id="setting-subjects" class:filtered={!shows('subjects')}><RulesSettings assistantName={spoken} /></div>
					<LinkRow id="memory" label={title('memory')} help="Notes {spoken} keeps from your chats. Edit one and it becomes yours." href="/virtues/assistant/memory" />
					<!-- The catalog is reached from inside this block ("Compare all"). -->
					<div class="block" id="setting-models" class:filtered={!anyShows('models', 'model-catalog')}><ModelSettings assistantName={spoken} /></div>
					<div class="block" id="setting-suggested-questions" class:filtered={!shows('suggested-questions')}><SuggestionsSettings assistantName={spoken} /></div>
				{:else if section.id === 'data'}
					<LinkRow id="keys" label={title('keys')} help="Keys your applets use. Values show only when you ask." href="/virtues/keys" />
					<LinkRow id="shared-links" label={title('shared-links')} help="Every link you've shared, and turning one off." href="/virtues/shared" />
				{:else if section.id === 'appearance'}
					<SettingRow id="theme" label={title('theme')} help="Light, dark, and the colors of each." block>
						<ThemePicker value={theme} onchange={chooseTheme} />
					</SettingRow>
				{:else if section.id === 'billing'}
					<BillingSettings />
					<div class="block" id="setting-limit-run" class:filtered={!anyShows('limit-day', 'limit-run')}><span id="setting-limit-day"></span><RequestLimitsSettings assistantName={spoken} /></div>
					<LinkRow id="byo-ai" label={title('byo-ai')} help="Send AI calls to your own provider and key instead of paying through Virtues." href="/virtues/billing/byo" />
					<LinkRow id="ai-cost" label={title('ai-cost')} help="This month, day by day, each kind of work, and every call." href="/virtues/billing" />
				{:else if section.id === 'server'}
					<div class="block" id="setting-updates" class:filtered={!anyShows('updates', 'auto-install', 'update-channel')}><span id="setting-auto-install"></span><span id="setting-update-channel"></span><UpdateSection /></div>
					<div class="block" id="setting-reach" class:filtered={!shows('reach')}><NetworkSection /></div>
					<LinkRow id="search-runs-on" label={title('search-runs-on')} help="Where search runs, and how much of your record it covers." href="/virtues/search" />
					<LinkRow id="server-health" label={title('server-health')} help="Storage, memory, backup and the last day, measured." href="/virtues/system" />
					<LinkRow id="devices" label={title('devices')} help="Everything paired with your server, and pairing a new one." href="/virtues/devices" />
				{/if}
			</div>
		</section>
	{/each}
	{/key}

	<footer class="foot" class:filtered={!!matches && !shows('start-over')}>
		<div id="setting-start-over" data-setting="start-over" class:filtered={!shows('start-over')}><ReopenSection /></div>
		{#if !matches}<IntroductionSection />{/if}
	</footer>
</div>

<style>
	.page {
		max-width: 720px;
		margin: 0 auto;
		padding: 48px 32px 96px;
		box-sizing: border-box;
	}

	h1 {
		font-size: 2.5rem;
		font-weight: 400;
		line-height: 1.1;
		margin: 0 0 16px;
	}

	/* The bar sticks to the top of the scroll. Once the title has scrolled
	   away, a small "Settings" slides in at its start. */
	.bar {
		position: sticky;
		top: 0;
		z-index: 2;
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: 8px 16px;
		padding: 12px 0;
		/* The pane's own paper (SplitContainer), so rows pass under it cleanly. */
		background: var(--color-page);
		border-bottom: 1px solid var(--color-border-subtle);
	}

	.mini {
		max-width: 0;
		overflow: hidden;
		white-space: nowrap;
		opacity: 0;
		font-family: var(--font-serif);
		font-size: 20px;
		color: var(--color-foreground);
		transition:
			max-width 260ms var(--ease-premium),
			opacity 200ms var(--ease-premium);
	}

	.folded .mini {
		max-width: 120px;
		opacity: 1;
	}

	.search {
		flex: 1 1 200px;
		display: flex;
		align-items: center;
		gap: 8px;
		height: 36px;
		padding: 0 12px;
		border: 1px solid var(--color-border);
		border-radius: var(--radius-md);
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
		font-size: 14px;
		color: var(--color-foreground);
	}

	nav {
		display: flex;
		flex-wrap: wrap;
		gap: 4px 16px;
	}

	nav button {
		position: relative;
		border: none;
		background: none;
		padding: 4px 0;
		font: inherit;
		font-size: 14px;
		color: var(--color-foreground-muted);
		cursor: pointer;
		transition: color var(--motion-hover-out) var(--ease-premium);
	}

	nav button:hover:not(:disabled) {
		color: var(--color-foreground);
		transition-duration: var(--motion-hover-in);
	}

	/* The section you're reading: ink, with a short rule under it. */
	nav button.current {
		color: var(--color-foreground);
	}

	nav button::after {
		content: '';
		position: absolute;
		left: 0;
		right: 0;
		bottom: 0;
		height: 2px;
		background: var(--color-foreground);
		transform: scaleX(0);
		transition: transform 220ms var(--ease-premium);
	}

	nav button.current::after {
		transform: scaleX(1);
	}

	/* No match in that section while you type. */
	nav button:disabled {
		color: var(--color-foreground-disabled);
		cursor: default;
	}

	.hint {
		margin: 8px 0 0;
		font-size: 13px;
		color: var(--color-foreground-muted);
	}

	.hint kbd {
		font-family: inherit;
		font-size: 12px;
		padding: 0 4px;
		border: 1px solid var(--color-border);
		border-radius: var(--radius-sm);
	}

	.nothing {
		margin: 32px 0 0;
		font-size: 14px;
		color: var(--color-foreground-muted);
	}

	.filtered {
		display: none;
	}

	section {
		margin-top: 64px;
		/* Lands under the sticky bar, not behind it. */
		scroll-margin-top: 120px;
	}

	h2 {
		font-size: 1.75rem;
		font-weight: 400;
		margin: 0 0 4px;
	}

	/* One sweep of the highlighter when you jump to a section, the way a
	   cited passage lights when you open it. */
	.hl {
		position: relative;
		z-index: 0;
	}

	.hl::after {
		content: '';
		position: absolute;
		left: -4px;
		right: -4px;
		bottom: 4px;
		height: 40%;
		z-index: -1;
		border-radius: var(--radius-sm);
		background: var(--color-highlight);
		transform: scaleX(0);
		transform-origin: left;
		opacity: 0;
	}

	h2.lit .hl::after {
		animation: section-lit 1600ms var(--ease-premium) both;
	}

	@keyframes section-lit {
		0% {
			transform: scaleX(0);
			opacity: 1;
		}
		30% {
			transform: scaleX(1);
			opacity: 1;
		}
		70% {
			opacity: 1;
		}
		100% {
			transform: scaleX(1);
			opacity: 0;
		}
	}

	@media (prefers-reduced-motion: reduce) {
		.mini,
		nav button,
		nav button::after {
			transition: none;
		}

		h2.lit .hl::after {
			animation: none;
		}
	}

	.lede {
		margin: 0 0 16px;
		font-size: 14px;
		color: var(--color-foreground-muted);
	}

	.rows {
		border-top: 1px solid var(--color-border-subtle);
	}

	/* An existing settings block in a row's place: its own heading and
	   controls, with the rows' hairline under it. */
	.block {
		padding: 16px 0;
		border-bottom: 1px solid var(--color-border-subtle);
	}

	.load-error {
		display: flex;
		flex-direction: column;
		align-items: flex-start;
		gap: 8px;
		padding: 12px 0;
	}

	.load-error p {
		margin: 0;
		font-size: 13px;
		color: var(--color-error);
	}

	.foot {
		margin-top: 64px;
		padding-top: 20px;
		border-top: 1px solid var(--color-border-subtle);
		display: flex;
		flex-direction: column;
		gap: 28px;
	}
</style>
