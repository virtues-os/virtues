/**
 * Model slots: which model fills each slot, and whether that is the person's
 * choice or the Virtues recommendation.
 *
 * One store because the Assistant page has two controls over the same
 * values: the slot pickers at the top and the catalog table at the bottom.
 * When each fetched and saved on its own, a choice made in the table left the
 * picker showing the old model until the page was reopened.
 *
 * A slot is on Recommended when its profile field is null. It then follows
 * `slots` from `/models/recommended`, which the box refreshes from virtues-api.
 * Any other value is the person's choice, and nothing changes it but them.
 */
import {
	getRecommendedModels,
	getAssistantProfile,
	updateAssistantProfile,
} from '$lib/api/client';

export type SlotKey = 'lite' | 'standard' | 'deep' | 'image';

export interface SlotConfig {
	key: SlotKey;
	label: string;
	description: string;
	dbField: string;
}

export const SLOTS: SlotConfig[] = [
	{
		key: 'standard',
		label: 'Standard',
		description: 'Conversations',
		dbField: 'standard_model_id',
	},
	{
		key: 'deep',
		label: 'Deep',
		description: 'Deep research and council',
		dbField: 'deep_model_id',
	},
	{
		key: 'lite',
		label: 'Lite',
		description: 'Titles, summaries, background work',
		dbField: 'lite_model_id',
	},
	{ key: 'image', label: 'Image', description: 'Text-to-image', dbField: 'image_model_id' },
];

/** One model as `/models/recommended` lists it. */
export interface CatalogModel {
	model_id: string;
	display_name: string;
	provider: string;
	context_window?: number | null;
	max_output_tokens?: number | null;
	supports_tools?: boolean | null;
	supports_vision?: boolean | null;
	supports_pdf?: boolean | null;
	supports_audio?: boolean | null;
	input_cost_per_1k?: number | null;
	output_cost_per_1k?: number | null;
	/** True for the models Virtues suggests for some slot. */
	recommended?: boolean;
	/** "all" | "some" | "none" | null. Null is unknown, not "none". */
	zdr?: string | null;
	no_training?: string | null;
}

const EMPTY: Record<SlotKey, string | null> = { lite: null, standard: null, deep: null, image: null };

let models = $state<CatalogModel[]>([]);
let recommended = $state<Record<string, string>>({});
let chosen = $state<Record<SlotKey, string | null>>({ ...EMPTY });
let catalogCold = $state(false);
let loading = $state(false);
let loaded = $state(false);
let error = $state<string | null>(null);
let saveError = $state<string | null>(null);
/** Whether the box knows the Standard/Deep tiers. One from before the
 *  2026-10-05 rename calls Standard `chat` and has no Deep, and this app can
 *  be newer than the box it talks to — so it reads and writes the old names
 *  there and hides the Deep picker.
 *  TODO(2026-10-05): drop once every box runs a release with the rename. */
let tiered = $state(true);

async function load(force = false) {
	if (loading || (loaded && !force)) return;
	loading = true;
	error = null;
	try {
		const [data, profile] = await Promise.all([
			getRecommendedModels<any>(),
			getAssistantProfile<any>().catch(() => null),
		]);
		const list: any[] = Array.isArray(data) ? data : (data?.data ?? []);
		models = list.map((m) => ({ ...m, model_id: m.model_id ?? m.id }));
		const slots = data?.slots ?? {};
		tiered = profile ? 'standard_model_id' in profile : 'standard' in slots;
		recommended = { ...slots, standard: slots.standard ?? slots.chat };
		catalogCold = !!data?.catalog_cold;
		if (profile) {
			chosen = {
				lite: profile.lite_model_id || null,
				standard: (tiered ? profile.standard_model_id : profile.chat_model_id) || null,
				deep: profile.deep_model_id || null,
				image: profile.image_model_id || null,
			};
		}
		loaded = true;
	} catch (e) {
		error = e instanceof Error ? e.message : "Couldn't load the models";
	} finally {
		loading = false;
	}
}

/** Choose a model for a slot, or pass null to put it back on Recommended.
 *  Optimistic, and rolled back if the save fails: a value that was never saved
 *  is worse to show than the old one. */
async function choose(slot: SlotConfig, modelId: string | null) {
	const previous = chosen[slot.key];
	chosen[slot.key] = modelId;
	saveError = null;
	try {
		// `null` is how the backend hears "follow the recommendation". An
		// omitted key would change nothing (see assistant_profile.rs).
		const field = !tiered && slot.key === 'standard' ? 'chat_model_id' : slot.dbField;
		await updateAssistantProfile({ [field]: modelId });
	} catch (e) {
		chosen[slot.key] = previous;
		saveError = `Couldn't save ${slot.label}. ${e instanceof Error ? e.message : ''}`.trim();
	}
}

function nameOf(id: string | null | undefined): string {
	if (!id) return '';
	return models.find((m) => m.model_id === id)?.display_name ?? id;
}

export const modelSlots = {
	/** The slots this box has. */
	get slots() {
		return tiered ? SLOTS : SLOTS.filter((s) => s.key !== 'deep');
	},
	get models() {
		return models;
	},
	get recommended() {
		return recommended;
	},
	get chosen() {
		return chosen;
	},
	get catalogCold() {
		return catalogCold;
	},
	get loading() {
		return loading;
	},
	get loaded() {
		return loaded;
	},
	get error() {
		return error;
	},
	get saveError() {
		return saveError;
	},
	load,
	choose,
	nameOf,
};
