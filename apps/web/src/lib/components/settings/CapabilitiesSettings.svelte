<script lang="ts">
	/**
	 * What the assistant can do: one switch per capability
	 * (`/api/settings/capabilities`). Your server enforces them
	 * (`api/capabilities.rs`): an off capability's tools are not offered in a
	 * chat, and the tool runner refuses them for applets too.
	 */
	import SwitchList from './SwitchList.svelte';
	import { getCapabilities, setCapability, type Capability } from '$lib/api/client';
	import { findSetting, titleOf } from '$lib/settings';

	let { assistantName }: { assistantName: string } = $props();

	const WORDS: Record<Capability['id'], { label: string; help: string }> = {
		web_search: { label: 'Search the web', help: 'Looks things up online when a question needs it.' },
		browser: { label: 'Use your browser', help: 'Works in the browser in the Virtues app on your Mac, while the app is open.' },
		applets: { label: 'Make and change applets', help: 'Builds applets that run on their own, and edits them. Running them stays on.' },
		pages: { label: 'Write in your pages', help: 'Creates pages and edits them when you ask.' },
	};

	const title = $derived(titleOf(findSetting('capabilities')?.title ?? 'What {name} can do', assistantName));
</script>

<SwitchList
	id="capabilities"
	label={title}
	help="Turn one off and {assistantName} won't do it, in a chat or in an applet."
	words={WORDS}
	load={getCapabilities}
	save={setCapability}
/>
