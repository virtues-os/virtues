<script lang="ts">
	import { DEFAULT_ASSISTANT_NAME } from '$lib/settings';
	/**
	 * One section of Settings on its own page: a link back to Settings, the
	 * title, one sentence, then its rows (`lib/settings`, agents/plan/settings
	 * -search-plan.md). Anything that is a list, a chart, a table or a long
	 * form opens a page of its own from a row (LinkRow).
	 *
	 * Arriving from a search result, Edit first or Changed lately, the page
	 * scrolls to that setting's row and lights it once (`settings/focus`).
	 */
	import { onMount } from 'svelte';
	import ThemePicker from '$lib/components/ThemePicker.svelte';
	import AccentPicker from '$lib/components/AccentPicker.svelte';
	import BackgroundPicker from '$lib/components/backgrounds/BackgroundPicker.svelte';
	import AssistantNameSettings from '$lib/components/AssistantNameSettings.svelte';
	import StyleNotesSettings from '$lib/components/StyleNotesSettings.svelte';
	import RulesSettings from '$lib/components/RulesSettings.svelte';
	import RequestLimitsSettings from '$lib/components/RequestLimitsSettings.svelte';
	import IntroductionSection from './IntroductionSection.svelte';
	import SettingRow from './SettingRow.svelte';
	import ModelRows from './ModelRows.svelte';
	import SettingsPage from './SettingsPage.svelte';
	import LoadError from './LoadError.svelte';
	import LinkRow from './LinkRow.svelte';
	import YouSettings from './YouSettings.svelte';
	import BillingSettings from './BillingSettings.svelte';
	import AppearanceSettings from './AppearanceSettings.svelte';
	import CapabilitiesSettings from './CapabilitiesSettings.svelte';
	import DataSettings from './DataSettings.svelte';
	import { getAssistantProfile, getByoKey } from '$lib/api/client';
	import { getAccent, getThemeChoice, setTheme, type Accent, type ThemeChoice } from '$lib/utils/theme';
	import { SECTIONS, findSetting, titleOf, type SectionId } from '$lib/settings';
	import { openSettingsPage } from '$lib/settings/focus';
	import { lightPendingSetting } from '$lib/settings/light';

	let { section: id }: { section: SectionId } = $props();

	const section = $derived(SECTIONS.find((s) => s.id === id)!);

	// The assistant's name goes into titles and help on every page; its
	// notes only on its own.
	let name = $state(DEFAULT_ASSISTANT_NAME);
	let styleNotes = $state('');
	let assistant = $state<'loading' | 'ready' | 'error'>('loading');

	async function loadAssistant() {
		assistant = 'loading';
		try {
			const p = await getAssistantProfile<{ assistant_name?: string | null; style_notes?: string | null }>();
			name = p.assistant_name?.trim() || DEFAULT_ASSISTANT_NAME;
			styleNotes = p.style_notes ?? '';
			assistant = 'ready';
		} catch {
			assistant = 'error';
		}
	}

	// Where AI runs, on Models and spending's provider row: the host of your
	// own provider, or "Not set". Left out when the read fails, so the row
	// says "Open" and claims nothing.
	let provider = $state<string | undefined>(undefined);
	$effect(() => {
		if (id !== 'spending') return;
		getByoKey<{ configured: boolean; endpoint_url: string | null }>()
			.then((b) => {
				if (!b.configured) provider = 'Not set';
				else {
					try {
						provider = b.endpoint_url ? new URL(b.endpoint_url).host : 'On';
					} catch {
						provider = 'On';
					}
				}
			})
			.catch(() => (provider = undefined));
	});

	// Copy names the assistant mid-sentence and at the start of one, so it
	// takes a capital whatever case the owner typed it in.
	const spoken = $derived(name.charAt(0).toUpperCase() + name.slice(1));
	const title = (settingId: string) => titleOf(findSetting(settingId)?.title ?? settingId, spoken);

	let theme = $state<ThemeChoice>(getThemeChoice());
	let accent = $state<Accent>(getAccent());

	// `setTheme` applies it and saves it in the assistant profile's
	// `ui_preferences`, which is where the app reads it and where Changed
	// lately records it.
	function chooseTheme(next: ThemeChoice) {
		theme = next;
		void setTheme(next);
	}

	onMount(() => {
		void loadAssistant();
		return lightPendingSetting();
	});
