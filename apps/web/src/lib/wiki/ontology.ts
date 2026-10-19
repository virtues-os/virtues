/**
 * Ontology display helpers.
 *
 * Maps backend `source_type` strings (calendar, sleep, message:slack, …)
 * to user-facing ontology names, in sentence case. A type with no name here
 * shows its own word, capitalized ("audio" reads "Audio").
 */

const ONTOLOGY_NAMES: Record<string, string> = {
	calendar: "Calendar events",
	email: "Email",
	email_sent: "Email",
	location: "Location visits",
	workout: "Workouts",
	sleep: "Sleep sessions",
	transaction: "Financial transactions",
	transcription: "Voice transcriptions",
	steps: "Steps",
	heart_rate: "Heart rate",
	hrv: "Heart rate variability",
	chat: "Chat sessions",
	page: "Page edits",
	app_usage: "App usage",
	web_browsing: "Web browsing",
	document: "Documents",
	bookmark: "Bookmarks",
};

/** Map a source_type back to its ontology display name. */
export function getOntologyName(sourceType: string): string {
	// "message:slack", "message:#design-team" etc. → Messages
	if (sourceType.startsWith("message:")) return "Messages";
	return ONTOLOGY_NAMES[sourceType] ?? sourceType.charAt(0).toUpperCase() + sourceType.slice(1).replace(/_/g, " ");
}
