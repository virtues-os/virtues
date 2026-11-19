// What an AI call was for, as the cost page names it.
import { describe, expect, it } from 'vitest';
import { featureLabel } from './aiFeatures';

describe('featureLabel', () => {
	it('names the known kinds of work', () => {
		expect(featureLabel('day_summary')).toBe('Day pages');
		expect(featureLabel('settings_draft')).toBe('Settings');
		expect(featureLabel('chat')).toBe('Chat');
	});

	it('reads an unnamed slug as its words', () => {
		expect(featureLabel('call_audio_channel_test')).toBe('Call audio channel test');
		expect(featureLabel('new-skill')).toBe('New skill');
		expect(featureLabel('')).toBe('Other');
	});
});
