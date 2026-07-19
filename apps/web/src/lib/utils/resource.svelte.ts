/**
 * createResource — a tiny async-data rune.
 *
 * Collapses the repeated per-view lifecycle boilerplate
 *
 *     let loading = $state(true);
 *     let error = $state<string | null>(null);
 *     onMount(load);
 *     async function load() {
 *       loading = true; error = null;
 *       try { data = await fetchX(); }
 *       catch (e) { error = e instanceof Error ? e.message : "Failed"; }
 *       finally { loading = false; }
 *     }
 *
 * into a single reactive object. Pair with the shared LoadingState /
 * ErrorState / EmptyState components:
 *
 *     const users = createResource(() => listUsers());
 *     // {#if users.loading}<LoadingState/>
 *     //  {:else if users.error}<ErrorState message={users.error} onRetry={users.reload}/>
 *     //  {:else if !users.data?.length}<EmptyState .../>
 *     //  {:else} ...users.data... {/if}
 *
 * For id-driven detail views, re-run on change:
 *     $effect(() => { id; resource.reload(); });
 */

export interface Resource<T> {
	/** The last successfully loaded value, or `initial` / undefined before first load. */
	readonly data: T | undefined;
	/** True while a load is in flight. */
	readonly loading: boolean;
	/** Error message from the most recent failed load, else null. */
	readonly error: string | null;
	/** Re-run the loader. Resolves when the load settles. */
	reload(): Promise<void>;
}

export interface ResourceOptions<T> {
	/** Seed value shown before the first load resolves. */
	initial?: T;
	/** Kick off the first load immediately (default true). Set false to defer to reload(). */
	immediate?: boolean;
	/** Fallback message when a thrown error has no `.message`. */
	errorMessage?: string;
}

export function createResource<T>(
	loader: () => Promise<T>,
	opts: ResourceOptions<T> = {},
): Resource<T> {
	const immediate = opts.immediate !== false;
	let data = $state<T | undefined>(opts.initial);
	let loading = $state(immediate);
	let error = $state<string | null>(null);

	async function reload(): Promise<void> {
		loading = true;
		error = null;
		try {
			data = await loader();
		} catch (e) {
			error = e instanceof Error ? e.message : (opts.errorMessage ?? 'Failed to load');
		} finally {
			loading = false;
		}
	}

	if (immediate) void reload();

	return {
		get data() {
			return data;
		},
		get loading() {
			return loading;
		},
		get error() {
			return error;
		},
		reload,
	};
}
