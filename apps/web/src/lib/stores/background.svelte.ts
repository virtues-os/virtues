/**
 * The home background: the setting, the installed styles, and the picture
 * they resolve to for today and the current appearance.
 *
 * The setting is stored on the server in `ui_preferences.background`, so
 * every device shows the same picture, and cached in localStorage with the
 * day's resolved picture so the first frame paints it without waiting on the
 * network. The rules for a setting are in lib/backgrounds/resolve.ts.
 *
 * Under a rotating setting the home control's arrows choose another picture
 * for the rest of the day (`ui_preferences.background_today`), and rotation
 * resumes the next day. What the arrows do is in lib/backgrounds/browse.ts,
 * which loads with the home control rather than at launch.
 */

import { backendUrl } from '$lib/config/backend';
import { getAssistantProfile, request, updateUiPreferences } from '$lib/api/client';
import { getTheme, isThemeDark } from '$lib/utils/theme';
import {
	dayNumber,
	find,
	formatAddress,
	parseSetting,
	pickForDay,
	pool,
	srcset,
	widest,
	type Address,
	type Appearance,
	type AvailableStyle,
	type DownloadProgress,
	type Picture,
	type StyleManifest,
} from '$lib/backgrounds/resolve';

const SETTING_KEY = 'virtues-background';
const PICTURE_KEY = 'virtues-background-picture';

/** What to draw. `lake` is the bundled default; `none` draws nothing. */
export type Shown =
	| { kind: 'lake' }
	| { kind: 'none' }
	| { kind: 'picture'; key: string; avif: string; webp: string; src: string; title: string };

/** Another picture for one day of a rotating setting. */
interface DayChoice {
	setting: string;
	day: number;
	address: string;
}

function isDayChoice(v: unknown): v is DayChoice {
	const o = v as DayChoice | null;
	return !!o && typeof o.setting === 'string' && typeof o.day === 'number' && typeof o.address === 'string';
}

interface CachedPicture {
	setting: string;
	day: number;
	address: string;
	picture: Picture;
}

/** The URL of a style's file, from its path in the manifest. */
export function fileUrl(style: string) {
	return (rel: string) => backendUrl(`/api/backgrounds/files/${style}/${rel}`);
}

function readStorage(key: string): string | null {
	try {
		return localStorage.getItem(key);
	} catch {
		return null;
	}
}

function writeStorage(key: string, value: string | null): void {
	try {
		if (value === null) localStorage.removeItem(key);
		else localStorage.setItem(key, value);
	} catch {
		// Storage disabled: the server still has the setting.
	}
}

function appearance(): Appearance {
	return typeof window !== 'undefined' && isThemeDark(getTheme()) ? 'dark' : 'light';
}

function readCachedPicture(): CachedPicture | null {
	const raw = readStorage(PICTURE_KEY);
	if (!raw) return null;
	try {
		return JSON.parse(raw) as CachedPicture;
	} catch {
		return null;
	}
}

class BackgroundStore {
	setting = $state(typeof window === 'undefined' ? '' : (readStorage(SETTING_KEY) ?? ''));
	styles = $state<StyleManifest[] | null>(null);
	appearance = $state<Appearance>(appearance());
	day = $state(dayNumber(new Date()));
	dayChoice = $state<DayChoice | null>(null);
	/** Styles the server can download, downloads in progress, and failed ones. */
	available = $state<AvailableStyle[]>([]);
	installing = $state<Record<string, DownloadProgress>>({});
	failed = $state<Record<string, string>>({});
	/** Whether the server was fetching its catalog at the last listing. */
	catalogPending = $state(false);
	/** Whether the last attempt to load the styles failed, before any loaded. */
	stylesUnavailable = $state(false);
	#cached = typeof window === 'undefined' ? null : readCachedPicture();
	#started = false;
	/** Set once the person changes the setting here, so a late profile does not undo it. */
	#changedHere = false;
	#midnight: ReturnType<typeof setTimeout> | null = null;

	/** Today's picture for the setting, or null when the setting has none. */
	readonly today = $derived.by((): { address: string; picture: Picture } | null => {
		const setting = parseSetting(this.setting);
		if (setting.kind !== 'path') return null;
		if (this.styles === null) {
			// Not loaded yet: the picture cached for this setting today, if any.
			const c = this.#cached;
			return c && c.setting === this.setting && c.day === this.day ? { address: c.address, picture: c.picture } : null;
		}
		const items = this.pool;
		const choice = this.dayChoice;
		const chosen =
			choice && choice.setting === this.setting && choice.day === this.day
				? items.find((a) => formatAddress(a) === choice.address)
				: undefined;
		const address = chosen ?? pickForDay(items, this.setting, this.day);
		if (!address) return null;
		const picture = find(this.styles, address);
		return picture ? { address: formatAddress(address), picture } : null;
	});

	/** Every picture the setting covers, once the styles are loaded. */
	readonly pool = $derived.by((): Address[] => (this.styles ? pool(this.styles, parseSetting(this.setting)) : []));

