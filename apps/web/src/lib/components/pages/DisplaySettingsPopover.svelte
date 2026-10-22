<script lang="ts">
	import Icon from "$lib/components/Icon.svelte";
	import {
		pageDisplay,
		type FontMode,
		type TextSize,
		type WidthMode,
	} from "$lib/stores/pageDisplay.svelte";

	interface Props {
		/** Open the page's markdown, read only; absent where the page already shows it. */
		onViewMarkdown?: () => void;
	}

	let { onViewMarkdown }: Props = $props();

	const fontModes: { value: FontMode; label: string }[] = [
		{ value: "sans", label: "Sans" },
		{ value: "serif", label: "Serif" },
		{ value: "mono", label: "Mono" },
	];

	const textSizes: { value: TextSize; label: string }[] = [
		{ value: "s", label: "S" },
		{ value: "m", label: "M" },
		{ value: "l", label: "L" },
	];

	const widthModes: { value: WidthMode; icon: string; label: string }[] = [
		{ value: "small", icon: "ri:contract-left-right-line", label: "Narrow" },
		{ value: "medium", icon: "ri:pause-line", label: "Medium" },
		{ value: "full", icon: "ri:expand-left-right-line", label: "Wide" },
		{ value: "page", icon: "ri:file-paper-2-line", label: "Page (A4)" },
	];
</script>

<div class="display-settings">
	<div class="row">
		<span class="row-label">Font</span>
		<div class="segmented">
			{#each fontModes as opt (opt.value)}
				<button
					class="seg"
					class:active={pageDisplay.fontMode === opt.value}
					style:font-family={opt.value === "serif"
						? "var(--font-serif-ui, Georgia, serif)"
						: opt.value === "mono"
							? "var(--font-mono, monospace)"
							: "var(--font-sans, sans-serif)"}
					onclick={() => pageDisplay.setFontMode(opt.value)}
				>
					{opt.label}
				</button>
			{/each}
		</div>
	</div>

	<div class="row">
		<span class="row-label">Size</span>
		<div class="segmented">
			{#each textSizes as opt (opt.value)}
				<button
					class="seg"
					class:active={pageDisplay.textSize === opt.value}
					onclick={() => pageDisplay.setTextSize(opt.value)}
				>
					{opt.label}
				</button>
			{/each}
		</div>
	</div>

	<div class="row">
		<span class="row-label">Width</span>
		<div class="segmented">
			{#each widthModes as opt (opt.value)}
				<button
					class="seg seg-icon"
					class:active={pageDisplay.widthMode === opt.value}
					title={opt.label}
					onclick={() => pageDisplay.setWidth(opt.value)}
				>
					<Icon icon={opt.icon} width="16" />
				</button>
			{/each}
		</div>
	</div>

	<div class="divider"></div>

	<button class="focus-toggle" aria-pressed={pageDisplay.focusMode} onclick={() => pageDisplay.toggleFocus()}>
		<Icon
			icon={pageDisplay.focusMode
				? "ri:focus-2-fill"
				: "ri:focus-2-line"}
			width="16"
		/>
		<span class="focus-label">Focus mode</span>
		<span class="focus-state" class:on={pageDisplay.focusMode}>
			{pageDisplay.focusMode ? "On" : "Off"}
		</span>
		<kbd class="focus-kbd">⌘⇧F</kbd>
	</button>

	<button class="focus-toggle" aria-pressed={pageDisplay.spellcheck} onclick={() => pageDisplay.toggleSpellcheck()}>
		<Icon icon={pageDisplay.spellcheck ? "ri:check-double-fill" : "ri:check-double-line"} width="16" />
		<span class="focus-label">Spell check</span>
		<span class="focus-state" class:on={pageDisplay.spellcheck}>
			{pageDisplay.spellcheck ? "On" : "Off"}
		</span>
	</button>

	{#if onViewMarkdown}
		<button class="focus-toggle" onclick={onViewMarkdown}>
			<Icon icon="ri:markdown-line" width="16" />
			<span class="focus-label">View as markdown</span>
		</button>
	{/if}
</div>

<style>
	.display-settings {
		display: flex;
		flex-direction: column;
		gap: 8px;
		padding: 12px;
		min-width: 220px;
	}

	.row {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 12px;
	}

	.row-label {
		font-size: 12px;
		color: var(--color-foreground-muted);
	}

	.segmented {
		display: flex;
		gap: 2px;
		padding: 2px;
		background: var(--color-surface-elevated);
		border-radius: var(--radius-md);
	}

	.seg {
		display: flex;
		align-items: center;
		justify-content: center;
		min-width: 30px;
		padding: 4px 8px;
		border: none;
		background: transparent;
		color: var(--color-foreground-muted);
		font-size: 12px;
		font-weight: 500;
		border-radius: var(--radius-md);
		cursor: pointer;
		transition:
			color 0.12s ease,
			background-color 0.12s ease;
	}

	.seg-icon {
		padding: 4px 6px;
	}

	.seg:hover {
		color: var(--color-foreground);
	}

	/* Raised by a hairline, not a shadow: the pane draws none. */
	.seg.active {
		background: var(--color-background);
		color: var(--color-foreground);
		outline: 1px solid var(--color-border);
		outline-offset: -1px;
	}

	.divider {
		height: 1px;
		background: var(--color-border-subtle, var(--color-border));
	}

	.focus-toggle {
		display: flex;
		align-items: center;
		gap: 8px;
		padding: 6px 8px;
		border: none;
		background: transparent;
		color: var(--color-foreground-muted);
		border-radius: var(--radius-md);
		cursor: pointer;
		transition:
			color 0.12s ease,
			background-color 0.12s ease;
	}

	.focus-toggle:hover {
		background: var(--hover-bg);
		color: var(--color-foreground);
	}

	.focus-label {
		flex: 1;
		text-align: left;
		font-size: 13px;
		font-weight: 500;
		color: var(--color-foreground);
	}

	.focus-state {
		font-size: 12px;
		color: var(--color-foreground-subtle);
	}

	.focus-state.on {
		color: var(--color-primary);
	}

	.focus-kbd {
		font-size: 12px;
		font-family: var(--font-sans, sans-serif);
		color: var(--color-foreground-subtle);
	}
</style>
