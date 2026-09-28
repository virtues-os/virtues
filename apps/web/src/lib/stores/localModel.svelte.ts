/**
 * Local mode's availability and its one-time model download.
 *
 * The box answers `GET /api/local-model` only when it knows local mode. That
 * matters: a box that doesn't would read the `local` mode as ordinary chat
 * and answer from the cloud. So the mode is offered only when this store has
 * heard `supported: true` from the box, and a 404 simply means "not here".
 */

import { request } from '$lib/api/client';

export interface LocalModelStatus {
	supported: boolean;
	ready: boolean;
	downloading: boolean;
	downloadedBytes: number;
	totalBytes: number;
	/** The last download's failure, in a sentence to show as it is. */
	error: string | null;
}

/** What one local turn measured, from the transient `data-local-stats` part. */
export interface LocalStats {
	promptTokens: number;
	generatedTokens: number;
	tokensPerSecond: number;
	secondsToFirstToken: number;
}

const UNSUPPORTED: LocalModelStatus = {
	supported: false,
	ready: false,
	downloading: false,
	downloadedBytes: 0,
	totalBytes: 0,
	error: null,
};

class LocalModelStore {
	status = $state<LocalModelStatus>(UNSUPPORTED);
	private loaded = false;
	private polling: ReturnType<typeof setTimeout> | null = null;

	/** Ask the box once per session; later calls reuse the answer. */
	async load(force = false) {
		if (this.loaded && !force) return;
		this.loaded = true;
		try {
			this.status = await request<LocalModelStatus>('/local-model');
		} catch {
			// A box without the route, or any failure: no local mode here.
			this.status = UNSUPPORTED;
		}
		if (this.status.downloading) this.poll();
	}

	async startDownload() {
		try {
			this.status = await request<LocalModelStatus>('/local-model', { method: 'POST' });
		} catch (e) {
			this.status = {
				...this.status,
				error: "Couldn't download the local model. Check your server's internet connection, then try again.",
			};
			return;
		}
		this.poll();
	}

	private poll() {
		if (this.polling) return;
		const tick = async () => {
			this.polling = null;
			try {
				this.status = await request<LocalModelStatus>('/local-model');
			} catch {
				return;
			}
			if (this.status.downloading) this.polling = setTimeout(tick, 1000);
		};
		this.polling = setTimeout(tick, 1000);
	}
}

export const localModel = new LocalModelStore();
