/**
 * Agent factory for creating ToolLoopAgent instances
 */
import { ToolLoopAgent, stepCountIs } from 'ai';
import type { AgentMetadata } from './types';
import { buildInstructions } from './instructions';
import { filterToolsByPreferences, type ToolPreferences } from '../tools/preferences';
import type { ToolRegistry } from '../tools/types';

/**
 * Build provider-specific options for reasoning/thinking based on model
 * This enables extended thinking for Claude, reasoning for OpenAI, and thinking for Gemini
 */
function buildProviderOptions(model: string) {
	const provider = model.split('/')[0];
	const modelName = model.split('/')[1] || '';

	switch (provider) {
		case 'anthropic':
			// Enable extended thinking for Claude models
			return {
				anthropic: {
					thinking: { type: 'enabled', budgetTokens: 10000 },
				},
			};
		case 'openai':
			// Enable reasoning for OpenAI models
			return {
				openai: {
					reasoningEffort: 'medium',
					reasoningSummary: 'auto',
				},
			};
		case 'google':
			// Enable thinking for Gemini models
			// enableThinking: activates reasoning
			// includeThoughts: streams the reasoning to client
			if (modelName.includes('gemini-3')) {
				return {
					google: {
						thinkingConfig: {
							enableThinking: true,
							includeThoughts: true,
						},
					},
				};
			} else if (modelName.includes('gemini-2.5')) {
				return {
					google: {
						thinkingConfig: {
							enableThinking: true,
							includeThoughts: true,
							thinkingBudget: 8000,
						},
					},
				};
			}
			return undefined;
		default:
			return undefined;
	}
}

/**
 * Create a ToolLoopAgent from metadata
 *
 * @param metadata - Agent configuration
 * @param allTools - All available tools
 * @param userName - User's name for personalization
 * @param assistantName - Assistant's configured name
 * @param toolPreferences - Optional user tool preferences for filtering
 * @returns Configured ToolLoopAgent instance
 */
export function createAgent(
	metadata: AgentMetadata,
	allTools: ToolRegistry,
	userName: string,
	assistantName: string = 'Ari',
	toolPreferences?: ToolPreferences
): ToolLoopAgent {
	// Determine which tools this agent gets
	// All agents get the same tools, filtered by user preferences
	const agentTools: ToolRegistry = toolPreferences
		? filterToolsByPreferences(allTools, toolPreferences)
		: { ...allTools };

	// Build agent instructions
	const instructions = buildInstructions(metadata.id, userName, assistantName);

	// Build provider options for thinking/reasoning based on model
	const providerOptions = buildProviderOptions(metadata.defaultModel);

	// Create ToolLoopAgent with provider options for thinking/reasoning
	// eslint-disable-next-line @typescript-eslint/no-explicit-any
	const agent = new ToolLoopAgent({
		id: `virtues-${metadata.id}`,
		model: metadata.defaultModel,
		instructions,
		tools: agentTools,
		stopWhen: stepCountIs(metadata.maxSteps || 5),
		maxRetries: 3,
		...(providerOptions && { providerOptions: providerOptions as any }),
	});

	console.log(`[AgentFactory] Created ${metadata.name} (model: ${metadata.defaultModel}, tools: ${Object.keys(agentTools).length})`);

	return agent;
}
