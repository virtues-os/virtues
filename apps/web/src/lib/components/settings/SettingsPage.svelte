<script lang="ts">
	/**
	 * Settings, on one page (agents/plan/settings-search-plan.md).
	 *
	 * Six sections from `lib/settings`, top to bottom. A setting you change
	 * sits in its row; anything that is a list, a chart, a table or a long
	 * form opens its own page from a row (LinkRow), and Back returns here.
	 * The section names at the top scroll to their section.
	 */
	import { onMount } from 'svelte';
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
	import { getAssistantProfile, updateProfile } from '$lib/api/client';
	import { getTheme, setTheme, type Theme } from '$lib/utils/theme';
	import { SECTIONS, findSetting, titleOf, type SectionId } from '$lib/settings';

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

	let page = $state<HTMLElement>();

	function goTo(id: SectionId) {
		page?.querySelector(`#section-${id}`)?.scrollIntoView({ behavior: 'smooth', block: 'start' });
	}
</script>

<div class="page" bind:this={page}>
	<header class="top">
		<h1>Settings</h1>
		<nav aria-label="Sections">
			{#each SECTIONS as section (section.id)}
				<button type="button" onclick={() => goTo(section.id)}>{titleOf(section.title, spoken)}</button>
			{/each}
		</nav>
	</header>

	{#each SECTIONS as section (section.id)}
		<section id="section-{section.id}" aria-labelledby="heading-{section.id}">
			<h2 id="heading-{section.id}">{titleOf(section.title, spoken)}</h2>
			<p class="lede">{titleOf(section.lede, spoken)}</p>
			<div class="rows">
				{#if section.id === 'you'}
					<YouSettings assistantName={spoken} />
				{:else if section.id === 'assistant'}
					{#if assistant === 'error'}
						<!-- Name and notes would otherwise show defaults the server may
						     not hold, and a save from there would overwrite what it does. -->
						<div class="load-error" role="alert">
							<p>Your server couldn't load {spoken}'s name and notes.</p>
							<Button variant="secondary" size="sm" onclick={loadAssistant}>Try again</Button>
						</div>
					{/if}
					<div class="block"><AssistantNameSettings bind:name disabled={assistant !== 'ready'} /></div>
					<div class="block">
						<StyleNotesSettings bind:notes={styleNotes} assistantName={spoken} disabled={assistant !== 'ready'} />
					</div>
					<div class="block"><RulesSettings assistantName={spoken} /></div>
					<LinkRow id="memory" label={title('memory')} help="Notes {spoken} keeps from your chats. Edit one and it becomes yours." href="/virtues/assistant/memory" />
					<div class="block"><ModelSettings assistantName={spoken} /></div>
					<div class="block"><SuggestionsSettings assistantName={spoken} /></div>
				{:else if section.id === 'data'}
					<LinkRow id="keys" label={title('keys')} help="Keys your applets use. Values show only when you ask." href="/virtues/keys" />
					<LinkRow id="shared-links" label={title('shared-links')} help="Every link you've shared, and turning one off." href="/virtues/shared" />
				{:else if section.id === 'appearance'}
					<SettingRow id="theme" label={title('theme')} help="Light, dark, and the colors of each." block>
						<ThemePicker value={theme} onchange={chooseTheme} />
					</SettingRow>
				{:else if section.id === 'billing'}
					<BillingSettings />
					<div class="block"><RequestLimitsSettings assistantName={spoken} /></div>
					<LinkRow id="byo-ai" label={title('byo-ai')} help="Send AI calls to your own provider and key instead of paying through Virtues." href="/virtues/billing/byo" />
					<LinkRow id="ai-cost" label={title('ai-cost')} help="This month, day by day, each kind of work, and every call." href="/virtues/billing" />
				{:else if section.id === 'server'}
					<div class="block"><UpdateSection /></div>
					<div class="block"><NetworkSection /></div>
					<LinkRow id="search-runs-on" label={title('search-runs-on')} help="Where search runs, and how much of your record it covers." href="/virtues/search" />
					<LinkRow id="server-health" label={title('server-health')} help="Storage, memory, backup and the last day, measured." href="/virtues/system" />
					<LinkRow id="devices" label={title('devices')} help="Everything paired with your server, and pairing a new one." href="/virtues/devices" />
				{/if}
			</div>
		</section>
	{/each}

	<footer class="foot">
		<div id="setting-start-over" data-setting="start-over"><ReopenSection /></div>
		<IntroductionSection />
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

	nav {
		display: flex;
		flex-wrap: wrap;
		gap: 6px 20px;
		padding: 8px 0;
		border-bottom: 1px solid var(--color-border-subtle);
	}

	nav button {
		border: none;
		background: none;
		padding: 4px 0;
		font: inherit;
		font-size: 14px;
		color: var(--color-foreground-muted);
		cursor: pointer;
		transition: color var(--motion-hover-out) var(--ease-premium);
	}

	nav button:hover {
		color: var(--color-foreground);
		transition-duration: var(--motion-hover-in);
	}

	section {
		margin-top: 64px;
		scroll-margin-top: 24px;
	}

	h2 {
		font-size: 1.75rem;
		font-weight: 400;
		margin: 0 0 4px;
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
