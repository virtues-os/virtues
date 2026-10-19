/**
 * Model configuration fetched from database via API
 * This provides a single source of truth for model metadata including context window limits
 */

import { browser } from '$app/environment';
import { listModels, getModel, ApiError } from '$lib/api/client';

export interface ModelOption {
	id: string;
	displayName: string;
	provider: string;
	contextWindow: number | null;
	maxOutputTokens: number | null;
	supportsTools: boolean | null;
	supportsVision: boolean | null;
	supportsPdf: boolean | null;
	supportsAudio: boolean | null;
	enabled: boolean;
	sortOrder: number;
	isDefault?: boolean;
}

/**
 * Fetch all models from API
 */
export async function fetchModels(): Promise<ModelOption[]> {
	if (!browser) {
		return [];
	}

	const data = await listModels<any[]>();

	// Transform API response to ModelOption format
	return data.map((model: any) => ({
		id: model.model_id,
		displayName: model.display_name,
		provider: model.provider,
		contextWindow: model.context_window,
		maxOutputTokens: model.max_output_tokens,
		supportsTools: model.supports_tools,
		supportsVision: model.supports_vision ?? null,
		supportsPdf: model.supports_pdf ?? null,
		supportsAudio: model.supports_audio ?? null,
		enabled: model.enabled,
		sortOrder: model.sort_order,
		isDefault: model.is_default || false
	}));
}

/**
 * Get model configuration by ID from API
 */
export async function getModelById(modelId: string): Promise<ModelOption | null> {
	if (!browser) {
		return null;
	}

	let model: any;
	try {
		model = await getModel<any>(modelId);
	} catch (e) {
		if (e instanceof ApiError && e.status === 404) return null;
		throw e;
	}

	return {
		id: model.model_id,
		displayName: model.display_name,
		provider: model.provider,
		contextWindow: model.context_window,
		maxOutputTokens: model.max_output_tokens,
		supportsTools: model.supports_tools,
		supportsVision: model.supports_vision ?? null,
		supportsPdf: model.supports_pdf ?? null,
		supportsAudio: model.supports_audio ?? null,
		enabled: model.enabled,
		sortOrder: model.sort_order,
		isDefault: model.is_default || false
	};
}
