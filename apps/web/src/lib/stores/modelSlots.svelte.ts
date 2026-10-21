/**
 * Which model runs each role (chat, deep research, background work,
 * pictures), and whether that is the person's choice or Virtues' pick.
 *
 * One store because two views write the same values: the role selects on
 * the Assistant page and the rows of the catalog page. Both read from here,
 * so a choice made in one shows in the other at once.
 *
 * A role follows the recommendation when its profile field is null. It then
 * resolves through `slots` from `/models/recommended`, which the box
 * refreshes from virtues-api. Any other value is the person's choice, and
 * nothing changes it but them.
 */
import {
	getRecommendedModels,
	getAssistantProfile,
	getByoKey,
	updateAssistantProfile,
} from '$lib/api/client';

export type SlotKey = 'lite' | 'standard' | 'deep' | 'image';

export interface SlotConfig {
	key: SlotKey;
	/** The role as a heading: "Deep research". */
	label: string;
	/** The role inside a sentence: "Use for deep research". */
	phrase: string;
	/** The role before "model": "Your background model". */
	mark: string;
	/** One line under the label, where the label alone undersells the role. */
	caption?: string;
	/** False for a role a language model cannot fill. The catalog lists
	 *  language models, and `generate_image` sends whatever id is stored here,
	 *  so offering one for pictures saves a model that cannot draw. */
	language: boolean;
	dbField: string;
}

export const SLOTS: SlotConfig[] = [
	{
		key: 'standard',
		label: 'Chat',
		phrase: 'chat',
		mark: 'chat',
		language: true,
		dbField: 'standard_model_id',
	},
	{
		key: 'deep',
		label: 'Deep research',
		phrase: 'deep research',
		mark: 'deep research',
		language: true,
		dbField: 'deep_model_id',
	},
	{
		key: 'lite',
		label: 'Background work',
		phrase: 'background work',
		mark: 'background',
		caption: 'Titles, summaries, and applets',
		language: true,
		dbField: 'lite_model_id',
	},
	{
		key: 'image',
		label: 'Pictures',
		phrase: 'pictures',
		mark: 'picture',
		language: false,
		dbField: 'image_model_id',
	},
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
	/** True for the models Virtues suggests for some role. */
	recommended?: boolean;
	/** "all" | "some" | "none" | null. Null is unknown, not "none". */
	zdr?: string | null;
	no_training?: string | null;
}

/** A failed save. `attempted` is the model the person chose (null for
 *  Recommended) and `kept` is what the role still uses, so the catalog can
 *  show the line on the row it concerns and nowhere else. */
export interface SaveError {
	message: string;
	attempted: string | null;
	kept: string | null;
}

const EMPTY: Record<SlotKey, string | null> = { lite: null, standard: null, deep: null, image: null };

const LOAD_ERROR = "Your server couldn't load the models.";

let models = $state<CatalogModel[]>([]);
let recommended = $state<Record<string, string>>({});
let chosen = $state<Record<SlotKey, string | null>>({ ...EMPTY });
let catalogCold = $state(false);
/** The box sends every AI request to the person's own provider, so the
 *  catalog's retention facts and Virtues' prices do not reach those requests
 *  (virtues_api/client.rs, "The BYO fork"). */
let byoKey = $state(false);
let loading = $state(false);
let loaded = $state(false);
let error = $state<string | null>(null);
let saveErrors = $state<Partial<Record<SlotKey, SaveError>>>({});

/** What the server last confirmed per role. A failed save rolls back to this,
 *  not to what the screen showed when that save began: arrowing through a
 *  select fires a save per step, and those overlap. */
let saved: Record<SlotKey, string | null> = { ...EMPTY };
/** The newest save per role. Only it may roll back or report. */
const latest: Record<SlotKey, number> = { lite: 0, standard: 0, deep: 0, image: 0 };
/** Saves for one role run one at a time, in the order they were made. */
const queue: Partial<Record<SlotKey, Promise<unknown>>> = {};
/** Whether the box knows the Standard/Deep tiers. One from before the
 *  2026-10-05 rename calls Standard `chat` and has no Deep, and this app can
 *  be newer than the box it talks to, so it reads and writes the old names
 *  there and hides the Deep role.
 *  TODO(2026-10-05): drop once every box runs a release with the rename. */
