import { describe, expect, it } from 'vitest';
import { displayNote, LANE_CAP, orderNotes } from './memoryNote';

const NICK = 'person_0123456789abcdef';
const DAVID = 'person_fedcba9876543210';
const HOME = 'place_00112233445566ff';
const ORG = 'org_a0a0a0a0b1b1b1b1';

describe('displayNote', () => {
	it('drops an id in brackets with the space before it', () => {
		expect(displayNote(`Nick (${NICK}) is your brother.`)).toBe('Nick is your brother.');
		expect(displayNote(`Nick [${NICK}] is your brother.`)).toBe('Nick is your brother.');
		expect(displayNote(`Nick (${NICK})'s dog is Pepper.`)).toBe("Nick's dog is Pepper.");
	});

	it('keeps what else the bracket held', () => {
		expect(displayNote(`Nick (${NICK}, +15125550100) is your brother.`)).toBe(
			'Nick (+15125550100) is your brother.',
		);
		expect(displayNote(`Nick (+15125550100; ${NICK}) is your brother.`)).toBe(
			'Nick (+15125550100) is your brother.',
		);
	});

	it('drops every id in a note', () => {
		expect(
			displayNote(`Nick (${NICK}) and David Okafor (${DAVID}) work at Canopy (${ORG}).`),
		).toBe('Nick and David Okafor work at Canopy.');
		expect(displayNote(`Nick and David Okafor (${NICK}, ${DAVID}) are cousins.`)).toBe(
			'Nick and David Okafor are cousins.',
		);
	});

	it('drops a bare id, a labeled id, and an id set off by commas', () => {
		expect(displayNote(`You live at the house ${HOME} on Elm.`)).toBe(
			'You live at the house on Elm.',
		);
		expect(displayNote(`Nick (id: ${NICK}) is your brother.`)).toBe('Nick is your brother.');
		expect(displayNote(`Nick, ${NICK}, is your brother.`)).toBe('Nick is your brother.');
		expect(displayNote(`Your brother is Nick, ${NICK}.`)).toBe('Your brother is Nick.');
		expect(displayNote(`Your brother: ${NICK}, Nick.`)).toBe('Your brother: Nick.');
	});

	it('reads a record link or route as a name', () => {
		expect(displayNote(`[Nick](/person/${NICK}) is your brother.`)).toBe('Nick is your brother.');
		expect(displayNote(`Nick (/person/${NICK}) is your brother.`)).toBe('Nick is your brother.');
	});

	it('leaves a note with no ids exactly as written', () => {
		const note = 'Short answers ,  unless you ask (for more).';
		expect(displayNote(note)).toBe(note);
		expect(displayNote('Your sister lives in Denver.')).toBe('Your sister lives in Denver.');
	});

	it('leaves words that only look like ids', () => {
		expect(displayNote('Your notes live in my_notes and file_names.')).toBe(
			'Your notes live in my_notes and file_names.',
		);
		// 15 and 17 hex digits are not an id the server mints.
		expect(displayNote('Code person_0123456789abcde stays.')).toBe(
			'Code person_0123456789abcde stays.',
		);
		expect(displayNote('Code person_0123456789abcdef0 stays.')).toBe(
			'Code person_0123456789abcdef0 stays.',
		);
	});

	it('keeps a note that is nothing but an id as written', () => {
		expect(displayNote(NICK)).toBe(NICK);
	});

	it('keeps the lines of a note', () => {
		expect(displayNote(`Nick (${NICK}) is your brother.\nCall him on Sundays.`)).toBe(
			'Nick is your brother.\nCall him on Sundays.',
		);
	});
});

describe('orderNotes', () => {
	const note = (id: number, lane: string, created_at: string) => ({ id, lane, created_at });

	it('lists facts, then manner, then practices, each oldest first', () => {
		const ordered = orderNotes([
			note(1, 'practices', '2026-01-01T00:00:00Z'),
			note(2, 'manner', '2026-03-01T00:00:00Z'),
			note(3, 'facts', '2026-04-01T00:00:00Z'),
			note(4, 'manner', '2026-02-01T00:00:00Z'),
			note(5, 'facts', '2026-01-15T00:00:00Z'),
		]);
		expect(ordered.map((n) => n.id)).toEqual([5, 3, 4, 2, 1]);
	});

	it('breaks a tie on id and puts an unknown lane last', () => {
		const at = '2026-01-01T00:00:00Z';
		const ordered = orderNotes([note(9, 'other', at), note(7, 'facts', at), note(6, 'facts', at)]);
		expect(ordered.map((n) => n.id)).toEqual([6, 7, 9]);
	});
});

describe('LANE_CAP', () => {
	it('matches the server', () => {
		// virtues-core/src/api/assistant_memories.rs `lane_cap`
		expect(LANE_CAP).toEqual({ facts: 12, manner: 8, practices: 8 });
	});
});
