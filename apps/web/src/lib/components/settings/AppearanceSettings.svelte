<script lang="ts">
	/**
	 * Appearance after the theme and accent, under "On this device": Glass
	 * (stores/glass), reading font and text size, which are the page display
	 * settings a page's Aa menu also sets (stores/pageDisplay), and Reduce
	 * motion (stores/motion). All four are kept in this browser, so a phone
	 * and a Mac can differ; the theme and accent above are saved to your
	 * profile and follow you.
	 */
	import SettingRow from './SettingRow.svelte';
	import Segmented from '$lib/components/Segmented.svelte';
	import Switch from './Switch.svelte';
	import { pageDisplay, type FontMode, type TextSize } from '$lib/stores/pageDisplay.svelte';
	import { motion } from '$lib/stores/motion.svelte';
	import { glass, type GlassLevel } from '$lib/stores/glass.svelte';
	import { titleOf, findSetting } from '$lib/settings';

	let { assistantName }: { assistantName: string } = $props();

	// Each font option is set in the face it picks.
	const FONTS = [
		{ value: 'sans', label: 'Sans serif', style: 'font-family: var(--font-sans)' },
		{ value: 'serif', label: 'Serif', style: 'font-family: var(--font-serif); font-size: 14px' },
		{ value: 'mono', label: 'Mono', style: 'font-family: var(--font-mono); font-size: 12px' },
	] as const satisfies readonly { value: FontMode; label: string; style: string }[];

	const SIZES = [
		{ value: 's', label: 'Small' },
		{ value: 'm', label: 'Medium' },
		{ value: 'l', label: 'Large' },
	] as const satisfies readonly { value: TextSize; label: string }[];

	const GLASS = [
		{ value: 'none', label: 'None' },
		{ value: 'low', label: 'Low' },
		{ value: 'medium', label: 'Medium' },
		{ value: 'high', label: 'High' },
	] as const satisfies readonly { value: GlassLevel; label: string }[];

	const title = (id: string) => titleOf(findSetting(id)?.title ?? id, assistantName);

	const motionHelp = $derived(
		motion.device
			? 'Your device already asks for less motion, so the app keeps still either way.'
			: 'Things appear and close without sliding or fading.',
	);
</script>

<section class="group">
	<h2 class="settings-label">On this device</h2>
	<p class="lede">These stay on this device, so your phone and your Mac can differ.</p>

	<SettingRow id="glass" label={title('glass')} help="How much of the painting shows through the sidebar and tabs. None makes them solid.">
		<Segmented id="glass-choice" label={title('glass')} value={glass.level} options={GLASS} onchange={(v) => glass.set(v)} />
	</SettingRow>

	<SettingRow id="reading-font" label={title('reading-font')} help="The typeface your pages are set in.">
		<Segmented id="reading-font-choice" label={title('reading-font')} value={pageDisplay.fontMode} options={FONTS} onchange={(v) => pageDisplay.setFontMode(v)} />
	</SettingRow>

	<SettingRow id="text-size" label={title('text-size')} help="How large text is on your pages.">
		<Segmented id="text-size-choice" label={title('text-size')} value={pageDisplay.textSize} options={SIZES} onchange={(v) => pageDisplay.setTextSize(v)} />
	</SettingRow>

	<SettingRow id="reduce-motion" label={title('reduce-motion')} help={motionHelp}>
		<Switch
			checked={motion.reduced}
			label={title('reduce-motion')}
			disabled={motion.device}
			onchange={(on) => motion.set(on ? 'reduce' : 'device')}
		/>
	</SettingRow>
</section>

<style>
	.group {
		padding: 32px 0 8px;
	}

	.lede {
		margin: 0 0 8px;
		font-size: 13px;
		line-height: 1.45;
		color: var(--color-foreground-muted);
	}
</style>
