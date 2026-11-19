import { describe, expect, it } from 'vitest';
import { mentionableUrl } from './projectActions';

describe('mentionableUrl', () => {
	it('takes one thing, a record, or a link out', () => {
		expect(mentionableUrl('/person/person_1')).toBe('/person/person_1');
		expect(mentionableUrl('/record/calendar_event/evt_1')).toBe('/record/calendar_event/evt_1');
		expect(mentionableUrl('/record/location_visit/vis_1?from=12:00')).toBe('/record/location_visit/vis_1');
		expect(mentionableUrl('https://example.com/a/b')).toBe('https://example.com/a/b');
	});

	it('leaves out routes that name no single thing', () => {
		expect(mentionableUrl('/settings/display/faces')).toBeNull();
		expect(mentionableUrl('/pages')).toBeNull();
	});
});
