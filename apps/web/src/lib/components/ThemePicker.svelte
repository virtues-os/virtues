<script lang="ts">
	/**
	 * Light, Dark or Match system (design.md, "Themes"). Each card is a
	 * small window in its theme's colors; Match system shows Light on its
	 * left half and Dark on its right.
	 */
	import {
		getAvailableThemes,
		getThemeDisplayName,
		setTheme,
		themePreviewColors,
		type Theme,
		type ThemeChoice
	} from '$lib/utils/theme';

	interface Props {
		value: ThemeChoice;
		onchange?: (choice: ThemeChoice) => void;
	}

	let { value, onchange }: Props = $props();

	const choices = getAvailableThemes();

	async function select(choice: ThemeChoice) {
		await setTheme(choice);
		onchange?.(choice);
	}
</script>

{#snippet preview(theme: Theme)}
	{@const c = themePreviewColors[theme]}
	<div class="window" style="background: {c.background}">
		<div class="sheet" style="background: {c.surface}; border-color: {c.surfaceElevated}">
			<div class="side" style="background: {c.surfaceElevated}">
				<span class="dot" style="background: {c.primary}"></span>
				<span class="dot faint" style="background: {c.foregroundMuted}"></span>
			</div>
			<div class="lines">
				<span class="line" style="background: {c.foreground}; width: 75%"></span>
				<span class="line faint" style="background: {c.foregroundMuted}"></span>
				<span class="line" style="background: {c.primary}; width: 66%"></span>
			</div>
		</div>
	</div>
{/snippet}

<div class="choices" role="radiogroup" aria-label="Theme">
	{#each choices as choice (choice)}
		<button
			type="button"
			class="choice"
			role="radio"
			aria-checked={value === choice}
			class:selected={value === choice}
			onclick={() => select(choice)}
		>
			<span class="frame">
				{#if choice === 'system'}
					<span class="split">
						{@render preview('oxford')}
						<span class="right">{@render preview('asgard')}</span>
					</span>
				{:else}
					{@render preview(choice)}
				{/if}
			</span>
			<span class="name">{getThemeDisplayName(choice)}</span>
		</button>
	{/each}
</div>

<style>
	.choices {
		display: grid;
		grid-template-columns: repeat(3, minmax(0, 1fr));
		gap: 16px;
		max-width: 32rem;
	}

	.choice {
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: 8px;
		padding: 0;
		background: none;
		border: none;
		cursor: pointer;
		color: var(--color-foreground);
	}

	.frame {
		display: block;
		width: 100%;
		border-radius: var(--radius-xl);
		overflow: hidden;
		/* The selection is the accent edge: the one color a pressable thing
		   carries. Rest and hover are hairlines, drawn inside the frame. */
		outline: 1px solid var(--color-border);
		outline-offset: -1px;
		transition: outline-color var(--motion-hover-out) var(--ease-premium);
	}

	.choice:hover .frame {
		outline-color: var(--color-foreground-subtle);
		transition-duration: var(--motion-hover-in);
	}

	.choice.selected .frame {
		outline: 2px solid var(--color-primary);
		outline-offset: -2px;
	}

	.window {
		display: block;
		aspect-ratio: 16 / 10;
		padding: 8px;
	}

	.sheet {
		display: flex;
		height: 100%;
		border: 1px solid;
		border-radius: var(--radius-md);
		overflow: hidden;
	}

	.side {
		display: flex;
		flex-direction: column;
		gap: 4px;
		width: 20%;
		padding: 4px;
	}

	.dot {
		display: block;
		aspect-ratio: 1;
		border-radius: var(--radius-sm);
	}

	.lines {
		display: flex;
		flex: 1;
		flex-direction: column;
		gap: 4px;
		padding: 8px;
	}

	.line {
		display: block;
		height: 4px;
		border-radius: var(--radius-full);
	}

	.faint {
		opacity: 0.5;
	}

	.split {
		position: relative;
		display: block;
	}

	.right {
		position: absolute;
		inset: 0;
		clip-path: inset(0 0 0 50%);
	}

	.name {
		font-size: var(--text-label);
		color: var(--color-foreground-muted);
	}

	.choice.selected .name {
		color: var(--color-foreground);
	}
</style>
