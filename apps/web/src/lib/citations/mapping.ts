/**
 * Icon and label mappings for citations
 * Maps ontology domains and tools to display information
 */

import type { DisplayInfo } from '$lib/types/Citation';

/**
 * Ontology table name → display info
 * Keys match the table names in data schema (e.g., "health_sleep", "praxis_calendar")
 */
export const ONTOLOGY_DISPLAY: Record<string, DisplayInfo> = {
	// Health domain
	health_sleep: { icon: 'ri:moon-line', color: 'text-indigo-500', label: 'Sleep' },
	health_steps: { icon: 'ri:footprint-line', color: 'text-green-500', label: 'Steps' },
	health_heart_rate: { icon: 'ri:heart-pulse-line', color: 'text-red-500', label: 'Heart Rate' },
	health_hrv: { icon: 'ri:heart-pulse-line', color: 'text-red-400', label: 'HRV' },
	health_workout: { icon: 'ri:run-line', color: 'text-orange-500', label: 'Workout' },

	// Praxis domain
	praxis_calendar: { icon: 'ri:calendar-line', color: 'text-blue-500', label: 'Calendar' },

	// Social domain
	social_email: { icon: 'ri:mail-line', color: 'text-purple-500', label: 'Email' },
	social_message: { icon: 'ri:chat-3-line', color: 'text-purple-400', label: 'Messages' },

	// Location domain
	location_point: { icon: 'ri:map-pin-line', color: 'text-blue-600', label: 'Location' },
	location_visit: { icon: 'ri:map-pin-2-line', color: 'text-blue-500', label: 'Visits' },

	// Knowledge domain
	knowledge_document: { icon: 'ri:file-text-line', color: 'text-amber-500', label: 'Documents' },
	knowledge_ai_conversation: { icon: 'ri:chat-ai-line', color: 'text-violet-500', label: 'AI Chats' },

	// Speech domain
	speech_transcription: { icon: 'ri:mic-line', color: 'text-pink-500', label: 'Transcription' },

	// Activity domain
	activity_app_usage: { icon: 'ri:apps-line', color: 'text-cyan-500', label: 'App Usage' },
	activity_web_browsing: { icon: 'ri:global-line', color: 'text-cyan-400', label: 'Browsing' },

	// Financial domain
	financial_transaction: {
		icon: 'ri:money-dollar-circle-line',
		color: 'text-emerald-500',
		label: 'Transactions'
	},
	financial_account: { icon: 'ri:bank-line', color: 'text-emerald-400', label: 'Accounts' }
};

/**
 * Tool name → display info (fallback when ontology can't be detected)
 */
export const TOOL_DISPLAY: Record<string, DisplayInfo> = {
	virtues_query_ontology: { icon: 'ri:database-2-line', color: 'text-gray-500', label: 'Data' },
	virtues_query_narratives: {
		icon: 'ri:book-2-line',
		color: 'text-violet-500',
		label: 'Narratives'
	},
	virtues_query_axiology: { icon: 'ri:heart-3-line', color: 'text-rose-500', label: 'Values' },
	virtues_semantic_search: {
		icon: 'ri:search-eye-line',
		color: 'text-violet-500',
		label: 'Search'
	},
	virtues_list_ontology_tables: {
		icon: 'ri:table-line',
		color: 'text-gray-400',
		label: 'Tables'
	},
	virtues_get_table_schema: { icon: 'ri:file-info-line', color: 'text-gray-400', label: 'Schema' },
	web_search: { icon: 'ri:global-line', color: 'text-indigo-500', label: 'Web Search' },
	// Google's native search grounding tool
	googleSearch: { icon: 'ri:google-line', color: 'text-blue-500', label: 'Web Search' },
	query_location_map: { icon: 'ri:map-2-line', color: 'text-blue-500', label: 'Map' }
};

/**
 * Default display info for unknown tools
 */
export const DEFAULT_DISPLAY: DisplayInfo = {
	icon: 'ri:tools-line',
	color: 'text-gray-500',
	label: 'Tool'
};

/**
 * Extract ontology table name from a SQL query
 * Looks for patterns like "FROM data.health_sleep" or "FROM data.praxis_calendar"
 */
export function extractOntologyFromQuery(query: string): string | null {
	// Match "FROM data.table_name" pattern (case insensitive)
	const fromMatch = query.match(/FROM\s+data\.(\w+)/i);
	if (fromMatch) {
		return fromMatch[1];
	}

	// Match "JOIN data.table_name" pattern
	const joinMatch = query.match(/JOIN\s+data\.(\w+)/i);
	if (joinMatch) {
		return joinMatch[1];
	}

	return null;
}

/**
 * Get display info for a tool call
 * Attempts to detect specific ontology from query, falls back to tool-level display
 */
export function getDisplayInfo(
	toolName: string,
	toolArgs?: Record<string, unknown>
): DisplayInfo {
	// Try to extract ontology from SQL query
	if (toolName === 'virtues_query_ontology' && toolArgs?.query) {
		const ontology = extractOntologyFromQuery(toolArgs.query as string);
		if (ontology && ONTOLOGY_DISPLAY[ontology]) {
			return ONTOLOGY_DISPLAY[ontology];
		}
	}

	// Fall back to tool-level display
	if (TOOL_DISPLAY[toolName]) {
		return TOOL_DISPLAY[toolName];
	}

	return DEFAULT_DISPLAY;
}

/**
 * Infer source type from tool name
 */
export function inferSourceType(
	toolName: string
): 'ontology' | 'axiology' | 'web_search' | 'narratives' | 'location' | 'generic' {
	switch (toolName) {
		case 'virtues_query_ontology':
		case 'virtues_semantic_search':
			return 'ontology';
		case 'virtues_query_axiology':
			return 'axiology';
		case 'virtues_query_narratives':
			return 'narratives';
		case 'web_search':
		case 'googleSearch':
			return 'web_search';
		case 'query_location_map':
			return 'location';
		default:
			return 'generic';
	}
}
