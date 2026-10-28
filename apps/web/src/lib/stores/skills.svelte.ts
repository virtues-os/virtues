/**
 * The skills the composer's `/` menu offers, loaded once and refreshed each
 * time the menu opens, so a skill saved in a chat shows up the next time.
 *
 * ChatView reads `names` to tell a message that starts with `/weekly-review`
 * from one that starts with a slash for any other reason.
 */

import { listSkills, type SkillSummary } from '$lib/api/client';

class SkillsStore {
	items = $state<SkillSummary[]>([]);
	create = $state<SkillSummary | null>(null);
	loaded = $state(false);
	failed = $state(false);

	/** Every skill name a turn may run as, the create skill included. */
	readonly names = $derived(
		new Set([...this.items.map((s) => s.name), ...(this.create ? [this.create.name] : [])]),
	);

	async load(): Promise<void> {
		try {
			const list = await listSkills();
			this.items = list.skills;
			this.create = list.create ?? null;
			this.failed = false;
		} catch (e) {
			// A box older than the skills API answers 404; the menu then says
			// it could not load skills, and a leading `/word` sends as text.
			console.error('Loading skills failed:', e);
			this.failed = true;
		} finally {
			this.loaded = true;
		}
	}
}

export const skills = new SkillsStore();
