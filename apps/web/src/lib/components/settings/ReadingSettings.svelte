<script lang="ts">
	/**
	 * What the assistant may read: one switch per kind of record
	 * (`/api/settings/reading`). Your server enforces them
	 * (`api/model_access.rs`): an off kind is withheld from the SQL tool,
	 * search, the day write-ups and applets. It keeps collecting it.
	 */
	import SwitchList from './SwitchList.svelte';
	import { getReading, setReading, type ReadingKind } from '$lib/api/client';

	let { assistantName }: { assistantName: string } = $props();

	const WORDS: Record<ReadingKind['id'], { label: string; help?: string }> = {
		messages: { label: 'Messages' },
		email: { label: 'Email' },
		recordings: { label: 'Recordings and transcripts' },
		calendar: { label: 'Calendar' },
		health: { label: 'Health', help: 'Sleep, heart, steps and workouts.' },
		cycle: { label: 'Cycle and symptoms', help: 'Off until you turn it on.' },
		location: { label: "Places you've been" },
		money: { label: 'Money', help: 'Accounts and transactions.' },
		browsing: { label: 'Browsing and apps' },
		documents: { label: 'Bookmarks and documents' },
	};

</script>

<!-- The page is titled What {name} may read, so the row names the switches. -->
<SwitchList
	id="model-reading"
	label="Kinds of record"
	help="Turn one off and {assistantName} stops seeing it: in chats, in your day pages and in applets. Your server keeps collecting it."
	words={WORDS}
	load={getReading}
	save={setReading}
/>
