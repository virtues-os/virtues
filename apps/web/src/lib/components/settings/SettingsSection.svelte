<script lang="ts">
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
	import { invalidate } from '$app/navigation';
	import { reducedMotion } from '$lib/stores/motion.svelte';
	import { lightUp } from '$lib/utils/lit';
	import Button from '$lib/components/Button.svelte';
	import ThemePicker from '$lib/components/ThemePicker.svelte';
	import AssistantNameSettings from '$lib/components/AssistantNameSettings.svelte';
	import StyleNotesSettings from '$lib/components/StyleNotesSettings.svelte';
	import RulesSettings from '$lib/components/RulesSettings.svelte';
	import ModelSettings from '$lib/components/ModelSettings.svelte';
	import SuggestionsSettings from '$lib/components/SuggestionsSettings.svelte';
	import RequestLimitsSettings from '$lib/components/RequestLimitsSettings.svelte';
	import UpdateSection from './UpdateSection.svelte';
	import NetworkSection from './NetworkSection.svelte';
	import SettingRow from './SettingRow.svelte';
	import LinkRow from './LinkRow.svelte';
	import YouSettings from './YouSettings.svelte';
	import BillingSettings from './BillingSettings.svelte';
	import AppearanceSettings from './AppearanceSettings.svelte';
	import CapabilitiesSettings from './CapabilitiesSettings.svelte';
	import { getAssistantProfile, updateProfile } from '$lib/api/client';
	import { getTheme, setTheme, type Theme } from '$lib/utils/theme';
	import { SECTIONS, findSetting, titleOf, type SectionId } from '$lib/settings';
	import { openSettingsPage, takePendingSetting } from '$lib/settings/focus';

	let { section: id }: { section: SectionId } = $props();

	const section = $derived(SECTIONS.find((s) => s.id === id)!);

	// The assistant's name goes into titles and help on every page; its
	// notes only on its own.
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

	// Copy names the assistant mid-sentence and at the start of one, so it
	// takes a capital whatever case the owner typed it in.
	const spoken = $derived(name.charAt(0).toUpperCase() + name.slice(1));
	const title = (settingId: string) => titleOf(findSetting(settingId)?.title ?? settingId, spoken);

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

	let lit = $state<string | null>(null);

	onMount(() => {
		void loadAssistant();
		const settingId = takePendingSetting();
		if (!settingId) return;
		// Rows load their values after the page mounts, and the tab restores
		// its own scroll position once the route settles. Scroll after a beat,
		// then once more, at once, if the row still isn't on screen.
		const row = () => document.getElementById(`setting-${settingId}`);
		const first = setTimeout(async () => {
			await tick();
			row()?.scrollIntoView({ behavior: reducedMotion() ? 'auto' : 'smooth', block: 'center' });
			lit = settingId;
		}, 250);
		const again = setTimeout(() => {
			const box = row()?.getBoundingClientRect();
			if (box && (box.top < 0 || box.bottom > window.innerHeight)) row()?.scrollIntoView({ block: 'center' });
		}, 900);
		return () => {
			clearTimeout(first);
			clearTimeout(again);
		};
	});

	// Lights the row the page arrived for, once (the class is added to the
	// row's own element, which lives in a child component).
	$effect(() => {
		if (!lit) return;
		const el = document.getElementById(`setting-${lit}`);
		if (el) lightUp(el);
	});
</script>

<div class="page">
	<button type="button" class="back" onclick={() => openSettingsPage('/virtues', spoken)}>
		<svg viewBox="0 0 24 24" aria-hidden="true" width="16" height="16">
			<path fill="none" stroke="currentColor" stroke-linecap="round" stroke-linejoin="round" stroke-width="1.7" d="M15 6l-6 6l6 6" />
		</svg>
		Settings
	</button>
	<h1>{titleOf(section.title, spoken)}</h1>
	<p class="lede">{titleOf(section.lede, spoken)}</p>

	<div class="rows">
		{#if id === 'you'}
			<YouSettings assistantName={spoken} />
		{:else if id === 'assistant'}
			{#if assistant === 'error'}
				<!-- Name and notes would otherwise show defaults the server may
				     not hold, and a save from there would overwrite what it does. -->
				<div class="load-error" role="alert">
					<p>Your server couldn't load {spoken}'s name and notes.</p>
					<Button variant="secondary" size="sm" onclick={loadAssistant}>Try again</Button>
				</div>
			{/if}
			<div class="block" id="setting-assistant-name"><AssistantNameSettings bind:name disabled={assistant !== 'ready'} /></div>
			<div class="block" id="setting-style">
				<StyleNotesSettings bind:notes={styleNotes} assistantName={spoken} disabled={assistant !== 'ready'} />
			</div>
			<CapabilitiesSettings assistantName={spoken} />
			<div class="block" id="setting-subjects"><RulesSettings assistantName={spoken} /></div>
			<!-- The catalog is reached from inside this block ("Compare all"). -->
			<div class="block" id="setting-models"><ModelSettings assistantName={spoken} /></div>
			<div class="block" id="setting-suggested-questions"><SuggestionsSettings assistantName={spoken} /></div>
			<LinkRow id="memory" label={title('memory')} help="Notes {spoken} keeps from your chats. Edit one and it becomes yours." href="/virtues/assistant/memory" />
		{:else if id === 'appearance'}
			<SettingRow id="theme" label={title('theme')} help="Light, dark, and the colors of each." block>
				<ThemePicker value={theme} onchange={chooseTheme} />
			</SettingRow>
			<AppearanceSettings assistantName={spoken} />
		{:else if id === 'billing'}
			<BillingSettings />
			<div class="block" id="setting-limit-run"><span id="setting-limit-day"></span><RequestLimitsSettings assistantName={spoken} /></div>
			<LinkRow id="byo-ai" label={title('byo-ai')} help="Send AI calls to your own provider and key instead of paying through Virtues." href="/virtues/billing/byo" />
			<LinkRow id="ai-cost" label={title('ai-cost')} help="This month, day by day, each kind of work, and every call." href="/virtues/billing/cost" />
		{:else if id === 'server'}
			<div class="block" id="setting-updates"><span id="setting-auto-install"></span><span id="setting-update-channel"></span><UpdateSection /></div>
			<div class="block" id="setting-reach"><NetworkSection /></div>
		{/if}
	</div>
</div>

<style>
	.page {
		max-width: var(--page-reading);
		margin: 0 auto;
		padding: 32px 32px 96px;
	}

	.back {
		display: inline-flex;
		align-items: center;
		gap: 4px;
		margin: 0 0 20px -4px;
		padding: 4px;
		border: none;
		border-radius: var(--radius-sm);
		background: none;
		font: inherit;
		font-size: 14px;
		color: var(--color-foreground-muted);
		cursor: pointer;
		transition: color var(--motion-hover-out) var(--ease-premium);
	}

	.back:hover {
		color: var(--color-foreground);
		transition-duration: var(--motion-hover-in);
	}

	h1 {
		font-size: 2.5rem;
		font-weight: 400;
		line-height: 1.1;
		margin: 0;
	}

	.lede {
		margin: 8px 0 24px;
		font-size: 15px;
		line-height: 1.5;
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

	/* The older blocks title themselves with an h2, which takes the serif
	   heading face; on these pages they read as the rows' names do. */
	.rows :global(.settings-label) {
		font-family: var(--font-sans);
		font-size: 15px;
		letter-spacing: normal;
		margin-bottom: 4px;
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

	@media (prefers-reduced-motion: reduce) {
		.back {
			transition: none;
		}
	}
</style>
