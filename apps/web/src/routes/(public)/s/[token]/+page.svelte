<script lang="ts">
	import { page } from "$app/stores";
	import { onMount } from "svelte";
	import Icon from "$lib/components/Icon.svelte";
	import PublicPageViewer from "$lib/components/pages/PublicPageViewer.svelte";
	import type { SharedPage } from "$lib/api/client";

	let loading = $state(true);
	let error = $state<string | null>(null);
	let sharedPage = $state<SharedPage | null>(null);

	const token = $derived(($page.params as Record<string, string>).token);

	onMount(async () => {
		try {
			const res = await fetch(`/api/s/${token}`);
			if (!res.ok) {
				error = res.status === 404 ? "This page doesn't exist or is no longer shared." : "Failed to load page.";
				return;
			}
			const data: SharedPage = await res.json();

			// Rewrite media URLs: /api/drive/files/:id/download -> /api/s/:token/files/:id
			data.content = data.content.replace(
				/\/api\/drive\/files\/([^/]+)\/download/g,
				`/api/s/${token}/files/$1`
			);
			if (data.cover_url) {
				data.cover_url = data.cover_url.replace(
					/\/api\/drive\/files\/([^/]+)\/download/g,
					`/api/s/${token}/files/$1`
				);
			}

			sharedPage = data;
		} catch {
			error = "Failed to load page.";
		} finally {
			loading = false;
		}
	});
</script>

<svelte:head>
	{#if sharedPage}
		<title>{sharedPage.title}</title>
	{:else}
		<title>Shared Page</title>
	{/if}
</svelte:head>

<div class="shared-page">
	{#if loading}
		<div class="shared-page-loading">
			<Icon icon="ri:loader-4-line" width="24" class="spin" />
		</div>
	{:else if error}
		<div class="shared-page-error">
			<Icon icon="ri:file-unknow-line" width="48" />
			<p>{error}</p>
		</div>
	{:else if sharedPage}
		{#if sharedPage.cover_url}
			<div class="shared-page-cover">
				<img src={sharedPage.cover_url} alt="" class="shared-page-cover-img" />
			</div>
		{/if}
		<div class="shared-page-content">
			<div class="shared-page-header">
				{#if sharedPage.icon}
					{#if sharedPage.icon.includes(":")}
						<Icon icon={sharedPage.icon} width="32" />
					{:else}
						<span class="shared-page-icon">{sharedPage.icon}</span>
					{/if}
				{/if}
				<h1 class="shared-page-title">{sharedPage.title}</h1>
			</div>
			<PublicPageViewer markdown={sharedPage.content} />
		</div>
	{/if}
</div>

<style>
	.shared-page {
		max-width: 42rem;
		margin: 0 auto;
		padding: 2rem 1.5rem 4rem;
	}

	.shared-page-loading {
		display: flex;
		justify-content: center;
		padding: 4rem 0;
	}

	.shared-page-error {
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: 16px;
		padding: 4rem 0;
		color: var(--color-foreground-muted);
		text-align: center;
	}

	.shared-page-error p {
		font-size: 15px;
		max-width: 300px;
	}

	.shared-page-cover {
		width: 100%;
		max-height: 280px;
		overflow: hidden;
		border-radius: 8px;
		margin-bottom: 1rem;
	}

	.shared-page-cover-img {
		width: 100%;
		height: 100%;
		object-fit: cover;
	}

	.shared-page-content {
		margin-top: 1.5rem;
	}

	.shared-page-header {
		display: flex;
		align-items: flex-start;
		gap: 12px;
		margin-bottom: 1.5rem;
	}

	.shared-page-icon {
		font-size: 2rem;
		line-height: 1;
	}

	.shared-page-title {
		font-family: var(--font-serif, Georgia, serif);
		font-size: 2rem;
		font-weight: 500;
		line-height: 1.2;
		color: var(--color-foreground);
		margin: 0;
	}

	:global(.spin) {
		animation: spin 1s linear infinite;
	}

	@keyframes spin {
		from { transform: rotate(0deg); }
		to { transform: rotate(360deg); }
	}
</style>
