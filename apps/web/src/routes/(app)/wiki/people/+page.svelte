<script lang="ts">
	import { PersonTable } from "$lib/components/wiki";
</script>

<div class="people-page">
	<header class="page-header">
		<h1>People</h1>
	</header>

	<PersonTable />
</div>

<style>
	.people-page {
		width: 100%;
		padding: 1.5rem 0;
		height: 100%;
		overflow-y: auto;
	}

	.page-header {
		margin-bottom: 1rem;
		padding: 0 2rem;
	}

	.page-header h1 {
		font-family: var(--font-serif, Georgia, serif);
		font-size: 2rem;
		font-weight: 400;
		color: var(--color-foreground);
		margin: 0;
		letter-spacing: -0.02em;
	}
</style>
