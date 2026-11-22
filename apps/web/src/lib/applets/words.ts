/**
 * The word a person sees for something the assistant set up. A thing with a
 * view is a site, a thing that starts itself is a task, and a prompt started
 * from chat is a skill. Null when what is known does not say which, such as
 * a setup call that changes one key of something made earlier.
 */
export type MadeWord = 'site' | 'task' | 'skill';

/** From the setup call's own input, while the call is still running. */
export function wordForSetup(input: Record<string, unknown>): MadeWord | null {
	if (typeof input.view === 'string' && input.view) return 'site';
	if (typeof input.trigger === 'string' && input.trigger) return 'task';
	if (Array.isArray(input.invoke) && input.invoke.includes('chat')) return 'skill';
	return null;
}

/** From what setup reported back. A result stored before setup reported `has_view` lists "has a view" among what it can do. */
export function wordForMade(output: Record<string, unknown>): MadeWord {
	const can = Array.isArray(output.capabilities) ? output.capabilities : [];
	if (output.has_view === true || can.includes('has a view')) return 'site';
	// A task row means it starts itself, by a schedule, a data watch or a webhook.
	if ((typeof output.schedule === 'string' && output.schedule) || (typeof output.task_id === 'string' && output.task_id)) return 'task';
	return 'skill';
}
