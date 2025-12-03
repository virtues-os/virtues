import { z } from 'zod';
import type { Pool } from 'pg';

/**
 * Schema for an individual axiology item (virtue, vice, temperament, preference)
 */
const axiologyItemSchema = z.object({
	title: z.string().describe('The name/title of the item'),
	description: z.string().optional().describe('Optional description or context')
});

/**
 * Tool for saving axiology items discovered through onboarding conversation
 * Returns a plain object compatible with AI SDK v6 tool format
 */
export async function createSaveAxiologyTool(pool: Pool) {
	const inputSchema = z.object({
		telos: axiologyItemSchema.optional().describe('The user\'s overarching life purpose or "north star"'),
		virtues: z.array(axiologyItemSchema).optional().describe('Character strengths the user wants to cultivate'),
		vices: z.array(axiologyItemSchema).optional().describe('Patterns or tendencies the user wants to overcome'),
		temperaments: z.array(axiologyItemSchema).optional().describe('Natural dispositions that shape how the user engages with the world'),
		preferences: z.array(axiologyItemSchema).optional().describe('Affinities, interests, and what brings the user joy'),
		mark_complete: z.boolean().optional().describe('If true, marks axiology discovery as complete and transitions to normal assistant mode')
	});

	return {
		description: 'Save axiology items (telos, virtues, vices, temperaments, preferences) discovered through conversation. Use this after the user agrees to save their framework. Set mark_complete=true when the discovery process is finished.',
		inputSchema,
		execute: async (input: z.infer<typeof inputSchema>) => {
			console.log('[saveAxiology] Executing with input:', JSON.stringify(input, null, 2));

			try {
				const savedItems = {
					telos: null as string | null,
					virtues: [] as string[],
					vices: [] as string[],
					temperaments: [] as string[],
					preferences: [] as string[]
				};

				// Save telos (only one active at a time)
				if (input.telos) {
					// Archive any existing active telos
					await pool.query(`
						UPDATE data.axiology_telos
						SET is_active = FALSE
						WHERE is_active = TRUE
					`);

					// Create new telos
					const telosResult = await pool.query(
						`INSERT INTO data.axiology_telos (title, description, is_active)
						 VALUES ($1, $2, TRUE)
						 RETURNING id`,
						[input.telos.title, input.telos.description || null]
					);
					savedItems.telos = telosResult.rows[0].id;
					console.log('[saveAxiology] Saved telos:', input.telos.title);
				}

				// Save virtues
				if (input.virtues && input.virtues.length > 0) {
					for (const virtue of input.virtues) {
						const result = await pool.query(
							`INSERT INTO data.axiology_virtue (title, description, is_active)
							 VALUES ($1, $2, TRUE)
							 RETURNING id`,
							[virtue.title, virtue.description || null]
						);
						savedItems.virtues.push(result.rows[0].id);
					}
					console.log('[saveAxiology] Saved', input.virtues.length, 'virtues');
				}

				// Save vices
				if (input.vices && input.vices.length > 0) {
					for (const vice of input.vices) {
						const result = await pool.query(
							`INSERT INTO data.axiology_vice (title, description, is_active)
							 VALUES ($1, $2, TRUE)
							 RETURNING id`,
							[vice.title, vice.description || null]
						);
						savedItems.vices.push(result.rows[0].id);
					}
					console.log('[saveAxiology] Saved', input.vices.length, 'vices');
				}

				// Save temperaments
				if (input.temperaments && input.temperaments.length > 0) {
					for (const temperament of input.temperaments) {
						const result = await pool.query(
							`INSERT INTO data.axiology_temperament (title, description, is_active)
							 VALUES ($1, $2, TRUE)
							 RETURNING id`,
							[temperament.title, temperament.description || null]
						);
						savedItems.temperaments.push(result.rows[0].id);
					}
					console.log('[saveAxiology] Saved', input.temperaments.length, 'temperaments');
				}

				// Save preferences
				if (input.preferences && input.preferences.length > 0) {
					for (const preference of input.preferences) {
						const result = await pool.query(
							`INSERT INTO data.axiology_preference (title, description, preference_domain, is_active)
							 VALUES ($1, $2, 'general', TRUE)
							 RETURNING id`,
							[preference.title, preference.description || null]
						);
						savedItems.preferences.push(result.rows[0].id);
					}
					console.log('[saveAxiology] Saved', input.preferences.length, 'preferences');
				}

				// Mark axiology as complete if requested
				if (input.mark_complete) {
					await pool.query(`
						UPDATE data.user_profile
						SET axiology_complete = TRUE
					`);
					console.log('[saveAxiology] Marked axiology as complete');
				}

				const totalSaved =
					(savedItems.telos ? 1 : 0) +
					savedItems.virtues.length +
					savedItems.vices.length +
					savedItems.temperaments.length +
					savedItems.preferences.length;

				return {
					success: true,
					message: `Successfully saved ${totalSaved} axiology items`,
					saved: savedItems,
					axiology_complete: input.mark_complete || false
				};
			} catch (error: any) {
				console.error('[saveAxiology] Error:', error);
				return {
					success: false,
					error: error.message || 'Failed to save axiology items'
				};
			}
		}
	};
}
