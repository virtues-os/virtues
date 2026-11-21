/**
 * What your server asks you, for the Inbox. The list is read again after
 * every answer, because one answer can settle another question too: saying
 * a place is home ends the question about where home is.
 */
import { toast } from 'svelte-sonner';
import { fetchPlaceQuestions, placeQuestion, sendPlaceAnswer, type PlaceAnswer, type Question } from '$lib/inbox/questions';

class QuestionsStore {
	items = $state<Question[]>([]);
	loaded = $state(false);

	async load(): Promise<void> {
		try {
			this.items = (await fetchPlaceQuestions()).map(placeQuestion);
		} catch (e) {
			// A failed read keeps the last list. The Inbox still shows everything else.
			console.warn('[questions] could not read the list:', e);
		} finally {
			this.loaded = true;
		}
	}

	/** Save an answer. The question leaves at once and comes back if the save fails. */
	async answer(q: Question, answer: PlaceAnswer): Promise<void> {
		const before = this.items;
		this.items = before.filter((x) => x.key !== q.key);
		try {
			await sendPlaceAnswer(q.place.id, answer);
		} catch (e) {
			console.error('[questions] answer failed:', e);
			this.items = before;
			toast.error("Your server couldn't save that answer", { description: 'Nothing changed. Try again' });
			return;
		}
		await this.load();
	}
}

export const questionsStore = new QuestionsStore();
