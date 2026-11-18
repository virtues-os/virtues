<script lang="ts">
	/**
	 * Appearance rows after the theme: Glass (stores/glass), reading font and
	 * text size, which are the page display settings a page's Aa menu also
	 * sets (stores/pageDisplay), and Reduce motion (stores/motion). All four
	 * are kept in this browser.
	 */
	import SettingRow from './SettingRow.svelte';
	import Select from '$lib/components/Select.svelte';
	import Switch from './Switch.svelte';
	import { pageDisplay, type FontMode, type TextSize } from '$lib/stores/pageDisplay.svelte';
	import { motion } from '$lib/stores/motion.svelte';
	import { glass, type GlassLevel } from '$lib/stores/glass.svelte';
	import { titleOf, findSetting } from '$lib/settings';

	let { assistantName }: { assistantName: string } = $props();

	const FONTS = [
		{ value: 'sans', label: 'Sans serif' },
		{ value: 'serif', label: 'Serif' },
		{ value: 'mono', label: 'Monospace' },
	] as const satisfies readonly { value: FontMode; label: string }[];

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

<SettingRow id="glass" label={title('glass')} help="How much of the painting shows through the sidebar, tabs and buttons. None makes them solid." forId="glass-choice">
	<Select id="glass-choice" value={glass.level} options={GLASS} onchange={(v) => glass.set(v)} />
</SettingRow>

<SettingRow id="reading-font" label={title('reading-font')} help="The typeface your pages are set in. A page's Aa menu changes it too." forId="reading-font-choice">
	<Select id="reading-font-choice" value={pageDisplay.fontMode} options={FONTS} onchange={(v) => pageDisplay.setFontMode(v)} />
</SettingRow>

<SettingRow id="text-size" label={title('text-size')} help="How large text is on your pages." forId="text-size-choice">
	<Select id="text-size-choice" value={pageDisplay.textSize} options={SIZES} onchange={(v) => pageDisplay.setTextSize(v)} />
</SettingRow>

<SettingRow id="reduce-motion" label={title('reduce-motion')} help={motionHelp}>
	<Switch
		checked={motion.reduced}
		label={title('reduce-motion')}
		disabled={motion.device}
		onchange={(on) => motion.set(on ? 'reduce' : 'device')}
	/>
</SettingRow>
