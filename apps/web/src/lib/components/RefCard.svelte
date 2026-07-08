<script lang="ts">
	// Shared reference-card body — per-type content, no shell. Used by both
	// RefPreview (floating, on hover) and RefEmbed (block, in the document) so the
	// two densities never diverge. Fetches a normalized summary for entity targets;
	// files/links render from what the ref already carries.
	import Icon from "$lib/components/Icon.svelte";
	import { refIcon } from "$lib/utils/refRoutes";
	import { getRefSummary, type RefSummary } from "$lib/utils/refSummary";

	let { type, label, url, mimeType } = $props<{
		type: string | null;
		label: string;
		url?: string;
		mimeType?: string;
	}>();

	const ENTITY_TYPES = new Set(["person", "place", "org", "thing"]);
	const id = $derived(url ? url.slice(url.lastIndexOf("/") + 1) : "");

	let summary = $state<RefSummary | null>(null);
	$effect(() => {
		summary = null;
		if (type && ENTITY_TYPES.has(type) && id) {
			const t = type;
			const wanted = id;
			getRefSummary(t, wanted).then((s) => {
				// Guard against a stale resolve after props changed.
				if (id === wanted) summary = s;
			});
		}
	});

	const IMAGE_EXT = /\.(jpe?g|png|gif|webp|svg|bmp|ico|heic)$/i;
	const isImage = $derived((mimeType?.startsWith("image/") ?? false) || IMAGE_EXT.test(label));
	const thumbnailUrl = $derived(
		type === "file" && isImage && id ? `/api/drive/files/${id}/download` : "",
	);

	function domainOf(u?: string): string {
		if (!u) return "";
		try {
			return new URL(u).hostname.replace("www.", "");
		} catch {
			return "";
		}
	}
	const domain = $derived(type === "link" ? domainOf(url) : "");

	const TYPE_LABEL: Record<string, string> = {
		person: "Person", place: "Place", org: "Organization", thing: "Thing",
		page: "Page", chat: "Chat", space: "Space", file: "File",
		day: "Day", year: "Year", source: "Source", link: "Link",
	};
	const typeLabel = $derived((type && TYPE_LABEL[type]) || "Reference");

	const title = $derived(summary?.name || label);
	// Meta line = type (or the site domain for links). The per-type facts below
	// already carry relationship/category, so we don't repeat it as a subtitle.
	const meta = $derived(domain || typeLabel);
	const avatar = $derived(summary?.avatarUrl || "");
</script>

<div class="ref-card">
	{#if thumbnailUrl}
		<div class="ref-card-media"><img src={thumbnailUrl} alt={title} loading="lazy" /></div>
	{/if}

	{#if type === "place" && summary?.coords}
		<!-- Schematic location. Real map imagery (warmed at resolution, box-proxied,
		     cached) is a later slice — this stays local + offline. -->
		<div class="ref-card-map" title="{summary.coords.lat}, {summary.coords.lng}">
			<Icon icon="ri:map-pin-2-fill" width="20" />
			<span class="ref-card-coords">
				{summary.coords.lat.toFixed(3)}, {summary.coords.lng.toFixed(3)}
			</span>
		</div>
	{/if}

	<div class="ref-card-body">
		<div class="ref-card-header">
			{#if avatar}
				<img class="ref-card-avatar" src={avatar} alt="" />
			{:else}
				<Icon icon={refIcon(type, { mimeType, filename: label })} width="15" />
			{/if}
			<span class="ref-card-title">{title}</span>
		</div>

		<div class="ref-card-meta">{meta}</div>

		{#if summary?.address}
			<div class="ref-card-address">{summary.address}</div>
		{/if}

		{#if summary?.facts?.length}
			<dl class="ref-card-facts">
				{#each summary.facts as f}
					<div class="ref-card-fact">
						<dt>{f.label}</dt>
						<dd>{f.value}</dd>
					</div>
				{/each}
			</dl>
		{/if}
	</div>
</div>

<style>
	.ref-card {
		display: flex;
		flex-direction: column;
		/* Reset to the UI font — mounted inside the serif prose editor, the card
		   would otherwise inherit large serif type. */
		font-family: var(--font-sans, ui-sans-serif, system-ui, sans-serif);
		font-size: 0.8125rem;
		line-height: 1.4;
	}

	.ref-card-media {
		width: 100%;
		max-height: 150px;
		overflow: hidden;
		background: var(--color-surface-sunken, var(--color-border));
	}
	.ref-card-media img {
		display: block;
		width: 100%;
		max-height: 150px;
		object-fit: cover;
	}

	.ref-card-map {
		display: flex;
		align-items: center;
		gap: 6px;
		padding: 10px 12px;
		color: var(--color-foreground-muted);
		background: var(--color-surface-sunken, var(--color-border));
		border-bottom: 1px solid var(--color-border);
	}
	.ref-card-coords {
		font-size: 0.6875rem;
		font-variant-numeric: tabular-nums;
	}

	.ref-card-body {
		padding: 8px 10px;
	}

	.ref-card-header {
		display: flex;
		align-items: center;
		gap: 6px;
		color: var(--color-foreground);
	}
	.ref-card-avatar {
		width: 20px;
		height: 20px;
		border-radius: 50%;
		object-fit: cover;
		flex-shrink: 0;
	}
	.ref-card-title {
		font-weight: 500;
		font-size: 0.8125rem;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.ref-card-meta {
		margin-top: 2px;
		font-size: 0.6875rem;
		color: var(--color-foreground-subtle);
	}
	.ref-card-address {
		margin-top: 4px;
		font-size: 0.6875rem;
		color: var(--color-foreground-muted);
	}

	.ref-card-facts {
		margin: 6px 0 0;
		display: flex;
		flex-direction: column;
		gap: 2px;
	}
	.ref-card-fact {
		display: flex;
		justify-content: space-between;
		gap: 12px;
		font-size: 0.6875rem;
	}
	.ref-card-fact dt {
		color: var(--color-foreground-subtle);
	}
	.ref-card-fact dd {
		margin: 0;
		color: var(--color-foreground);
		text-align: right;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
</style>
