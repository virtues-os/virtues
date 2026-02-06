/**
 * AI Persona Configuration
 *
 * @deprecated This file is deprecated. Use the persona store instead:
 * import { personaStore } from '$lib/stores/personas.svelte';
 *
 * Personas are now fetched from the backend API and stored in the database.
 * Users can customize existing personas and create new ones.
 */

/**
 * @deprecated Use string type instead. Persona IDs are now dynamic.
 */
export type PersonaId = string;

/**
 * @deprecated Use Persona type from the store instead.
 */
export interface PersonaOption {
	id: string;
	name: string;
	description: string;
	icon: string;
}

/**
 * @deprecated Use personaStore.personas instead.
 * This constant is kept for backwards compatibility only.
 * Personas are now fetched from /api/personas and stored in the database.
 */
export const PERSONAS: PersonaOption[] = [
	{
		id: 'standard',
		name: 'Standard',
		description: 'Neutral and capable',
		icon: 'ri:user-line'
	},
	{
		id: 'concierge',
		name: 'Concierge',
		description: 'Anticipatory and proactive',
		icon: 'ri:service-line'
	},
	{
		id: 'analyst',
		name: 'Analyst',
		description: 'Structured and thorough',
		icon: 'ri:line-chart-line'
	},
	{
		id: 'coach',
		name: 'Coach',
		description: 'Supportive and growth-focused',
		icon: 'ri:graduation-cap-line'
	}
];

/**
 * @deprecated Use personaStore.getById() instead
 */
export function getPersonaById(id: string): PersonaOption | undefined {
	return PERSONAS.find((p) => p.id === id);
}

/**
 * @deprecated Use personaStore.personas[0] instead
 */
export function getDefaultPersona(): PersonaOption {
	return PERSONAS[0];
}