	readonly shown = $derived.by((): Shown => {
		const setting = parseSetting(this.setting);
		if (setting.kind === 'default') return { kind: 'lake' };
		if (setting.kind === 'plain') return { kind: 'none' };
		const today = this.today;
		if (!today) {
			// Still loading: draw nothing rather than flash the lake. Loaded and
			// the style is gone, or the styles could not load and nothing is
			// cached for today: fall back to the lake.
			return this.styles === null && !this.stylesUnavailable ? { kind: 'none' } : { kind: 'lake' };
		}
		const v = today.picture[this.appearance];
		const style = today.address.split('/')[0];
		const url = fileUrl(style);
		return {
			kind: 'picture',
			key: `${today.address}:${this.appearance}`,
			avif: srcset(v, 'avif', url),
			webp: srcset(v, 'webp', url),
			src: widest(v, url),
			title: v.title,
		};
	});

	/** Load the styles and the stored setting once, and follow the appearance and the date. */
	start(): void {
		if (this.#started || typeof window === 'undefined') return;
		this.#started = true;

		window.addEventListener('themechange', () => (this.appearance = appearance()));
		// A laptop asleep at midnight, or a background tab, misses its timer:
		// when the window comes back, take up the date again, and try the
		// styles again if they could not load.
		document.addEventListener('visibilitychange', () => {
			if (document.visibilityState !== 'visible') return;
			this.#newDay();
			if (this.stylesUnavailable) void this.#loadStyles();
		});
		this.#scheduleMidnight();

		void this.#load();
	}

	async #load(): Promise<void> {
		try {
			const profile = await getAssistantProfile<{ ui_preferences?: { background?: unknown; background_today?: unknown } }>();
			const stored = profile.ui_preferences?.background;
			const choice = profile.ui_preferences?.background_today;
			if (!this.#changedHere) {
				this.dayChoice = isDayChoice(choice) ? choice : null;
				this.#remember(typeof stored === 'string' ? stored : '');
			}
		} catch (error) {
			// No server yet (setup) or unreachable: keep the cached setting.
			console.warn('Background setting not loaded:', error);
		}
		if (parseSetting(this.setting).kind === 'path') await this.#loadStyles();
	}

	/** Load the installed styles if they are not loaded yet, for the pickers. */
	async ensureStyles(): Promise<void> {
		if (this.styles === null || this.stylesUnavailable) await this.#loadStyles();
	}

	/** Load the styles again: after a download starts, and while it runs. */
	async refresh(): Promise<void> {
		await this.#loadStyles();
	}

	async #loadStyles(): Promise<void> {
		try {
			const res = await request<{
				styles: StyleManifest[];
				available?: AvailableStyle[];
				installing?: Record<string, DownloadProgress>;
				failed?: Record<string, string>;
				catalog_pending?: boolean;
			}>('/backgrounds');
			this.styles = res.styles;
			this.available = res.available ?? [];
			this.installing = res.installing ?? {};
			this.failed = res.failed ?? {};
			this.catalogPending = res.catalog_pending ?? false;
			this.stylesUnavailable = false;
			this.#cacheToday();
		} catch (error) {
			// Styles that loaded before stay as they are, so the picture does not
			// change. With none loaded yet, home shows today's cached picture, or
			// the lake; the next ensureStyles() or a return to the window tries
			// again.
			console.warn('Background styles not loaded:', error);
			if (this.styles === null) this.stylesUnavailable = true;
		}
	}

	/** Downloads in progress and failed, without the manifests, while a download runs. */
	async refreshDownloads(): Promise<void> {
		const res = await request<{ installing: Record<string, DownloadProgress>; failed: Record<string, string> }>(
			'/backgrounds/downloads',
		);
		this.installing = res.installing;
		this.failed = res.failed;
	}

	/** A download that did not start, said on its card. */
	noteFailure(style: string, message: string): void {
		this.failed = { ...this.failed, [style]: message };
	}

	#remember(setting: string): void {
		this.setting = setting;
		writeStorage(SETTING_KEY, setting || null);
	}

	#cacheToday(): void {
		const today = this.today;
		if (!today) {
			writeStorage(PICTURE_KEY, null);
			return;
		}
		const value: CachedPicture = { setting: this.setting, day: this.day, ...today };
		this.#cached = value;
		writeStorage(PICTURE_KEY, JSON.stringify(value));
	}

	/** Move to today's date if it changed, and set the next midnight timer. */
	#newDay(): void {
		const today = dayNumber(new Date());
		if (today !== this.day) {
			this.day = today;
			if (this.styles) this.#cacheToday();
		}
		this.#scheduleMidnight();
	}

	#scheduleMidnight(): void {
		if (this.#midnight) clearTimeout(this.#midnight);
		const now = new Date();
		const next = new Date(now.getFullYear(), now.getMonth(), now.getDate() + 1, 0, 0, 5);
		this.#midnight = setTimeout(() => this.#newDay(), next.getTime() - now.getTime());
	}

	/**
	 * Another picture for today under a rotating setting, until midnight.
	 * The home control's arrows choose it (lib/backgrounds/controls.ts).
	 */
	async chooseForToday(address: string): Promise<void> {
		this.#changedHere = true;
		const choice: DayChoice = { setting: this.setting, day: this.day, address };
		this.dayChoice = choice;
		this.#cacheToday();
		await updateUiPreferences({ background_today: choice });
	}

	/** Change the setting and save it to the server. */
	async set(setting: string): Promise<void> {
		const parsed = parseSetting(setting);
		const value = parsed.kind === 'default' ? '' : setting;
		this.#changedHere = true;
		this.#remember(value);
		if (parsed.kind === 'path' && this.styles === null) await this.#loadStyles();
		else this.#cacheToday();
		await updateUiPreferences({ background: value });
	}
}

export const background = new BackgroundStore();
