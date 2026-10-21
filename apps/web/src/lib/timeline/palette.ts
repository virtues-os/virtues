/**
 * The map palette the Timeline paints over the box's atlas: the prototype's
 * "Apple" preset (dayback/src/main.js:1642-1661), a calm stage under the day's
 * marks. Each group recolours one paint property of a fixed set of Protomaps
 * layers; everything else keeps the flavor's own colours.
 */
import type { LayerSpecification, StyleSpecification } from 'maplibre-gl';

type Hsl = { h: number; s: number; l: number };
type Group =
	| 'land'
	| 'build'
	| 'water'
	| 'greenForest'
	| 'greenField'
	| 'greenUrban'
	| 'greenOther'
	| 'roadMinor'
	| 'roadMajor'
	| 'labelInk'
	| 'labelHalo';
export type Palette = Record<Group, Hsl>;

export const APPLE_LIGHT: Palette = {
	land: { h: 45, s: 42, l: 93 },
	build: { h: 30, s: 5, l: 91 },
	water: { h: 193, s: 68, l: 72 },
	// The four greens eyedropped from Apple Maps by role (forest, field or
	// pitch, urban park, other), so a park reads as the same park by day and
	// by night (TIMELINE-DESIGN-PRINCIPLES "Colour").
	greenForest: { h: 95, s: 52, l: 72 },
	greenField: { h: 94, s: 66, l: 80 },
	greenUrban: { h: 89, s: 60, l: 77 },
	greenOther: { h: 91, s: 54, l: 74 },
	roadMinor: { h: 0, s: 0, l: 100 },
	roadMajor: { h: 0, s: 0, l: 100 },
	labelInk: { h: 38, s: 10, l: 40 },
	labelHalo: { h: 45, s: 30, l: 97 },
};

export const APPLE_DARK: Palette = {
	land: { h: 212, s: 27, l: 27 },
	build: { h: 212, s: 28, l: 42 },
	water: { h: 225, s: 59, l: 30 },
	greenForest: { h: 180, s: 100, l: 16 },
	greenField: { h: 160, s: 98, l: 21 },
	greenUrban: { h: 176, s: 100, l: 18 },
	greenOther: { h: 173, s: 44, l: 24 },
	roadMinor: { h: 218, s: 17, l: 28 },
	roadMajor: { h: 218, s: 17, l: 28 },
	labelInk: { h: 188, s: 14, l: 80 },
	labelHalo: { h: 196, s: 35, l: 9 },
};

const LABELS = [
	'roads_labels_minor',
	'roads_labels_major',
	'places_subplace',
	'places_region',
	'places_locality',
	'places_country',
	'water_label_ocean',
	'water_label_lakes',
	'earth_label_islands',
	'address_label',
	'water_waterway_label',
];

/** The park layer's kinds by green: woodland, then open grass. */
const FOREST = ['forest', 'wood', 'national_park', 'nature_reserve', 'protected_area', 'scrub'];
const FIELD = ['grassland', 'grass', 'golf_course'];

/** Paint property -> the base layer ids it is set on, per group. */
const GROUPS: Partial<Record<Group, Record<string, string[]>>> = {
	land: { 'background-color': ['background'], 'fill-color': ['earth'] },
	build: { 'fill-color': ['buildings'] },
	water: { 'fill-color': ['water'], 'line-color': ['water_stream', 'water_river'] },
	greenUrban: { 'fill-color': ['landuse_urban_green'] },
	greenOther: { 'fill-color': ['landuse_school', 'landuse_zoo'] },
	roadMinor: {
		'line-color': [
			'roads_minor',
			'roads_minor_service',
			'roads_link',
			'roads_other',
			'roads_taxiway',
			'roads_minor_casing',
			'roads_link_casing',
			'roads_minor_service_casing',
		],
	},
	roadMajor: {
		'line-color': [
			'roads_major',
			'roads_highway',
			'roads_rail',
			'roads_major_casing_late',
			'roads_major_casing_early',
			'roads_highway_casing_late',
			'roads_highway_casing_early',
		],
	},
	labelInk: { 'text-color': LABELS },
	labelHalo: { 'text-halo-color': LABELS },
};

export const hsl = (c: Hsl) => `hsl(${Math.round(c.h)}, ${Math.round(c.s)}%, ${Math.round(c.l)}%)`;

/**
 * `style` with `palette` painted over it. The atlas stacks one copy of the
 * layer set per map tier, prefixed with the tier ("home:earth"), so a layer
 * is matched on the id after its last colon.
 */
export function recolour(style: StyleSpecification, palette: Palette): StyleSpecification {
	const paint = new Map<string, Record<string, unknown>>();
	for (const group of Object.keys(GROUPS) as Group[]) {
		for (const [prop, ids] of Object.entries(GROUPS[group] ?? {})) {
			for (const id of ids) paint.set(id, { ...paint.get(id), [prop]: hsl(palette[group]) });
		}
	}
	// One base layer holds forest, fields and parks alike: its fill picks the
	// green by the land's kind.
	paint.set('landuse_park', {
		'fill-color': [
			'match',
			['get', 'kind'],
			FOREST,
			hsl(palette.greenForest),
			FIELD,
			hsl(palette.greenField),
			['park'],
			hsl(palette.greenUrban),
			hsl(palette.greenOther),
		],
	});
	return {
		...style,
		layers: style.layers.map((layer) => {
			const set = paint.get(layer.id.slice(layer.id.lastIndexOf(':') + 1));
			if (!set) return layer;
			const own = (layer as { paint?: Record<string, unknown> }).paint;
			return { ...layer, paint: { ...own, ...set } } as LayerSpecification;
		}),
	};
}
