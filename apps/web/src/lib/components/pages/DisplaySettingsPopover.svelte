<script lang="ts">
	import Icon from "$lib/components/Icon.svelte";
	import {
		pageDisplay,
		type FontMode,
		type TextSize,
		type WidthMode,
	} from "$lib/stores/pageDisplay.svelte";

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
						? "var(--font-serif, Georgia, serif)"
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
					<Icon icon={opt.icon} width="15" />
				</button>
			{/each}
		</div>
	</div>

	<div class="divider"></div>

	<button class="focus-toggle" onclick={() => pageDisplay.toggleFocus()}>
		<Icon
			icon={pageDisplay.focusMode
				? "ri:focus-2-fill"
				: "ri:focus-2-line"}
			width="15"
		/>
		<span class="focus-label">Focus mode</span>
		<span class="focus-state" class:on={pageDisplay.focusMode}>
			{pageDisplay.focusMode ? "On" : "Off"}
		</span>
		<kbd class="focus-kbd">⌘⇧F</kbd>
	</button>

	<button class="focus-toggle" onclick={() => pageDisplay.toggleRaw()}>
		<Icon
			icon={pageDisplay.rawMode ? "ri:markdown-fill" : "ri:markdown-line"}
			width="15"
		/>
		<span class="focus-label">Raw markdown</span>
		<span class="focus-state" class:on={pageDisplay.rawMode}>
			{pageDisplay.rawMode ? "On" : "Off"}
		</span>
	</button>
</div>

<style>
	.display-settings {
		display: flex;
		flex-direction: column;
		gap: 10px;
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
		border-radius: 7px;
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
		border-radius: 5px;
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

	.seg.active {
		background: var(--color-background);
		color: var(--color-foreground);
		box-shadow: 0 1px 2px rgba(0, 0, 0, 0.06);
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
		border-radius: 6px;
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
		font-size: 11px;
		color: var(--color-foreground-subtle);
	}

	.focus-state.on {
		color: var(--color-primary);
	}

	.focus-kbd {
		font-size: 10px;
		font-family: var(--font-mono, monospace);
		color: var(--color-foreground-subtle);
		background: var(--color-surface-elevated);
		padding: 1px 5px;
		border-radius: 4px;
	}
</style>
