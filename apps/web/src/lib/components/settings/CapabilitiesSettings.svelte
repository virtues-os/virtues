<script lang="ts">
	/**
	 * What the assistant can do: one switch per capability
	 * (`/api/settings/capabilities`). Your server enforces them
	 * (`api/capabilities.rs`): an off capability's tools are not offered in a
	 * chat, and the tool runner refuses them for applets too.
	 */
	import SwitchList from './SwitchList.svelte';
	import { getCapabilities, setCapability, type Capability } from '$lib/api/client';
	import { GOOGLE_WORDS } from '$lib/sources/capabilities';

	let { assistantName }: { assistantName: string } = $props();

	const WORDS: Record<Capability['id'], { label: string; help: string }> = {
		web_search: { label: 'Search the web', help: 'Looks things up online when a question needs it.' },
		browser: { label: 'Use your browser', help: 'Browses the web in the Virtues app on your Mac, while the app is open.' },
		applets: { label: 'Make sites, skills and tasks that repeat', help: 'Builds sites, skills and tasks that repeat, and edits them. What you already have keeps running.' },
		pages: { label: 'Write in your pages', help: 'Creates pages and edits them when you ask.' },
		email: GOOGLE_WORDS.email,
		calendar: GOOGLE_WORDS.calendar,
	};
</script>

<SwitchList
	id="capabilities"
	label="Tools"
	help="Turn a tool off and {assistantName} won't use it, in a chat or in a task."
	words={WORDS}
	load={getCapabilities}
	save={setCapability}
/>
