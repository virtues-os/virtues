/**
 * Provider Icon Mapping
 *
 * Maps model provider names to icon identifiers.
 * Uses simple-icons for official brand icons, remixicon for fallbacks.
 */

export const PROVIDER_ICONS: Record<string, string> = {
	anthropic: 'simple-icons:anthropic',
	google: 'simple-icons:google',
	openai: 'simple-icons:openai',
	mistral: 'simple-icons:mistral',
	cerebras: 'ri:cpu-line',
	ollama: 'simple-icons:ollama',
	deepseek: 'ri:brain-line',
	xai: 'simple-icons:x',
	glm: 'ri:sparkling-line',
	zai: 'ri:sparkling-line'
};

export const DEFAULT_PROVIDER_ICON = 'ri:robot-fill';

/**
 * Get the icon identifier for a model provider
 */
export function getProviderIcon(provider: string): string {
	const normalized = provider.toLowerCase();
	return PROVIDER_ICONS[normalized] || DEFAULT_PROVIDER_ICON;
}
