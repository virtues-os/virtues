<script lang="ts">
	import type { Tab } from '$lib/tabs/types';
	import { spaceStore } from '$lib/stores/space.svelte';
	import { ActivityHeatmap } from '$lib/components/wiki';
	import { onMount } from 'svelte';
	import Icon from '$lib/components/Icon.svelte';

	let { tab, active }: { tab: Tab; active: boolean } = $props();

	// Entity counts from API
	let entityCounts = $state<{ people: number; places: number; orgs: number }>({
		people: 0,
		places: 0,
		orgs: 0
	});

	// Activity data for heatmap (will come from API)
	let activityData = $state<Map<string, { count: number; slug: string }>>(new Map());
	let loadingActivity = $state(true);

	onMount(async () => {
		// Load entity counts
		try {
			const [peopleRes, placesRes, orgsRes] = await Promise.all([
				fetch('/api/wiki/people'),
				fetch('/api/wiki/places'),
				fetch('/api/wiki/organizations')
			]);

			if (peopleRes.ok) {
				const people = await peopleRes.json();
				entityCounts.people = Array.isArray(people) ? people.length : 0;
			}
			if (placesRes.ok) {
				const places = await placesRes.json();
				entityCounts.places = Array.isArray(places) ? places.length : 0;
			}
			if (orgsRes.ok) {
				const orgs = await orgsRes.json();
				entityCounts.orgs = Array.isArray(orgs) ? orgs.length : 0;
			}
		} catch (e) {
			console.error('Failed to load entity counts:', e);
		}

		// Load activity data for the past year
		try {
			const endDate = new Date();
			const startDate = new Date();
			startDate.setFullYear(startDate.getFullYear() - 1);

			const res = await fetch(
				`/api/wiki/days?start_date=${startDate.toISOString().split('T')[0]}&end_date=${endDate.toISOString().split('T')[0]}`
			);

			if (res.ok) {
				const days = await res.json();
				const dataMap = new Map<string, { count: number; slug: string }>();

				for (const day of days) {
					// Count activity based on whether there's content
					const hasContent = day.autobiography || day.autobiography_sections;
					if (hasContent) {
						dataMap.set(day.date, { count: 1, slug: day.date });
					}
				}

				activityData = dataMap;
			}
		} catch (e) {
			console.error('Failed to load activity data:', e);
		} finally {
			loadingActivity = false;
		}
	});

	// Handle day click from heatmap
	function handleDayClick(_date: Date, slug: string) {
		// slug is a date string like "2026-01-24"
		spaceStore.openTabFromRoute(`/day/day_${slug}`);
	}

	// Handle navigation
	function navigateTo(route: string) {
		spaceStore.openTabFromRoute(route);
	}

	// Today's formatted date
	const today = new Date();
	const todaySlug = today.toISOString().split('T')[0];
	const todayFormatted = today.toLocaleDateString('en-US', {
		weekday: 'long',
		month: 'long',
		day: 'numeric',
		year: 'numeric'
	});

	// Entity display config
	const entities = [
		{ key: 'people', label: 'People', route: '/person', icon: 'ri:user-line' },
		{ key: 'places', label: 'Places', route: '/place', icon: 'ri:map-pin-line' },
		{ key: 'orgs', label: 'Organizations', route: '/org', icon: 'ri:building-line' }
	] as const;
</script>

