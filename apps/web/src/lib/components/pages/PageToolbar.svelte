<script lang="ts">
	import Icon from "$lib/components/Icon.svelte";
	import IconPicker from "$lib/components/IconPicker.svelte";
	import CoverImagePicker from "$lib/components/CoverImagePicker.svelte";
	import { VersionHistoryPanel } from "$lib/components/pages";
	import { Popover } from "$lib/floating";
	import type { YjsDocument } from "$lib/yjs";

	type WidthMode = "small" | "medium" | "full";

	interface Props {
		icon: string | null;
		coverUrl: string | null;
		widthMode: WidthMode;
		showDragHandles: boolean;
		copied: boolean;
		pageId: string;
		yjsDoc: YjsDocument | undefined;
		showCoverPicker?: boolean;
		onIconSelect: (value: string | null) => void;
		onCoverSelect: (url: string | null) => void;
		onWidthCycle: () => void;
		onToggleDragHandles: () => void;
		onCopyMarkdown: () => void;
		onDelete: () => void;
	}

	let {
		icon,
		coverUrl,
		widthMode,
		showDragHandles,
		copied,
		pageId,
		yjsDoc,
		showCoverPicker = $bindable(false),
		onIconSelect,
		onCoverSelect,
		onWidthCycle,
		onToggleDragHandles,
		onCopyMarkdown,
		onDelete,
	}: Props = $props();

	let showIconPicker = $state(false);
	let showVersionHistory = $state(false);
	let showDeleteConfirm = $state(false);
</script>

<div class="page-toolbar">
	<div class="toolbar-spacer"></div>
	<Popover bind:open={showIconPicker} placement="bottom-start">
		{#snippet trigger({ toggle })}
			<button
				onclick={toggle}
				class="toolbar-action"
				title={icon ? "Change icon" : "Add icon"}
			>
				{#if icon}
					{#if icon.includes(":")}
						<Icon {icon} width="15" />
					{:else}
						<span class="toolbar-emoji">{icon}</span>
					{/if}
				{:else}
					<Icon icon="ri:emotion-line" width="15" />
				{/if}
			</button>
		{/snippet}
		{#snippet children({ close })}
			<IconPicker
				value={icon}
				onSelect={(value) => {
					onIconSelect(value);
				}}
				{close}
			/>
		{/snippet}
	</Popover>
	<Popover bind:open={showCoverPicker} placement="bottom-start">
		{#snippet trigger({ toggle })}
			<button
				onclick={toggle}
				class="toolbar-action"
				title={coverUrl ? "Change cover" : "Add cover"}
			>
				<Icon
					icon={coverUrl
						? "ri:image-edit-line"
						: "ri:image-line"}
					width="15"
				/>
			</button>
		{/snippet}
		{#snippet children({ close })}
			<CoverImagePicker
				value={coverUrl}
				onSelect={(url) => {
					onCoverSelect(url);
				}}
				{close}
			/>
		{/snippet}
	</Popover>
	<button
		onclick={onWidthCycle}
		class="toolbar-action"
		title={widthMode === "small"
			? "Small width"
			: widthMode === "medium"
				? "Medium width"
				: "Full width"}
	>
		<Icon
			icon={widthMode === "small"
				? "ri:contract-left-right-line"
				: widthMode === "medium"
					? "ri:pause-line"
					: "ri:expand-left-right-line"}
			width="15"
		/>
	</button>
	<button
		onclick={onToggleDragHandles}
		class="toolbar-action"
		class:active={showDragHandles}
		title={showDragHandles
			? "Hide line numbers"
			: "Show line numbers"}
	>
		<Icon icon="ri:hashtag" width="15" />
	</button>
	<button
		onclick={onCopyMarkdown}
		class="toolbar-action"
		class:active={copied}
		title={copied ? "Copied!" : "Copy as Markdown"}
	>
		<Icon
			icon={copied ? "ri:check-line" : "ri:file-copy-line"}
			width="15"
		/>
	</button>
	<Popover bind:open={showVersionHistory} placement="bottom-end">
		{#snippet trigger({ toggle })}
			<button
				onclick={toggle}
				class="toolbar-action"
				title="Version history"
			>
				<Icon icon="ri:history-line" width="15" />
			</button>
		{/snippet}
		{#snippet children({ close })}
			<VersionHistoryPanel {close} {pageId} {yjsDoc} />
		{/snippet}
	</Popover>
	<div class="toolbar-divider"></div>
	<Popover bind:open={showDeleteConfirm} placement="bottom-end">
		{#snippet trigger({ toggle })}
			<button
				onclick={toggle}
				class="toolbar-action toolbar-action-danger"
				title="Delete page"
			>
				<Icon icon="ri:delete-bin-line" width="15" />
			</button>
		{/snippet}
		{#snippet children({ close })}
			<div class="delete-confirm">
				<p class="delete-confirm-text">Delete this page?</p>
				<div class="delete-confirm-actions">
					<button
						class="delete-confirm-btn delete-confirm-cancel"
						onclick={close}
					>
						Cancel
					</button>
					<button
						class="delete-confirm-btn delete-confirm-delete"
						onclick={onDelete}
					>
						Delete
					</button>
				</div>
			</div>
		{/snippet}
	</Popover>
</div>

<style>
	.page-toolbar {
		display: flex;
		align-items: center;
		gap: 4px;
		padding: 4px 12px;
		background: var(--color-background);
		border-bottom: 1px solid var(--color-border);
		flex-shrink: 0;
	}

	.toolbar-spacer {
		flex: 1;
	}

	.toolbar-action {
		display: flex;
		align-items: center;
		justify-content: center;
		padding: 6px;
		border: none;
		background: none;
		color: var(--color-foreground-muted);
		cursor: pointer;
		border-radius: 4px;
		transition: all 0.15s ease;
	}

	.toolbar-action:hover {
		color: var(--color-foreground);
		background: var(--color-surface-elevated);
	}

	.toolbar-action.active {
		color: var(--color-primary);
	}

	.toolbar-action-danger:hover {
		color: var(--color-error);
	}

	.toolbar-emoji {
		font-size: 14px;
		line-height: 1;
	}

	.toolbar-divider {
		width: 1px;
		height: 16px;
		background: var(--color-border);
		margin: 0 2px;
	}

	.delete-confirm {
		padding: 12px;
		min-width: 180px;
	}

	.delete-confirm-text {
		margin: 0 0 12px 0;
		font-size: 13px;
		font-weight: 500;
		color: var(--color-foreground);
	}

	.delete-confirm-actions {
		display: flex;
		gap: 8px;
		justify-content: flex-end;
	}

	.delete-confirm-btn {
		padding: 6px 12px;
		font-size: 12px;
		font-weight: 500;
		border: none;
		border-radius: 6px;
		cursor: pointer;
		transition: all 0.15s ease;
	}

	.delete-confirm-cancel {
		background: var(--color-surface-elevated);
		color: var(--color-foreground-muted);
	}

	.delete-confirm-cancel:hover {
		background: var(--color-border);
		color: var(--color-foreground);
	}

	.delete-confirm-delete {
		background: var(--color-error);
		color: white;
	}

	.delete-confirm-delete:hover {
		filter: brightness(1.1);
	}
</style>
