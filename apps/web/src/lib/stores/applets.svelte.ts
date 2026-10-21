/**
 * The applets list, held once for every surface that shows it: the Applets
 * table, the Home sidebar's list under Applets, and the problem badge on the
 * Home rail tile. Each used to fetch on its own, and three copies of one list
 * can disagree about whether something is broken.
 */
import { listApplets, type Applet } from '$lib/api/client';
import { needsYou } from '$lib/applets/days';

/** Made by you, in a chat or by hand, rather than shipped with the server. */
const isMine = (a: Applet) => a.origin === 'user' || a.origin === 'ai';

class AppletsStore {
	list = $state<Applet[]>([]);
	loaded = $state(false);
	error = $state<string | null>(null);
	private inflight: Promise<void> | null = null;

	/** Applets you made, newest problems first, then by name. The built-in and
	 *  source applets stay in Settings > Applets. */
	mine = $derived(
		this.list
			.filter((a) => isMine(a) && !a.archived_at)
			.sort((a, b) => Number(needsYou(b)) - Number(needsYou(a)) || a.name.localeCompare(b.name))
	);

	/** Load, or join the load already in flight. */
	load(): Promise<void> {
		this.inflight ??= listApplets()
			.then((list) => {
				this.list = list;
				this.loaded = true;
				this.error = null;
			})
			.catch((e) => {
				this.error = e instanceof Error ? e.message : String(e);
			})
			.finally(() => {
				this.inflight = null;
			});
		return this.inflight;
	}
}

export const appletsStore = new AppletsStore();
