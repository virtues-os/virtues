import { z } from 'zod';
import { env } from '$env/dynamic/private';

/**
 * Exa Web Search Tool for AI SDK v6
 *
 * Proxies web search through virtues-core which handles:
 * - Exa API calls
 * - Usage metering and rate limiting
 *
 * @see https://docs.exa.ai for API documentation
 */
export async function createWebSearchTool() {
	// Get the API URL from env (defaults to localhost for dev)
	const apiUrl = env.ELT_API_URL || 'http://localhost:8000';

	const inputSchema = z.object({
		query: z.string().describe('The search query to find relevant web content'),
		numResults: z
			.number()
			.min(1)
			.max(10)
			.optional()
			.describe('Number of search results to return (default: 5, max: 10)'),
		type: z
			.enum(['auto', 'keyword', 'neural'])
			.optional()
			.describe('Search type: auto (intelligent hybrid), keyword (exact matches), neural (semantic search). Default: auto'),
		category: z
			.enum(['company', 'research paper', 'news', 'pdf', 'github', 'personal site', 'linkedin profile', 'financial report'])
			.optional()
			.describe('Filter results by content category'),
		includeDomains: z
			.array(z.string())
			.optional()
			.describe('Only include results from these domains (e.g., ["github.com", "stackoverflow.com"])'),
		excludeDomains: z
			.array(z.string())
			.optional()
			.describe('Exclude results from these domains'),
		startPublishedDate: z
			.string()
			.optional()
			.describe('Filter results published after this date (ISO 8601 format)'),
		endPublishedDate: z
			.string()
			.optional()
			.describe('Filter results published before this date (ISO 8601 format)')
	});

	return {
		description:
			'Search the web using Exa AI. Returns relevant, high-quality web search results with content summaries. Best for finding recent information, research papers, company info, or specific domain knowledge.',
		inputSchema,
		execute: async ({
			query,
			numResults = 5,
			type = 'auto',
			category,
			includeDomains,
			excludeDomains,
			startPublishedDate,
			endPublishedDate
		}: z.infer<typeof inputSchema>) => {
			console.log('[webSearch] Executing search via core:', { query, numResults, type, category });

			try {
				// Build the request body for core's /api/search/web endpoint
				const requestBody: Record<string, unknown> = {
					query,
					numResults,
					searchType: type
				};

				// Add optional filters (using camelCase to match Rust serde)
				if (category) requestBody.category = category;
				if (includeDomains && includeDomains.length > 0) {
					requestBody.includeDomains = includeDomains;
				}
				if (excludeDomains && excludeDomains.length > 0) {
					requestBody.excludeDomains = excludeDomains;
				}
				if (startPublishedDate) requestBody.startPublishedDate = startPublishedDate;
				if (endPublishedDate) requestBody.endPublishedDate = endPublishedDate;

				// Call core's search endpoint (handles Exa API + usage tracking)
				const response = await fetch(`${apiUrl}/api/search/web`, {
					method: 'POST',
					headers: {
						'Content-Type': 'application/json'
					},
					body: JSON.stringify(requestBody)
				});

				if (!response.ok) {
					const errorData = await response.json().catch(() => ({ error: 'Unknown error' }));
					console.error('[webSearch] API error:', response.status, errorData);

					// Handle rate limit errors specifically
					if (response.status === 429) {
						return {
							success: false,
							error: errorData.message || 'Monthly search limit reached',
							limitExceeded: true,
							resetsAt: errorData.resets_at,
							query,
							resultsCount: 0,
							results: []
						};
					}

					throw new Error(errorData.error || `API error (${response.status})`);
				}

				const data = await response.json();
				console.log(`[webSearch] Found ${data.resultsCount || 0} results`);

				// Format results for the AI (core already returns formatted results)
				const formattedResults = (data.results || []).map((result: any, index: number) => ({
					position: index + 1,
					title: result.title,
					url: result.url,
					publishedDate: result.publishedDate,
					author: result.author,
					summary: result.summary,
					text: result.text?.substring(0, 500),
					score: result.score
				}));

				return {
					success: true,
					type: 'web_search_results',
					query,
					resultsCount: formattedResults.length,
					searchType: data.searchType || type,
					results: formattedResults,
					metadata: {
						autopromptString: data.autopromptString,
						requestId: data.requestId
					}
				};
			} catch (error: any) {
				console.error('[webSearch] Error:', error.message);
				console.error('[webSearch] Stack:', error.stack);

				return {
					success: false,
					error: error.message,
					query,
					resultsCount: 0,
					results: []
				};
			}
		}
	};
}
