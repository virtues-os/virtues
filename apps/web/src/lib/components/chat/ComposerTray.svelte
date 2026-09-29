<!--
	ComposerTray.svelte

	What sits on top of the chat composer: the drop hint while a file is
	dragged over the chat, the staged attachments, the banner when the model
	cannot read one of them, and the messages queued behind a running turn.

	Renders no wrapper of its own on purpose. The drop hint is positioned
	against the composer wrapper that contains this tray (ChatView's
	`.chat-input-wrapper`), so a positioned element here would move it.
-->
<script lang="ts">
	import Icon from "$lib/components/Icon.svelte";
	import { formatFileSize, type Attachment } from "$lib/components/chat/state/attachments.svelte";
	import type { ModelChoiceController } from "$lib/components/chat/state/modelChoice.svelte";

	let {
		dragActive,
		attachments,
		onRemoveAttachment,
		capabilityIssue,
		onSwitchModel,
		queued,
		onRemoveQueued,
	}: {
		dragActive: boolean;
		attachments: Attachment[];
		onRemoveAttachment: (id: string) => void;
		capabilityIssue: ModelChoiceController["capabilityIssue"];
		onSwitchModel: () => void;
		/** Messages typed while a turn runs, sent in order when it finishes. */
		queued: string[];
		onRemoveQueued: (index: number) => void;
	} = $props();
</script>

