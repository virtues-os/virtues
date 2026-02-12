<script lang="ts">
	import Icon from "$lib/components/Icon.svelte";

	type WidthMode = "small" | "medium" | "full";

	interface Props {
		coverUrl: string;
		widthMode: WidthMode;
		onChangeCover: () => void;
		onRemoveCover: () => void;
	}

	let { coverUrl, widthMode, onChangeCover, onRemoveCover }: Props = $props();

	let coverHover = $state(false);
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div
	class="cover-image-wrapper"
	class:width-small={widthMode === "small"}
	class:width-medium={widthMode === "medium"}
	class:width-full={widthMode === "full"}
	onmouseenter={() => (coverHover = true)}
	onmouseleave={() => (coverHover = false)}
>
	<div
		class="cover-image"
		style="background-image: url({coverUrl})"
	></div>
	{#if coverHover}
		<div class="cover-overlay">
			<button
				class="cover-overlay-btn"
				onclick={onChangeCover}
			>
				<Icon
					icon="ri:image-edit-line"
					width="14"
				/>
				Change cover
			</button>
			<button
				class="cover-overlay-btn cover-overlay-btn-danger"
				onclick={onRemoveCover}
			>
				<Icon icon="ri:close-line" width="14" />
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
		border-radius: 0.5rem;
		overflow: hidden;
		transition: max-width 0.2s ease-out;
	}

	.cover-image-wrapper.width-small {
		max-width: 32rem;
	}

	.cover-image-wrapper.width-medium {
		max-width: 42rem;
	}

	.cover-image-wrapper.width-full {
		max-width: 100%;
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
		border-radius: 6px;
		cursor: pointer;
		transition: background 150ms;
	}

	.cover-overlay-btn:hover {
		background: rgba(0, 0, 0, 0.8);
	}

	.cover-overlay-btn-danger:hover {
		background: rgba(220, 38, 38, 0.8);
	}
</style>