</script>

<SettingsPage title={titleOf(section.title, spoken)} lede={titleOf(section.lede, spoken)} assistantName={spoken}>
	<div class="rows">
		{#if id === 'you'}
			<YouSettings assistantName={spoken} />
			<div class="block" id="setting-introduction"><IntroductionSection /></div>
		{:else if id === 'assistant'}
			{#if assistant === 'error'}
				<!-- Name and notes would otherwise show defaults the server may
				     not hold, and a save from there would overwrite what it does. -->
				<LoadError message={`Your server couldn't load ${spoken}'s name and notes.`} onretry={loadAssistant} />
			{/if}
			<div class="block" id="setting-assistant-name"><AssistantNameSettings bind:name disabled={assistant !== 'ready'} /></div>
			<div class="block" id="setting-style">
				<StyleNotesSettings bind:notes={styleNotes} assistantName={spoken} disabled={assistant !== 'ready'} />
			</div>
			<div class="block" id="setting-subjects"><RulesSettings assistantName={spoken} /></div>
		{:else if id === 'tools'}
			<CapabilitiesSettings assistantName={spoken} />
		{:else if id === 'spending'}
			<!-- Each role's row opens the model list with that role picked. -->
			<div class="block" id="setting-models"><ModelRows assistantName={spoken} /></div>
			<div class="block" id="setting-limit-run"><span id="setting-limit-day"></span><RequestLimitsSettings assistantName={spoken} /></div>
			<LinkRow id="ai-cost" label={title('ai-cost')} help="This month day by day, what it went to, and every call." href="/virtues/assistant/spending/cost" />
			<div class="block">
				<h2 class="settings-label">Where it runs</h2>
				<LinkRow
					id="byo-ai"
					label={title('byo-ai')}
					help="Send AI calls to a provider and key of your own."
					href="/virtues/assistant/spending/provider"
					value={provider}
				/>
			</div>
			<p class="settings-aside">
				Your subscription and balance are on
				<button type="button" class="settings-link" onclick={() => openSettingsPage('/virtues/billing', spoken)}>Billing</button>.
			</p>
		{:else if id === 'appearance'}
			<SettingRow id="theme" label={title('theme')} help="Light, dark, the same as your device, or light from sunrise to sunset. It follows you to every device." block>
				<ThemePicker value={theme} onchange={chooseTheme} />
			</SettingRow>
			<SettingRow id="background" label={title('background')} help="The picture behind home. A style or a collection shows a different picture each day. A style downloads to your server the first time you pick it." block>
				<BackgroundPicker />
			</SettingRow>
			<SettingRow id="accent" label={title('accent')} help="The color of links, selections and the main button on each page." block>
				<AccentPicker value={accent} onchange={(next) => (accent = next)} />
			</SettingRow>
			<AppearanceSettings assistantName={spoken} />
		{:else if id === 'billing'}
			<BillingSettings />
			<p class="settings-aside">
				Your own AI provider is on
				<button type="button" class="settings-link" onclick={() => openSettingsPage('/virtues/assistant/spending', spoken)}>Models and spending</button>.
			</p>
		{:else if id === 'data'}
			<DataSettings />
		{/if}
	</div>
</SettingsPage>

<style>
	.rows {
		margin-top: 8px;
	}

	/* A settings block in a row's place: its own heading and controls.
	   Space sets the blocks apart, no rule between them. */
	.block {
		padding: 24px 0 8px;
	}
</style>