{#if dragActive}
	<div class="drop-hint">
		<Icon icon="ri:download-2-line" width="15" />
		<span>Drop to attach &middot; images, PDFs, audio, or text</span>
	</div>
{/if}
{#if attachments.length > 0}
	<div class="attachments">
		{#each attachments as a (a.id)}
			<div class="attachment">
				{#if a.kind === "image"}
					<img src={a.url} alt={a.filename} class="attachment-thumb" />
				{:else}
					<span class="attachment-icon">
						<Icon
							icon={a.kind === "pdf"
								? "ri:file-pdf-fill"
								: a.kind === "audio"
									? "ri:music-2-line"
									: "ri:file-text-line"}
							width="18"
						/>
					</span>
				{/if}
				<div class="attachment-meta">
					<!-- An image carries only its size (VIR-237): a screenshot's
					     generated filename says nothing the thumbnail has not
					     already shown. Every other kind keeps its name, because a
					     type icon and a byte count cannot tell two PDFs apart. The
					     name still reaches assistive tech through the img alt. -->
					{#if a.kind !== "image"}
						<span class="attachment-name">{a.filename}</span>
					{/if}
					<span class="attachment-size">{formatFileSize(a.size)}</span>
				</div>
				<button
					type="button"
					class="attachment-remove"
					aria-label="Remove attachment"
					onclick={() => onRemoveAttachment(a.id)}
				>
					<Icon icon="ri:close-line" width="13" />
				</button>
			</div>
		{/each}
	</div>
{/if}
{#if capabilityIssue}
	<div class="capability-banner">
		<span>
			{capabilityIssue.modelName} can't read {capabilityIssue.lacks.join(" or ")}.
		</span>
		{#if capabilityIssue.candidate}
			<button type="button" class="capability-switch" onclick={onSwitchModel}>
				Switch to {capabilityIssue.candidate.displayName}
			</button>
		{:else}
			<span class="capability-none">No available model can read {capabilityIssue.lacks.join(" or ")} yet.</span>
		{/if}
	</div>
{/if}
{#if queued.length > 0}
	<div class="queued-messages">
		{#each queued as q, i (i)}
			<div class="queued-chip">
				<span class="queued-text">{q}</span>
				<button
					type="button"
					class="queued-remove"
					aria-label="Remove queued message"
					onclick={() => onRemoveQueued(i)}
				>
					<Icon icon="ri:close-line" width="13" />
				</button>
			</div>
		{/each}
	</div>
{/if}

<style>
	.queued-messages {
		display: flex;
		flex-direction: column;
		gap: 0.375rem;
		margin-bottom: 0.5rem;
	}

	.queued-chip {
		display: flex;
		align-items: center;
		gap: 0.5rem;
		padding: 0.375rem 0.625rem;
		border: 1px solid var(--color-border);
		border-radius: 0.625rem;
		background: var(--color-surface-elevated);
		font-size: 0.8125rem;
		color: var(--color-foreground-muted);
	}

	.queued-text {
		flex: 1;
		min-width: 0;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.queued-remove {
		flex-shrink: 0;
		display: inline-flex;
		align-items: center;
		justify-content: center;
		padding: 0.125rem;
		border-radius: 0.375rem;
		color: var(--color-foreground-muted);
		transition: background-color 0.15s ease;
	}

	.queued-remove:hover {
		background: var(--color-border);
	}

	.attachments {
		display: flex;
		flex-wrap: wrap;
		gap: 0.5rem;
		margin-bottom: 0.5rem;
	}

	.attachment {
		display: inline-flex;
		align-items: center;
		gap: 0.5rem;
		padding: 0.375rem 0.5rem 0.375rem 0.375rem;
		border: 1px solid var(--color-border-subtle);
		border-radius: 0.625rem;
		background: var(--color-surface-elevated);
		max-width: 15rem;
	}

	/* Four times the area of the old 2.25rem chip (VIR-238), which was too
	   small to tell one screenshot from another. Linear 4x (9rem) was the
	   other reading of the ticket and is far too tall — it would own the
	   composer. The icon below stays at 2.25rem: a file chip is identified by
	   its name, which it keeps, so it has nothing to gain from the height. */
	.attachment-thumb {
		width: 4.5rem;
		height: 4.5rem;
		border-radius: 0.4rem;
		object-fit: cover;
		flex-shrink: 0;
		display: block;
	}

	.attachment-icon {
		width: 2.25rem;
		height: 2.25rem;
		border-radius: 0.4rem;
		display: flex;
		align-items: center;
		justify-content: center;
		background: var(--color-surface);
		color: var(--color-foreground-muted);
		flex-shrink: 0;
	}

	.attachment-meta {
		display: flex;
		flex-direction: column;
		gap: 0.0625rem;
		min-width: 0;
	}

	.attachment-name {
		font-size: 0.8125rem;
		color: var(--color-foreground);
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.attachment-size {
		font-size: 0.6875rem;
		color: var(--color-foreground-subtle);
	}

	.attachment-remove {
		flex-shrink: 0;
		display: inline-flex;
		align-items: center;
		justify-content: center;
		padding: 0.125rem;
		border-radius: 0.375rem;
		color: var(--color-foreground-muted);
		transition: background-color 0.15s ease;
	}

	.attachment-remove:hover {
		background: var(--color-border);
	}

	.capability-banner {
		display: flex;
		align-items: center;
		flex-wrap: wrap;
		gap: 0.5rem;
		margin-bottom: 0.5rem;
		padding: 0.375rem 0.625rem;
		border: 1px solid var(--color-warning, var(--color-border));
		border-radius: 0.625rem;
		background: var(--color-warning-subtle, var(--color-surface-elevated));
		font-size: 0.8125rem;
		color: var(--color-foreground);
	}

	.capability-switch {
		color: var(--color-primary);
		font-weight: 500;
	}

	.capability-switch:hover {
		text-decoration: underline;
	}

	.capability-none {
		color: var(--color-foreground-muted);
	}

	/* The composer becomes the dropzone (no full-screen scrim — context stays
	   visible, the cue points at the exact landing spot). Drop still works
	   anywhere over the chat root. */
	.drop-hint {
		position: absolute;
		left: 50%;
		bottom: calc(100% - 0.5rem);
		transform: translateX(-50%);
		display: inline-flex;
		align-items: center;
		gap: 0.4rem;
		padding: 0.3rem 0.7rem;
		border-radius: var(--radius-full);
		background: var(--color-primary);
		color: var(--color-on-primary, #fff);
		font-size: 0.75rem;
		font-weight: 500;
		white-space: nowrap;
		box-shadow: 0 6px 18px -6px color-mix(in srgb, var(--color-primary) 60%, transparent);
		pointer-events: none;
		z-index: 11;
		animation: drop-hint-in 0.18s cubic-bezier(0.22, 1, 0.36, 1);
	}

	@keyframes drop-hint-in {
		from {
			opacity: 0;
			transform: translateX(-50%) translateY(0.35rem);
		}
		to {
			opacity: 1;
			transform: translateX(-50%) translateY(0);
		}
	}

	@media (prefers-reduced-motion: reduce) {
		.drop-hint {
			animation: none;
		}
	}
</style>
