/**
 * Citation types for inline source references
 * Enables NotebookLM/Perplexity-style citations in responses
 */

export type SourceType = 'ontology' | 'web_search' | 'narratives' | 'location' | 'generic';

export interface Citation {
	/** Sequential ID: "1", "2", "3" */
	id: string;

	/** Links to actual tool call for verification */
	tool_call_id: string;

	/** Tool that generated this citation */
	tool_name: string;

	/** Categorization for display purposes */
	source_type: SourceType;

	/** Remixicon identifier (e.g., "ri:heart-pulse-line") */
	icon: string;

	/** Display label (e.g., "Sleep", "Calendar", "Values") */
	label: string;

	/** Tailwind color class (e.g., "text-red-500") */
	color: string;

	/** Short preview text (e.g., "7 records, avg 7.2 hrs") */
	preview: string;

	/** Full tool result data for detailed panel view */
	data?: unknown;

	/** Tool arguments used (for debugging/power users) */
	args?: Record<string, unknown>;

	/** Web search specific: URL of source */
	url?: string;

	/** Web search specific: Title of source */
	title?: string;

	/** Timestamp when tool was called */
	timestamp?: string;
}

export interface CitationContext {
	/** All citations in order of appearance */
	citations: Citation[];

	/** Quick lookup by citation ID */
	byId: Map<string, Citation>;

	/** Quick lookup by tool_call_id */
	byToolCallId: Map<string, Citation>;
}

/**
 * Display information for a source type
 */
export interface DisplayInfo {
	icon: string;
	color: string;
	label: string;
}
