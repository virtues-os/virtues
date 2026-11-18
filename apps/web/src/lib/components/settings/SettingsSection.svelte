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
	import { onMount, tick } from 'svelte';
	import { reducedMotion } from '$lib/stores/motion.svelte';
	import { lightUp } from '$lib/utils/lit';
	import Button from '$lib/components/Button.svelte';
	import ThemePicker from '$lib/components/ThemePicker.svelte';
	import AccentPicker from '$lib/components/AccentPicker.svelte';
	import AssistantNameSettings from '$lib/components/AssistantNameSettings.svelte';
	import StyleNotesSettings from '$lib/components/StyleNotesSettings.svelte';
	import RulesSettings from '$lib/components/RulesSettings.svelte';
	import SuggestionsSettings from '$lib/components/SuggestionsSettings.svelte';
	import RequestLimitsSettings from '$lib/components/RequestLimitsSettings.svelte';
	import UpdateSection from './UpdateSection.svelte';
	import NetworkSection from './NetworkSection.svelte';
	import ReopenSection from './ReopenSection.svelte';
	import IntroductionSection from './IntroductionSection.svelte';
	import SettingRow from './SettingRow.svelte';
	import ModelRows from './ModelRows.svelte';
	import SettingsPage from './SettingsPage.svelte';
	import LinkRow from './LinkRow.svelte';
	import YouSettings from './YouSettings.svelte';
	import BillingSettings from './BillingSettings.svelte';
	import AppearanceSettings from './AppearanceSettings.svelte';
	import CapabilitiesSettings from './CapabilitiesSettings.svelte';
	import DataSettings from './DataSettings.svelte';
	import { getAssistantProfile } from '$lib/api/client';
	import { getAccent, getThemeChoice, setTheme, type Accent, type ThemeChoice } from '$lib/utils/theme';
	import { SECTIONS, findSetting, titleOf, type SectionId } from '$lib/settings';
	import { openSettingsPage, takePendingSetting } from '$lib/settings/focus';

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
		const settingId = takePendingSetting();
		if (!settingId) return;
		// Some rows draw only once their values load, and the tab restores its
		// own scroll position once the route settles. Wait for the row, then
		// scroll to it, and once more, at once, if it still isn't on screen.
		let timer: ReturnType<typeof setTimeout> | undefined;
		const started = Date.now();
		const find = async () => {
			await tick();
			const el = target(settingId);
			if (!el) {
				if (Date.now() - started < 3000) timer = setTimeout(find, 150);
				return;
			}
			el.scrollIntoView({ behavior: reducedMotion() ? 'auto' : 'smooth', block: 'center' });
			lightUp(el);
			timer = setTimeout(() => {
				const box = el.getBoundingClientRect();
				if (box.top < 0 || box.bottom > window.innerHeight) el.scrollIntoView({ block: 'center' });
			}, 650);
		};
		timer = setTimeout(find, 250);
		return () => clearTimeout(timer);
	});

	/** The element to scroll to and light: the setting's row, or for a setting
	 *  that shares a block with others (an empty marker inside it), the block. */
	function target(settingId: string): HTMLElement | null {
		const el = document.getElementById(`setting-${settingId}`);
		if (!el) return null;
		return el.offsetHeight > 0 ? el : (el.closest<HTMLElement>('.block') ?? el.parentElement);
	}
</script>

<SettingsPage title={titleOf(section.title, spoken)} lede={titleOf(section.lede, spoken)} assistantName={spoken}>
	<div class="rows">
		{#if id === 'you'}
			<YouSettings assistantName={spoken} />
		{:else if id === 'assistant'}
			{#if assistant === 'error'}
				<!-- Name and notes would otherwise show defaults the server may
				     not hold, and a save from there would overwrite what it does. -->
				<div class="load-error" role="alert">
					<p>Your server couldn't load {spoken}'s name and notes.</p>
					<Button variant="secondary" onclick={loadAssistant}>Try again</Button>
				</div>
			{/if}
			<div class="block" id="setting-assistant-name"><AssistantNameSettings bind:name disabled={assistant !== 'ready'} /></div>
			<div class="block" id="setting-style">
				<StyleNotesSettings bind:notes={styleNotes} assistantName={spoken} disabled={assistant !== 'ready'} />
			</div>
			<div class="block" id="setting-subjects"><RulesSettings assistantName={spoken} /></div>
		{:else if id === 'tools'}
			<CapabilitiesSettings assistantName={spoken} />
			<div class="block" id="setting-suggested-questions"><SuggestionsSettings assistantName={spoken} /></div>
		{:else if id === 'spending'}
			<!-- Each role's row opens the model list with that role picked. -->
			<div class="block" id="setting-models"><ModelRows assistantName={spoken} /></div>
			<div class="block" id="setting-limit-run"><span id="setting-limit-day"></span><RequestLimitsSettings assistantName={spoken} /></div>
			<LinkRow id="ai-cost" label={title('ai-cost')} help="This month day by day, what it went to, and every call." href="/virtues/assistant/spending/cost" />
			<p class="aside">
				Your subscription, balance and your own AI key are on
				<button type="button" class="link" onclick={() => openSettingsPage('/virtues/billing', spoken)}>Billing</button>.
			</p>
		{:else if id === 'appearance'}
			<SettingRow id="theme" label={title('theme')} help="Light, dark, or the same as your device." block>
				<ThemePicker value={theme} onchange={chooseTheme} />
			</SettingRow>
			<SettingRow id="accent" label={title('accent')} help="The color of links, selections and the main button on each page." block>
				<AccentPicker value={accent} onchange={(next) => (accent = next)} />
			</SettingRow>
			<AppearanceSettings assistantName={spoken} />
		{:else if id === 'billing'}
			<BillingSettings />
			<LinkRow id="byo-ai" label={title('byo-ai')} help="Send AI calls to your own provider and key instead of paying through Virtues." href="/virtues/billing/byo" />
			<!-- The cost page lives under Models and spending; Billing links to it too, beside the balance it draws down. -->
			<LinkRow id="ai-cost-billing" label={title('ai-cost')} help="This month day by day, what it went to, and every call." href="/virtues/assistant/spending/cost" />
		{:else if id === 'data'}
			<DataSettings />
		{:else if id === 'server'}
			<div class="block" id="setting-updates"><span id="setting-auto-install"></span><span id="setting-update-channel"></span><UpdateSection /></div>
			<NetworkSection />
			<!-- Start over unpairs every device, so it sits alone at the foot. -->
			<div class="foot">
				<div id="setting-start-over"><ReopenSection /></div>
				<IntroductionSection />
			</div>
		{/if}
	</div>
</SettingsPage>

<style>
	.rows {
		margin-top: 8px;
	}

	.foot {
		margin-top: 48px;
		display: flex;
		flex-direction: column;
		gap: 28px;
	}

	/* A settings block in a row's place: its own heading and controls.
	   Space sets the blocks apart, no rule between them. */
	.block {
		padding: 24px 0 8px;
	}

	/* A block's h2 is a section heading: the app's serif h2 at the heading step. */
	.rows :global(.settings-label) {
		font-family: var(--font-serif);
		font-size: var(--text-heading);
		font-weight: 400;
		line-height: 1.3;
		letter-spacing: normal;
		margin: 0 0 8px;
	}

	.aside {
		margin: 24px 0 0;
		font-size: 13px;
		color: var(--color-foreground-subtle);
	}

	.link {
		padding: 0;
		border: none;
		background: none;
		font: inherit;
		color: var(--color-primary);
		cursor: pointer;
	}

	.link:hover {
		text-decoration: underline;
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
</style>
