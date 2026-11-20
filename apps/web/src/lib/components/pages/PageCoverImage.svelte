<script lang="ts">
	import Icon from "$lib/components/Icon.svelte";
	import { backendUrl } from "$lib/config/backend";
	import { PAGE_WIDTH_CSS, type PageWidthChoice } from "$lib/stores/pageWidth.svelte";
	import type { PagePaper } from "$lib/api/client";

	interface Props {
		coverUrl: string;
		/** The page's width, so the cover is as wide as the text under it. */
		width: PageWidthChoice;
		/** On paper the text is the printed sheet, so the cover is too. */
		paper?: PagePaper;
		onChangeCover: () => void;
		onRemoveCover: () => void;
	}

	let { coverUrl, width, paper = "screen", onChangeCover, onRemoveCover }: Props = $props();

	const sheet = $derived(width === "sheet" || paper !== "screen");

	let coverHover = $state(false);
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div
	class="cover-image-wrapper"
	class:width-sheet={sheet}
	class:paper-letter={paper === "letter"}
	style:max-width={sheet ? undefined : PAGE_WIDTH_CSS[width]}
	onmouseenter={() => (coverHover = true)}
	onmouseleave={() => (coverHover = false)}
>
	<div
		class="cover-image"
		style="background-image: url({backendUrl(coverUrl)})"
	></div>
	{#if coverHover}
		<div class="cover-overlay">
			<button
				class="cover-overlay-btn"
				onclick={onChangeCover}
			>
				<Icon
					icon="mynaui:image"
					width="14"
				/>
				Change cover
			</button>
			<button
				class="cover-overlay-btn cover-overlay-btn-danger"
				onclick={onRemoveCover}
			>
				<Icon icon="mynaui:x" width="14" />
				Remove
			</button>
		</div>
	{/if}
</div>

<style>
	.cover-image-wrapper {
		position: relative;
		margin-left: auto;
		margin-right: auto;
		margin-bottom: 1.5rem;
		border-radius: var(--radius-lg);
		overflow: hidden;
		transition: max-width 0.2s ease-out;
	}

	/* As wide as the A4 sheet below it (`.page-inner.width-sheet`), or the
	   US Letter one on Letter paper. */
	.cover-image-wrapper.width-sheet {
		max-width: 210mm;
	}

	.cover-image-wrapper.width-sheet.paper-letter {
		max-width: 8.5in;
	}

	.cover-image {
		width: 100%;
		aspect-ratio: 3 / 1;
		background-size: cover;
		background-position: center;
	}

	.cover-overlay {
		position: absolute;
		bottom: 0;
		right: 0;
		display: flex;
		gap: 4px;
		padding: 8px;
		animation: cover-fade-in 100ms ease-out;
	}

	@keyframes cover-fade-in {
		from {
			opacity: 0;
		}
		to {
			opacity: 1;
		}
	}

	.cover-overlay-btn {
		display: flex;
		align-items: center;
		gap: 6px;
		padding: 6px 10px;
		font-size: 12px;
		font-weight: 500;
		color: white;
		background: rgba(0, 0, 0, 0.6);
		backdrop-filter: blur(4px);
		border: none;
		border-radius: var(--radius-md);
		cursor: pointer;
		transition: background 150ms;
	}

	.cover-overlay-btn:hover {
		background: rgba(0, 0, 0, 0.8);
	}

	.cover-overlay-btn-danger:hover {
		background: color-mix(in srgb, var(--color-error) 80%, transparent);
	}
</style>
