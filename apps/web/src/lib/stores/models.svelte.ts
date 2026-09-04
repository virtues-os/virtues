/**
 * Models store - fetches and caches model data from API
 * Centralizes model state management including selection
 */
import { fetchModels, type ModelOption } from '$lib/config/models';

let modelsCache: ModelOption[] = $state([]);
let loading = $state(true);
let error = $state<string | null>(null);
let selectedModel = $state<ModelOption | undefined>(undefined);
let initializationPromise: Promise<void> | null = null;

/**
 * Load models from API
 */
async function loadModels() {
	if (modelsCache.length > 0) {
		// Already loaded
		return;
	}

	loading = true;
	error = null;

	try {
		modelsCache = await fetchModels();
	} catch (err) {
		error = err instanceof Error ? err.message : 'Failed to load models';
	} finally {
		loading = false;
	}
}

/**
 * Get initialization promise to wait for models to load
 */
export function getInitializationPromise(): Promise<void> {
	// A failed load must not be remembered as a load.
	//
	// This used to memoize the promise unconditionally, so one flaky fetch —
	// a phone's first request over a cold iroh hop, say — left the catalog
	// empty for the life of the app process: every later caller got the
	// already-settled promise back and `loadModels` never ran a second time.
	// A desktop reload hid it; a webview that survives days of suspends did
	// not. Only a load that actually produced models is worth keeping.
	if (!initializationPromise) {
		initializationPromise = loadModels().finally(() => {
			if (modelsCache.length === 0) initializationPromise = null;
		});
	}
	return initializationPromise;
}

/**
 * Get all models (returns cached value, does not trigger load)
 */
export function getModels(): ModelOption[] {
	return modelsCache;
}

/**
 * Get model by ID
 */
export function getModelById(modelId: string): ModelOption | undefined {
	return modelsCache.find((m) => m.id === modelId);
}

/**
 * The model the box says the Chat slot resolves to, for DISPLAY only.
 *
 * The box decides what actually answers (`model_choice::resolve_turn_model`);
 * this is how the picker names it. It used to fall back to `modelsCache[0]`,
 * which is not a default but "whatever the gateway listed first" — and the
 * three vouched models all carry the same sort order, so that tie-break
 * currently lands on the CODING model. If the flag ever goes missing the
 * honest answer is that we do not know, and a blank picker beats a confident
 * mislabel.
 */
export function getDefaultModel(): ModelOption | undefined {
	return modelsCache.find((m) => m.isDefault);
}

/**
 * Get the currently selected model
 */
export function getSelectedModel(): ModelOption | undefined {
	return selectedModel;
}

/**
 * Set the selected model
 */
export function setSelectedModel(model: ModelOption | undefined) {
	selectedModel = model;
}

/**
 * Seed what the picker DISPLAYS, once, from the owner's standing pin.
 *
 * This used to be a four-step precedence chain (conversation model, then the
 * profile pin, then the flag, then the first row) — a second implementation of
 * a rule the box already owns, running in a browser that may not have the
 * catalog. The box's chain is the real one: pin, then the cloud slot map, then
 * the compiled floor, in `api::model_choice`. This is display only, and being
 * unable to seed it is no longer a reason a turn cannot be sent.
 */
export function initializeSelectedModel(profileDefaultModelId?: string): void {
	if (selectedModel) return; // the person's pick outlives a chat switch

	const pinned = profileDefaultModelId ? getModelById(profileDefaultModelId) : undefined;
	setSelectedModel(pinned ?? getDefaultModel());
}

/**
 * Check if models are loading
 */
export function isLoading(): boolean {
	return loading;
}

/**
 * Get error if any
 */
export function getError(): string | null {
	return error;
}

/**
 * Force reload models
 */
export async function reloadModels() {
	modelsCache = [];
	initializationPromise = null;
	await loadModels();
}
