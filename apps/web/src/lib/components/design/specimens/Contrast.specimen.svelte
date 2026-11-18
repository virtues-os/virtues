<script lang="ts">
	/**
	 * WCAG 2.1 contrast for the pairs a reader meets, in Light and Dark,
	 * measured from the cascade (lib/design/color.ts). Translucent colors are
	 * flattened onto the page first, the way they are drawn.
	 */
	import { onMount } from 'svelte';
	import { contrast, hex, over, readTokens, type TokenReading } from '$lib/design/color';
	import type { Theme } from '$lib/utils/theme';

	type Pair = { fg: string; bg: string; where: string; need: number };

	const PAIRS: Pair[] = [
		{ fg: '--foreground', bg: '--page', where: 'Reading text on paper', need: 4.5 },
		{ fg: '--foreground-muted', bg: '--page', where: 'Secondary text', need: 4.5 },
		{ fg: '--foreground-subtle', bg: '--page', where: 'Hints, dates, captions', need: 4.5 },
		{ fg: '--foreground', bg: '--desk', where: 'Text on the desk', need: 4.5 },
		{ fg: '--primary', bg: '--page', where: 'Links and pressable ink', need: 4.5 },
		{ fg: '--surface', bg: '--primary', where: 'The primary Button label on the accent', need: 4.5 },
		{ fg: '--surface', bg: '--error', where: 'The danger Button label under the pointer', need: 4.5 },
		{ fg: '--success', bg: '--page', where: 'Success text', need: 4.5 },
		{ fg: '--warning', bg: '--page', where: 'Warning text', need: 4.5 },
		{ fg: '--error', bg: '--page', where: 'Error text', need: 4.5 },
		{ fg: '--border-field', bg: '--page', where: "A text field's edge at rest (shape, 3:1)", need: 3 },
		// A hairline is decoration: separation comes from tone (design.md,
		// "Elevation"), so it is measured and held to no minimum.
		{ fg: '--border', bg: '--page', where: 'A hairline (decorative, no minimum)', need: 0 }
	];

	const names = [...new Set(PAIRS.flatMap((p) => [p.fg, p.bg]))];
	let readings = $state<Record<Theme, Record<string, TokenReading>> | null>(null);

	onMount(() => {
		readings = readTokens(names);
	});

	function measure(theme: Theme, pair: Pair) {
		const r = readings?.[theme];
		const page = r?.['--page']?.rgb;
		const fg = r?.[pair.fg]?.rgb;
		const bg = r?.[pair.bg]?.rgb;
		if (!page || !fg || !bg) return null;
		const ground = over(bg, page);
		const ink = over(fg, ground);
		const ratio = contrast(ink, ground);
		return { ratio, ink: hex(ink), ground: hex(ground), pass: ratio >= pair.need };
	}
</script>

{#if readings}
	<table>
		<thead>
			<tr><th>Pair</th><th>Where</th><th>Light</th><th>Dark</th></tr>
		</thead>
		<tbody>
			{#each PAIRS as pair (pair.fg + pair.bg)}
				<tr>
					<td><code>{pair.fg}</code> on <code>{pair.bg}</code></td>
					<td>{pair.where}</td>
					{#each ['oxford', 'asgard'] as const as theme (theme)}
						{@const m = measure(theme, pair)}
						<td class="ratio" class:fail={m && !m.pass}>
							{#if m}
								<span class="chip" style="color: {m.ink}; background: {m.ground}">Aa</span>
								{m.ratio.toFixed(2)}{#if !m.pass} · below {pair.need}{/if}
							{:else}
								not defined
							{/if}
						</td>
					{/each}
				</tr>
			{/each}
		</tbody>
	</table>
{/if}

<style>
	table {
		width: 100%;
		border-collapse: collapse;
		font-size: var(--text-small);
	}
	th {
		text-align: left;
		font-weight: 400;
		color: var(--color-foreground-subtle);
		padding: 0 8px 8px 0;
	}
	td {
		padding: 8px 8px 8px 0;
		border-top: 1px solid var(--color-border);
		vertical-align: middle;
	}
	.ratio {
		white-space: nowrap;
		font-variant-numeric: tabular-nums;
	}
	.fail {
		color: var(--color-error);
	}
	.chip {
		display: inline-block;
		margin-right: 8px;
		padding: 0 4px;
		border-radius: var(--radius-sm);
		outline: 1px solid var(--color-border);
		font-family: var(--font-serif);
	}
</style>
