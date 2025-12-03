/**
 * Agent configuration definitions
 */
import type { AgentMetadata } from './types';

/**
 * Agent configurations
 * 2-agent system:
 * - agent: Has all tools, can perform actions (general assistant)
 * - onboarding: Has save_axiology tool for values discovery
 */
export const AGENT_CONFIGS: AgentMetadata[] = [
	{
		id: 'agent',
		name: 'Agent',
		description: 'Intelligent assistant with access to all available tools. Can query data, search the web, visualize information, and help with tasks.',
		color: '#6b7280', // Gray
		icon: '🤖',
		defaultModel: 'google/gemini-3-pro-preview',
		maxSteps: 5,
		enabled: true,
	},
	{
		id: 'onboarding',
		name: 'Onboarding Guide',
		description: 'Warm, exploratory assistant that helps users discover their values, goals, and what matters most to them through natural conversation.',
		color: '#8b5cf6', // Purple
		icon: '🌟',
		defaultModel: 'google/gemini-3-pro-preview',
		maxSteps: 5,
		enabled: true,
	},
];

/**
 * Get agent configuration by ID
 * @param id - Agent ID
 * @returns Agent metadata or undefined if not found
 */
export function getAgentConfig(id: string): AgentMetadata | undefined {
	return AGENT_CONFIGS.find((config) => config.id === id);
}

/**
 * Get all enabled agent configurations
 * @returns Array of enabled agent metadata
 */
export function getEnabledAgentConfigs(): AgentMetadata[] {
	return AGENT_CONFIGS.filter((config) => config.enabled);
}

/**
 * Get default agent ID
 * @returns ID of the default agent
 */
export function getDefaultAgentId(): string {
	return 'agent';
}
