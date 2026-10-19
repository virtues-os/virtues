<!--
	MessageFile.svelte

	A file inside a message: an image (click to open in the lightbox), an
	audio player, or a download link for anything else. User turns show
	their attachments `compact`; an assistant's generated image is shown at
	full size through the same element.
-->
<script lang="ts">
	import Icon from "$lib/components/Icon.svelte";

	let {
		part,
		compact = false,
		onOpenImage,
	}: {
		part: { mediaType?: string; url: string; filename?: string };
		compact?: boolean;
		/** Open an image in the shared-element lightbox, from the clicked element. */
		onOpenImage: (e: MouseEvent, src: string, alt: string) => void;
	} = $props();

	const mt = $derived(part.mediaType || "");
</script>

{#if mt.startsWith("image/")}
	<!-- svelte-ignore a11y_click_events_have_key_events a11y_no_noninteractive_element_interactions -->
	<img
		src={part.url}
		alt={part.filename || "image"}
		class="msg-image"
		class:compact-img={compact}
		onclick={(e) => onOpenImage(e, part.url, part.filename || "image")}
	/>
{:else if mt.startsWith("audio/")}
	<audio src={part.url} controls class="msg-audio"></audio>
{:else}
	<a class="msg-file" href={part.url} download={part.filename || "file"}>
		<Icon icon={mt === "application/pdf" ? "ri:file-pdf-fill" : "ri:file-text-line"} width="16" />
		<span>{part.filename || "Document"}</span>
	</a>
{/if}

<style>
	.msg-image {
		max-width: min(420px, 100%);
		max-height: 420px;
		border-radius: 0.75rem;
		border: 1px solid var(--color-border-subtle);
		display: block;
		cursor: zoom-in;
	}

	/* User-attached images render as compact thumbnails; assistant and
	   generated images keep the larger size. */
	.msg-image.compact-img {
		max-width: min(260px, 100%);
		max-height: 260px;
	}

	.msg-audio {
		width: min(420px, 100%);
	}

	.msg-file {
		display: inline-flex;
		align-items: center;
		gap: 0.375rem;
		padding: 0.5rem 0.75rem;
		border: 1px solid var(--color-border-subtle);
		border-radius: 0.625rem;
		background: var(--color-surface-elevated);
		font-size: 0.875rem;
		color: var(--color-foreground);
		text-decoration: none;
	}

	.msg-file:hover {
		border-color: var(--color-border-strong);
	}
</style>
