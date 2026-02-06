<script lang="ts">
	import type { Tab } from '$lib/tabs/types';
	import { Page } from '$lib';
	import CitedMarkdown from '$lib/components/CitedMarkdown.svelte';

	let { tab, active }: { tab: Tab; active: boolean } = $props();

	// Changelog entries - hardcoded for now, can be fetched from /api/changelog later
	const changelog = `
## v1.0 - Initial Release

- Personal AI assistant with multi-model support
- Real-time data streaming from iOS, macOS, and cloud services
- Wiki system with auto-generated people, places, and timeline
- Page editor with collaborative editing
- Drive for personal file storage
- Financial tracking via Plaid and Apple FinanceKit
- Health data from HealthKit
- Location and activity tracking
`;
</script>

<Page>
	<div class="max-w-3xl">
		<div class="mb-8">
			<h1 class="text-3xl font-serif font-medium text-foreground mb-2">What's New</h1>
			<p class="text-foreground-muted">Latest updates and improvements</p>
		</div>

		<div class="prose prose-sm max-w-none
			[&_h2]:text-xl [&_h2]:font-serif [&_h2]:font-medium [&_h2]:text-foreground [&_h2]:mt-8 [&_h2]:mb-3
			[&_h3]:text-lg [&_h3]:font-medium [&_h3]:text-foreground [&_h3]:mt-6 [&_h3]:mb-2
			[&_ul]:space-y-1 [&_ul]:text-foreground-muted
			[&_li]:text-sm
			[&_p]:text-foreground-muted [&_p]:text-sm
		">
			<CitedMarkdown content={changelog} />
		</div>
	</div>
</Page>