let tiered = $state(true);

async function load(force = false) {
	if (loading || (loaded && !force)) return;
	loading = true;
	error = null;
	try {
		// The profile is not optional: without it every role would read as
		// Recommended, including the ones the person chose.
		const [data, profile, byo] = await Promise.all([
			getRecommendedModels<any>(),
			getAssistantProfile<any>(),
			getByoKey<{ configured?: boolean }>(),
		]);
		const list: any[] = Array.isArray(data) ? data : (data?.data ?? []);
		models = list.map((m) => ({ ...m, model_id: m.model_id ?? m.id }));
		const slots = data?.slots ?? {};
		tiered = 'standard_model_id' in profile;
		recommended = { ...slots, standard: slots.standard ?? slots.chat };
		catalogCold = !!data?.catalog_cold;
		byoKey = !!byo?.configured;
		chosen = {
			lite: profile.lite_model_id || null,
			standard: (tiered ? profile.standard_model_id : profile.chat_model_id) || null,
			deep: profile.deep_model_id || null,
			image: profile.image_model_id || null,
		};
		saved = { ...chosen };
		saveErrors = {};
		loaded = true;
	} catch {
		error = LOAD_ERROR;
	} finally {
		loading = false;
	}
}

/** Choose a model for a role, or pass null to follow the recommendation.
 *  Optimistic, and rolled back if the save fails: a value that was never saved
 *  is worse to show than the old one. */
async function choose(slot: SlotConfig, modelId: string | null) {
	const key = slot.key;
	const seq = ++latest[key];
	chosen[key] = modelId;
	saveErrors[key] = undefined;
	// `null` is how the backend hears "follow the recommendation". An
	// omitted key would change nothing (see assistant_profile.rs).
	const field = !tiered && key === 'standard' ? 'chat_model_id' : slot.dbField;
	const run = (queue[key] ?? Promise.resolve()).then(() =>
		updateAssistantProfile({ [field]: modelId }),
	);
	queue[key] = run.catch(() => {});
	try {
		await run;
		saved[key] = modelId;
	} catch {
		// A newer choice for this role is queued behind this one; it decides
		// what the role ends up with.
		if (seq !== latest[key]) return;
		const kept = saved[key];
		chosen[key] = kept;
		const was = kept
			? nameOf(kept)
			: (catalogName(recommended[key]) ?? 'the recommended model');
		saveErrors[key] = {
			message: `Your server couldn't save this, so ${slot.label} still uses ${was}. Choose again in a moment.`,
			attempted: modelId,
			kept,
		};
	}
}

/** The catalog's name for a model, or null for one it doesn't list. */
function catalogName(id: string | null | undefined): string | null {
	if (!id) return null;
	return models.find((m) => m.model_id === id)?.display_name ?? null;
}

/** A model's name. One the catalog doesn't list, such as a pinned picture
 *  model or one the gateway has dropped, goes by the last part of its id,
 *  without the provider prefix. */
function nameOf(id: string | null | undefined): string {
	if (!id) return '';
	return catalogName(id) ?? id.slice(id.lastIndexOf('/') + 1);
}

/** The models a role's select suggests: every language role's
 *  recommendation, plus any the catalog flags. The flag can miss a role's
 *  own pick (the cloud's slot map and the catalog's flag are set apart), so
 *  the slot map is read too. None for a role a language model cannot fill. */
function suggestedFor(slot: SlotConfig): CatalogModel[] {
	if (!slot.language) return [];
	const picks = new Set(
		SLOTS.filter((s) => s.language).map((s) => recommended[s.key]),
	);
	return models.filter((m) => m.recommended || picks.has(m.model_id));
}

export const modelSlots = {
	/** The roles this box has. */
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
	get byoKey() {
		return byoKey;
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
	get saveErrors() {
		return saveErrors;
	},
	load,
	choose,
	catalogName,
	nameOf,
	suggestedFor,
};
