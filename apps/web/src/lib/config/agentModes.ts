/**
 * Agent Mode Configuration
 *
 * Defines available agent modes that control tool availability.
 * Similar to Cursor's Agent/Chat modes.
 */

export type AgentModeId = 'chat' | 'deep_research' | 'council';

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
		id: 'chat',
		name: 'Chat',
		description: 'Fast, acts on confirmation',
		icon: 'ri:chat-3-line',
		color: null, // No background for default
		maxToolCalls: 20,
		tools: { search: true, data: true, edit: true }
	},
	{
		id: 'deep_research',
		name: 'Deep Research',
		description: 'Investigate your life & the web',
		icon: 'ri:search-eye-line',
		color: 'var(--color-info)',
		maxToolCalls: 50,
		tools: { search: true, data: true, edit: false }
	},
	{
		id: 'council',
		name: 'Council',
		description: 'Weigh a hard decision from many perspectives',
		icon: 'ri:group-line',
		color: 'var(--color-warning)',
		maxToolCalls: 40,
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
