/**
 * Short self-introductions for built-in templates and system pipelines.
 *
 * Used as the fallback excerpt on action cards when there's no real
 * last-successful output to show yet.
 *
 * TODO: promote this to a `description` field in actions/templates.toml
 * and serve it from the API so this map can go away.
 */
export const ACTION_DESCRIPTIONS: Record<string, string> = {
	day_illustration: 'Paints a small illustration of your day once it has enough to draw from.',
	day_summary_eod:
		'Scores novelty, resolves sleep, and writes the autobiography entry for each finished day.',
	trash_purge: 'Sweeps the drive trash each night.',
	embedding_index: 'Keeps your semantic index fresh so search finds what you meant.',
	transcription_resolution: 'Cleans up dictated audio into readable prose when it arrives.',
	credential_refresh: 'Refreshes expiring OAuth tokens ahead of time.',
	ios_ingest:
		'Receives every stream from your paired iPhone — health, location, calendar, contacts, audio, and finance.'
};

/**
 * Return the best introduction blurb for an action.
 * Priority: hardcoded map → first sentence of `agent` → null.
 */
export function descriptionFor(action: {
	function_name?: string | null;
	agent?: string | null;
}): string | null {
	const fn = action.function_name ?? '';
	if (fn && ACTION_DESCRIPTIONS[fn]) return ACTION_DESCRIPTIONS[fn];
	if (action.agent) {
		const first = action.agent
			.split(/[.!?\n]/)
			.map((s) => s.trim())
			.find(Boolean);
		if (first) return first.length > 160 ? first.slice(0, 157) + '…' : first;
	}
	return null;
}