<div class="wiki-scroll-container">
<div class="wiki-page">
	<header class="page-header">
		<h1>Wiki</h1>
		<p class="page-subtitle">Your personal knowledge base</p>
	</header>

	<!-- Today context -->
	<div class="today-context">
		<p>
			Today's entry is
			<button onclick={() => navigateTo(`/day/day_${todaySlug}`)} class="today-link">
				{todayFormatted}
			</button>
		</p>
	</div>

	<!-- Activity Heatmap -->
	<section class="section heatmap-section">
		<h2>Activity</h2>
		{#if loadingActivity}
			<div class="heatmap-loading">
				<span class="loading-text">Loading activity...</span>
			</div>
		{:else}
			<ActivityHeatmap {activityData} onDayClick={handleDayClick} />
		{/if}
	</section>

	<hr class="divider" />

	<!-- Entities -->
	<section class="section">
		<h2>Entities</h2>
		<p class="section-description">
			The people, places, and organizations that appear in your data.
		</p>

		<div class="entity-grid">
			{#each entities as entity}
				{@const count = entityCounts[entity.key]}
				<button onclick={() => navigateTo(entity.route)} class="entity-card">
					<Icon icon={entity.icon} class="entity-icon"/>
					<span class="entity-label">{entity.label}</span>
					<span class="entity-count">{count}</span>
				</button>
			{/each}
		</div>
	</section>

	<hr class="divider" />

	<!-- Coming Soon -->
	<section class="section coming-soon-section">
		<div class="coming-soon-header">
			<Icon icon="ri:seedling-line" class="coming-soon-icon"/>
			<h2>What's Next</h2>
		</div>

		<p class="coming-soon-intro">
			The wiki is growing. Here's what we're building:
		</p>

		<div class="feature-list">
			<div class="feature-item">
				<div class="feature-title">
					<Icon icon="ri:book-open-line"/>
					<span>Narrative Structure</span>
				</div>
				<p class="feature-description">
					Organize your life into acts and chapters. Define the major seasons of your story and the arcs within them.
				</p>
			</div>

			<div class="feature-item">
				<div class="feature-title">
					<Icon icon="ri:calendar-line"/>
					<span>Temporal View</span>
				</div>
				<p class="feature-description">
					Browse by year, month, or day. See what happened when, with automatic journaling from your connected sources.
				</p>
			</div>

			<div class="feature-item">
				<div class="feature-title">
					<Icon icon="ri:links-line"/>
					<span>Entity Resolution</span>
				</div>
				<p class="feature-description">
					As you connect more data sources, we'll automatically identify and link people, places, and things across your life.
				</p>
			</div>

			<div class="feature-item">
				<div class="feature-title">
					<Icon icon="ri:edit-line"/>
					<span>AI-Assisted Journaling</span>
				</div>
				<p class="feature-description">
					Daily summaries generated from your data, ready for you to review and personalize. Your story, written with you.
				</p>
			</div>
		</div>
	</section>
</div>
</div>

<style>
	.wiki-scroll-container {
		height: 100%;
		overflow-y: auto;
	}

	.wiki-page {
		max-width: 42rem;
		margin: 0 auto;
		padding: 2.5rem 2rem;
	}

	/* Header */
	.page-header {
		margin-bottom: 1.5rem;
	}

	.page-header h1 {
		font-family: var(--font-serif, Georgia, serif);
		font-size: 2rem;
		font-weight: 400;
		color: var(--color-foreground);
		margin: 0;
		letter-spacing: -0.02em;
	}

	.page-subtitle {
		font-size: 0.9375rem;
		color: var(--color-foreground-muted);
		margin: 0.25rem 0 0 0;
	}

	/* Today context */
	.today-context {
		margin-bottom: 2rem;
	}

	.today-context p {
		font-size: 1rem;
		color: var(--color-foreground-muted);
		margin: 0;
	}

	.today-link {
		color: var(--color-primary);
		background: none;
		border: none;
		padding: 0;
		font: inherit;
		font-weight: 500;
		cursor: pointer;
		text-decoration: none;
		transition: opacity 0.15s ease;
	}

	.today-link:hover {
		opacity: 0.8;
		text-decoration: underline;
	}

	/* Sections */
	.section {
		margin-bottom: 2rem;
	}

	.section h2 {
		font-family: var(--font-serif, Georgia, serif);
		font-size: 1.25rem;
		font-weight: 400;
		color: var(--color-foreground);
		margin: 0 0 0.75rem 0;
	}

	.section-description {
		font-size: 0.875rem;
		color: var(--color-foreground-muted);
		margin: 0 0 1rem 0;
		line-height: 1.5;
	}

	.heatmap-section {
		margin-bottom: 1.5rem;
	}

	.heatmap-loading {
		padding: 2rem;
		text-align: center;
	}

	.loading-text {
		font-size: 0.875rem;
		color: var(--color-foreground-subtle);
	}

	/* Divider */
	.divider {
		border: none;
		border-top: 1px solid var(--color-border);
		margin: 1.5rem 0;
	}

	/* Entity grid */
	.entity-grid {
		display: grid;
		grid-template-columns: repeat(2, 1fr);
		gap: 0.75rem;
	}

	.entity-card {
		display: flex;
		align-items: center;
		gap: 0.75rem;
		padding: 1rem;
		background: var(--color-surface-elevated);
		border: 1px solid var(--color-border);
		border-radius: 8px;
		cursor: pointer;
		transition: all 0.15s ease;
		text-align: left;
		font: inherit;
	}

	.entity-card:hover {
		border-color: var(--color-border-subtle);
		background: var(--color-surface-hover);
	}

	.entity-icon {
		font-size: 1.25rem;
		color: var(--color-foreground-muted);
	}

	.entity-label {
		flex: 1;
		font-size: 0.9375rem;
		font-weight: 500;
		color: var(--color-foreground);
	}

	.entity-count {
		font-size: 0.875rem;
		color: var(--color-foreground-subtle);
		font-variant-numeric: tabular-nums;
	}

	/* Coming soon section */
	.coming-soon-section {
		margin-top: 2rem;
	}

	.coming-soon-header {
		display: flex;
		align-items: center;
		gap: 0.5rem;
		margin-bottom: 0.75rem;
	}

	.coming-soon-header h2 {
		margin: 0;
	}

	.coming-soon-icon {
		font-size: 1.25rem;
		color: var(--color-success);
	}

	.coming-soon-intro {
		font-size: 0.9375rem;
		color: var(--color-foreground-muted);
		margin: 0 0 1.25rem 0;
		line-height: 1.5;
	}

	.feature-list {
		display: flex;
		flex-direction: column;
		gap: 1rem;
	}

	.feature-item {
		padding: 1rem;
		background: var(--color-surface-elevated);
		border: 1px solid var(--color-border);
		border-radius: 8px;
	}

	.feature-title {
		display: flex;
		align-items: center;
		gap: 0.5rem;
		margin-bottom: 0.5rem;
	}

	.feature-title :global(svg) {
		font-size: 1rem;
		color: var(--color-foreground-muted);
	}

	.feature-title span {
		font-size: 0.9375rem;
		font-weight: 500;
		color: var(--color-foreground);
	}

	.feature-description {
		font-size: 0.8125rem;
		color: var(--color-foreground-muted);
		margin: 0;
		line-height: 1.5;
	}

	/* Responsive */
	@media (max-width: 640px) {
		.wiki-page {
			padding: 1.5rem;
		}

		.entity-grid {
			grid-template-columns: 1fr;
		}
	}
</style>
