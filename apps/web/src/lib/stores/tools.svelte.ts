/**
 * Tools store - fetches and caches tool data from API
 */
import { fetchTools, type Tool } from '$lib/config/tools';

let toolsCache: Tool[] = $state([]);
let loading = $state(true);
let error = $state<string | null>(null);
let initializationPromise: Promise<void> | null = null;

/**
 * Load tools from API
 */
async function loadTools() {
	if (toolsCache.length > 0) {
		return;
	}

	loading = true;
	error = null;

	try {
		toolsCache = await fetchTools();
	} catch (err) {
		error = err instanceof Error ? err.message : 'Failed to load tools';
	} finally {
		loading = false;
	}
}

/**
 * Get initialization promise to wait for tools to load
 */
export function getInitializationPromise(): Promise<void> {
	if (!initializationPromise) {
		initializationPromise = loadTools();
	}
	return initializationPromise;
}

/**
 * Get all tools
 */
export function getTools(): Tool[] {
	return toolsCache;
}

/**
 * Get tool by ID
 */
export function getToolById(id: string): Tool | undefined {
	return toolsCache.find((t) => t.id === id);
}

/**
 * Get tools by type
 */
export function getToolsByType(type: 'builtin' | 'mcp'): Tool[] {
	return toolsCache.filter((t) => t.tool_type === type);
}

/**
 * Check if tools are loading
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
 * Force reload tools
 */
export async function reloadTools() {
	toolsCache = [];
	initializationPromise = null;
	await loadTools();
}
