<script lang="ts">
	// Shared reference body — per-type content, no shell. Used by both RefPreview
	// (floating, on hover) and RefEmbed (block, in the document) so the two
	// densities never diverge. Fetches a normalized summary for entity targets;
	// files/links render from what the ref already carries.
	//
	// A `!` embed transcludes the RICH information of the thing, so this is
	// image-forward and shows the full fact set (never capped). Three layouts,
	// chosen by what the thing actually has:
	//   • figure   — scene/content media as a full-width hero + caption below:
	//                a place's map, a file image, a place/thing cover photo.
	//   • portrait — an identity image beside the facts: a person's photo (round)
	//                or an org's logo (rounded square).
	//   • line     — no image: a quiet glyph + title + facts.
	// Titles inherit the page font (--editor-font-family) at normal weight;
	// captions are quiet natural case. Full facts shown; deeper transclusion
	// (a page's text, an org's people) is a later step needing new data.
	import Icon from "$lib/components/Icon.svelte";
	import MovementMap from "$lib/components/timeline/MovementMap.svelte";
	import { refIcon } from "$lib/utils/refRoutes";
	import { getRefSummary, type RefSummary } from "$lib/utils/refSummary";

	let { type, label, url, mimeType } = $props<{
		type: string | null;
		label: string;
		url?: string;
		mimeType?: string;
	}>();

	const ENTITY_TYPES = new Set(["person", "place", "org"]);
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
	const fileImageUrl = $derived(
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
		person: "Person", place: "Place", org: "Organization",
		page: "Page", chat: "Chat", space: "Space", file: "File",
		day: "Day", year: "Year", source: "Source", link: "Link",
		record: "Record",
	};
	const typeLabel = $derived((type && TYPE_LABEL[type]) || "Reference");

	const title = $derived(summary?.name || label);
	const cover = $derived(summary?.avatarUrl || "");

	// Caption: type/domain, then address, then every fact value we fetched.
	// Quiet, natural case; may wrap to a second line.
	const meta = $derived(
		[
			domain || typeLabel,
			summary?.address,
			...(summary?.facts?.map((f) => f.value) ?? []),
		]
			.filter(Boolean)
			.join(" · "),
	);

	const hasMap = $derived(type === "place" && !!summary?.coords);
	// Scene/content media → full-width figure: a file image, or a place
	// cover photo (when there's no map to show instead).
	const figureImage = $derived(
		fileImageUrl || (!hasMap && cover && type === "place" ? cover : ""),
	);

	// Monogram fallback so an image-less person/org still has a visual anchor
	// with real mass (Gmail/Notion style) instead of a floating glyph.
	function initials(name: string): string {
		const parts = name.trim().split(/\s+/).filter(Boolean);
		if (!parts.length) return "?";
		if (parts.length === 1) return parts[0].slice(0, 2).toUpperCase();
		return (parts[0][0] + parts[parts.length - 1][0]).toUpperCase();
	}
	const usesMonogram = $derived(!cover && (type === "person" || type === "org"));
</script>

{#if hasMap && summary?.coords}
	<figure class="ref-card ref-figure">
		<div class="ref-figure-media ref-figure-map">
			<MovementMap
				stops={[{ lat: summary.coords.lat, lng: summary.coords.lng, label: title }]}
				height={150}
				zoom={13}
				interactive={false}
			/>
		</div>
		<figcaption class="ref-caption">
			<span class="ref-title">{title}</span>
			<span class="ref-meta">{meta}</span>
		</figcaption>
	</figure>
{:else if figureImage}
	<figure class="ref-card ref-figure">
		<div class="ref-figure-media"><img src={figureImage} alt={title} loading="lazy" /></div>
		<figcaption class="ref-caption">
			<span class="ref-title">{title}</span>
			<span class="ref-meta">{meta}</span>
		</figcaption>
	</figure>
{:else}
	<div class="ref-card ref-line">
		{#if cover}
			<img class="ref-anchor ref-photo" class:round={type === "person"} src={cover} alt="" />
		{:else if usesMonogram}
			<span class="ref-anchor ref-monogram" class:round={type === "person"}>{initials(title)}</span>
		{:else}
			<span class="ref-anchor ref-tile">
				<Icon icon={refIcon(type, { mimeType, filename: label })} width="18" />
			</span>
		{/if}
		<div class="ref-line-text">
			<span class="ref-title">{title}</span>
			<span class="ref-meta ref-meta-wrap">{meta}</span>
		</div>
	</div>
{/if}

<style>
	/* Title belongs to the writing — inherits the reader's page font at normal
	   weight; never bold, never a hardcoded sans. */
	.ref-title {
		font-family: var(--editor-font-family, var(--font-serif));
		font-size: 1.0625rem;
		font-weight: 400;
		color: var(--color-foreground);
		line-height: 1.25;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	/* Caption — quiet natural case in the UI sans. */
	.ref-meta {
		font-family: var(--font-sans);
		font-size: 0.75rem;
		font-weight: 400;
		color: var(--color-foreground-subtle);
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	/* In the row layouts the caption may wrap to a second line (more facts). */
	.ref-meta-wrap {
		white-space: normal;
		display: -webkit-box;
		-webkit-line-clamp: 2;
		line-clamp: 2;
		-webkit-box-orient: vertical;
	}

	/* ---- Figure (map / image / cover): media full column-width, caption below ---- */
	.ref-figure {
		margin: 0;
		display: flex;
		flex-direction: column;
	}
	.ref-figure-media {
		width: 100%;
		border-radius: 6px;
		overflow: hidden;
		background: var(--color-surface-sunken);
	}
	.ref-figure-media img {
		display: block;
		width: 100%;
		/* Fixed height so the block reserves its space at first paint (before the
		   image loads) — avoids the reflow that mis-positions lines below it. */
		height: 200px;
		object-fit: cover;
	}
	.ref-figure-map {
		pointer-events: none;
	}
	.ref-caption {
		display: flex;
		flex-direction: column;
		gap: 2px;
		margin-top: 8px;
	}

	/* ---- Line (person / org / page / thing without scene media) ----
	   Every row carries a 44px visual anchor: a photo/logo if we have one, else
	   a monogram (person/org) or a filled glyph tile — so nothing is a bare,
	   weightless icon. */
	.ref-line {
		display: flex;
		align-items: center;
		gap: 12px;
	}
	.ref-anchor {
		width: 44px;
		height: 44px;
		border-radius: 9px;
		flex-shrink: 0;
	}
	.ref-anchor.round {
		border-radius: 50%;
	}
	.ref-photo {
		object-fit: cover;
	}
	.ref-monogram {
		display: inline-flex;
		align-items: center;
		justify-content: center;
		background: var(--color-surface-sunken);
		color: var(--color-foreground-muted);
		font-family: var(--font-sans);
		font-size: 0.9375rem;
		font-weight: 500;
		letter-spacing: 0.02em;
		user-select: none;
	}
	.ref-tile {
		display: inline-flex;
		align-items: center;
		justify-content: center;
		background: var(--color-surface-sunken);
		color: var(--color-foreground-subtle);
	}
	.ref-line-text {
		flex: 1;
		min-width: 0;
		display: flex;
		flex-direction: column;
		gap: 3px;
	}
</style>
