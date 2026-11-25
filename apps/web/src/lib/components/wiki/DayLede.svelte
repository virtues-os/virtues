<!--
	DayLede.svelte

	A day article's opening paragraph, drawn where a list shows it under a
	date (the wiki's front page, a year's days). The stored lede is the
	article's own markdown: its names are links, and what the veil may hide
	sits in ⟦ ⟧ marks, sometimes inside a link's label. Printed as text, both
	showed as raw markup, so this reads the marks (`veilMarks`) and draws the
	rest as the day page does (DayInline): names as quiet refs, which the veil
	hides as links, and the marked phrases hidden by `veiled`.
-->
<script lang="ts">
	import DayInline from './DayInline.svelte';
	import { veilMarks } from '$lib/wiki/dayArticle';
	import { veiled } from '$lib/actions/veil';
	import { veil } from '$lib/stores/veil.svelte';

	let { markdown, class: klass = '' }: { markdown: string; class?: string } = $props();

	const marked = $derived(veilMarks(markdown));
</script>

<span class={klass} use:veiled={{ hiding: veil.hiding, phrases: marked.phrases }}
	><DayInline markdown={marked.markdown} /></span
>
