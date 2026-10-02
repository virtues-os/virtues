import { describe, expect, it } from 'vitest';
import type { StyleSpecification } from 'maplibre-gl';
import { APPLE_LIGHT, hsl, recolour } from './palette';

const style = (): StyleSpecification => ({
	version: 8,
	sources: {},
	layers: [
		{ id: 'world:background', type: 'background', paint: { 'background-color': '#000' } },
		{ id: 'home:earth', type: 'fill', source: 'home', 'source-layer': 'earth', paint: { 'fill-opacity': 0.5 } },
		{ id: 'visited:pois', type: 'symbol', source: 'visited', 'source-layer': 'pois', paint: { 'text-color': '#123' } },
		{
			id: 'home:places_locality',
			type: 'symbol',
			source: 'home',
			'source-layer': 'places',
			paint: { 'text-color': '#111', 'text-halo-color': '#fff' },
		},
	],
});

describe('recolour', () => {
	it('paints a group onto every tier copy of its layer, keeping other paint', () => {
		const [bg, earth] = recolour(style(), APPLE_LIGHT).layers as { paint: Record<string, unknown> }[];
		expect(bg.paint['background-color']).toBe(hsl(APPLE_LIGHT.land));
		expect(earth.paint).toEqual({ 'fill-opacity': 0.5, 'fill-color': hsl(APPLE_LIGHT.land) });
	});

	it('sets ink and halo on the same label layer', () => {
		const label = recolour(style(), APPLE_LIGHT).layers[3] as { paint: Record<string, unknown> };
		expect(label.paint['text-color']).toBe(hsl(APPLE_LIGHT.labelInk));
		expect(label.paint['text-halo-color']).toBe(hsl(APPLE_LIGHT.labelHalo));
	});

	it('leaves layers outside the palette and the input style untouched', () => {
		const input = style();
		const out = recolour(input, APPLE_LIGHT);
		expect(out.layers[2]).toBe(input.layers[2]);
		expect((input.layers[0] as { paint: Record<string, unknown> }).paint['background-color']).toBe('#000');
	});
});
