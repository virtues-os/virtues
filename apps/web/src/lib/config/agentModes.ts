/**
 * Agent Mode Configuration
 *
 * Defines available agent modes that control tool availability.
 * Similar to Cursor's Agent/Chat modes.
 */

export type AgentModeId = 'agent' | 'chat' | 'research';

export interface AgentMode {
	id: AgentModeId;
	name: string;
	description: string;
	icon: string;
	/** Background color for pill indicator (null = no background) */
	color: string | null;
	/** Maximum tool calls per turn (0 = no tools) */
	maxToolCalls: number;
	/** Tool category access */
	tools: {
		search: boolean;
		data: boolean;
		edit: boolean;
	};
}

export const AGENT_MODES: AgentMode[] = [
	{
		id: 'agent',
		name: 'Agent',
		description: 'All tools',
		icon: 'ri:infinity-line',
		color: null, // No background for default
		maxToolCalls: 20,
		tools: { search: true, data: true, edit: true }
	},
	{
		id: 'chat',
		name: 'Chat',
		description: 'No tools',
		icon: 'ri:chat-3-line',
		color: 'var(--color-success)',
		maxToolCalls: 0,
		tools: { search: false, data: false, edit: false }
	},
	{
		id: 'research',
		name: 'Research',
		description: 'Read-only',
		icon: 'ri:search-eye-line',
		color: 'var(--color-warning)',
		maxToolCalls: 50,
		tools: { search: true, data: true, edit: false }
	}
];

export function getModeById(id: AgentModeId): AgentMode | undefined {
	return AGENT_MODES.find((m) => m.id === id);
}

export function getDefaultMode(): AgentMode {
	return AGENT_MODES[0];
}

/**
 * Get the next mode in the cycle (for Shift+Tab)
 */
export function getNextMode(currentId: AgentModeId): AgentMode {
	const currentIndex = AGENT_MODES.findIndex((m) => m.id === currentId);
	const nextIndex = (currentIndex + 1) % AGENT_MODES.length;
	return AGENT_MODES[nextIndex];
}
