/**
 * Tool configuration fetched from registry and database via API
 */

import { browser } from '$app/environment';

export interface Tool {
	id: string;
	name: string;
	description: string | null;
	tool_type: 'builtin' | 'mcp';
	category: string | null;
	icon: string | null;
	display_order: number | null;
	enabled: boolean;
	server_name?: string | null;
	input_schema?: any | null;
}

/**
 * Fetch all tools from API
 */
export async function fetchTools(category?: string): Promise<Tool[]> {
	if (!browser) {
		return [];
	}

	const url = category ? `/api/tools?category=${encodeURIComponent(category)}` : '/api/tools';
	const response = await fetch(url);
	if (!response.ok) {
		throw new Error(`Failed to fetch tools: ${response.statusText}`);
	}
	return await response.json();
}

/**
 * Get tool by ID from API
 */
export async function getToolById(id: string): Promise<Tool | null> {
	if (!browser) {
		return null;
	}

	const response = await fetch(`/api/tools/${encodeURIComponent(id)}`);
	if (!response.ok) {
		if (response.status === 404) return null;
		throw new Error(`Failed to fetch tool: ${response.statusText}`);
	}
	return await response.json();
}
