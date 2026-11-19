/**
 * What an AI call was for, in words: the call log's feature slug
 * (`app_ai_calls.feature`) as the cost page names it, in its kinds and in
 * each call's row.
 */

const NAMES: Record<string, string> = {
	chat: 'Chat',
	chat_title: 'Chat titles',
	chat_opening: 'Chat openings',
	day_summary: 'Day pages',
	task: 'Tasks',
	applet: 'Applets',
	settings_draft: 'Settings',
	style_preview: 'Style samples',
	bookmark_enrichment: 'Bookmarks',
	suggested_questions: 'Suggested questions',
	interview: 'Interview',
	entity_article: 'Wiki articles',
	other: 'Other',
};

/** `day_summary` → "Day pages"; a slug with no name of its own reads as its words. */
export function featureLabel(slug: string): string {
	const named = NAMES[slug];
	if (named) return named;
	const words = slug.replace(/[_-]+/g, ' ').trim();
	return words ? words.charAt(0).toUpperCase() + words.slice(1) : 'Other';
}
