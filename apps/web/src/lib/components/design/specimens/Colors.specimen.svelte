<script lang="ts">
	/** Every color role in Light and Dark, side by side, read from the cascade. */
	import { onMount } from 'svelte';
	import { readTokens, type TokenReading } from '$lib/design/color';
	import type { Theme } from '$lib/utils/theme';

	const GROUPS: { name: string; tokens: string[] }[] = [
		{ name: 'Grounds', tokens: ['--desk', '--page', '--surface', '--surface-elevated'] },
		{ name: 'Ink', tokens: ['--foreground', '--foreground-muted', '--foreground-subtle'] },
		{ name: 'Lines', tokens: ['--border', '--border-strong', '--border-field'] },
		{ name: 'Accent', tokens: ['--primary', '--secondary'] },
		{ name: 'Status', tokens: ['--success', '--warning', '--error', '--info'] }
	];

	let readings = $state<Record<Theme, Record<string, TokenReading>> | null>(null);
	onMount(() => {
		readings = readTokens(GROUPS.flatMap((g) => g.tokens));
	});
</script>

{#if readings}
	<div class="themes">
		{#each ['oxford', 'asgard'] as const as theme (theme)}
			<div class="theme" data-theme-sample={theme}>
				<div class="theme-name">{theme === 'oxford' ? 'Light' : 'Dark'}</div>
				{#each GROUPS as group (group.name)}
					<div class="group">
						<div class="group-name">{group.name}</div>
						{#each group.tokens as token (token)}
							{@const r = readings[theme][token]}
							<div class="row">
								<span class="swatch" style="background: {r?.value || 'transparent'}"></span>
								<code>{token}</code>
								<span class="value">{r?.value || 'not defined'}</span>
							</div>
						{/each}
					</div>
				{/each}
			</div>
		{/each}
	</div>
{/if}

<style>
	.themes {
		display: grid;
		grid-template-columns: repeat(auto-fit, minmax(240px, 1fr));
		gap: 24px;
	}
	.theme-name {
		font-size: var(--text-label);
		color: var(--color-foreground);
		margin-bottom: 12px;
	}
	.group {
		margin-bottom: 16px;
	}
	.group-name {
		font-size: var(--text-caption);
		color: var(--color-foreground-subtle);
		margin-bottom: 4px;
	}
	.row {
		display: grid;
		grid-template-columns: 24px 1fr auto;
		align-items: center;
		gap: 8px;
		padding: 4px 0;
		font-size: var(--text-small);
	}
	.swatch {
		width: 24px;
		height: 24px;
		border-radius: var(--radius-sm);
		outline: 1px solid var(--color-border);
		outline-offset: -1px;
	}
	.value {
		color: var(--color-foreground-muted);
		font-variant-numeric: tabular-nums;
	}
</style>
