/**
 * Ontology display helpers.
 *
 * Maps backend `source_type` strings (calendar, sleep, message:slack, …)
 * to user-facing ontology names.
 */

const ONTOLOGY_NAMES: Record<string, string> = {
	calendar: "Calendar Events",
	email: "Email",
	email_sent: "Email",
	location: "Location Visits",
	workout: "Workouts",
	sleep: "Sleep Sessions",
	transaction: "Financial Transactions",
	transcription: "Voice Transcriptions",
	steps: "Steps",
	heart_rate: "Heart Rate",
	hrv: "Heart Rate Variability",
	chat: "Chat Sessions",
	page: "Page Edits",
	listening: "Listening History",
	app_usage: "App Usage",
	web_browsing: "Web Browsing",
	document: "Documents",
	bookmark: "Bookmarks",
};

/** Map a source_type back to its ontology display name. */
export function getOntologyName(sourceType: string): string {
	// "message:slack", "message:#design-team" etc. → Messages
	if (sourceType.startsWith("message:")) return "Messages";
	return ONTOLOGY_NAMES[sourceType] ?? sourceType;
}
