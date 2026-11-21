// The word for what the assistant set up. Every name here is invented.
import { describe, expect, it } from 'vitest';
import { wordForMade, wordForSetup } from './words';

describe('the word for what the assistant set up', () => {
	it('reads a running setup call: a view is a site, a trigger is a task, a chat start is a skill', () => {
		expect(wordForSetup({ name: 'Water log', view: '<!doctype html>', trigger: 'every day at 06:00' })).toBe('site');
		expect(wordForSetup({ name: 'Week in review', trigger: 'every sunday at 18:00', prompt: 'Write the week.' })).toBe('task');
		expect(wordForSetup({ name: 'Packing list', prompt: 'List what to pack.', invoke: ['chat', 'assistant'] })).toBe('skill');
	});

	it('says nothing when the call changes one key of something made earlier', () => {
		expect(wordForSetup({ name: 'Water log', limit: '$1 per day' })).toBeNull();
	});

	it('reads a finished setup, old results included', () => {
		expect(wordForMade({ has_view: true, schedule: '0 0 6 * * *' })).toBe('site');
		expect(wordForMade({ capabilities: ['has a view'] })).toBe('site');
		expect(wordForMade({ has_view: false, schedule: '0 0 18 * * 0' })).toBe('task');
		expect(wordForMade({ has_view: false, schedule: null })).toBe('skill');
	});
});
