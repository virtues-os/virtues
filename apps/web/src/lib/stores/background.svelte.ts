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
 * resumes the next day. Under one picture they change the setting to its
 * neighbor in the collection.
 */

import { backendUrl } from '$lib/config/backend';
import { getAssistantProfile, request, updateUiPreferences } from '$lib/api/client';
import { getTheme, isThemeDark } from '$lib/utils/theme';
import {
	dayNumber,
	find,
	formatAddress,
	narrowest,
	parseSetting,
	pickForDay,
	pool,
	srcset,
	stepFrom,
	widest,
	type Address,
	type Appearance,
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

function fileUrl(style: string) {
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
	#cached = typeof window === 'undefined' ? null : readCachedPicture();
	#started = false;

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

	/** Whether the setting changes picture each day. */
	readonly rotating = $derived.by(() => {
		const s = parseSetting(this.setting);
		return s.kind === 'path' && !s.picture;
	});

	/** Today's picture with the names of its style and collection, for the home control. */
	readonly current = $derived.by(() => {
		const today = this.today;
		if (!today || !this.styles) return null;
		const [style, collection] = today.address.split('/');
		const c = this.styles.find((s) => s.style === style)?.collections.find((x) => x.id === collection);
		return { address: today.address, style, collection, collectionTitle: c?.title ?? collection, title: today.picture[this.appearance].title };
	});

	readonly shown = $derived.by((): Shown => {
		const setting = parseSetting(this.setting);
		if (setting.kind === 'default') return { kind: 'lake' };
		if (setting.kind === 'plain') return { kind: 'none' };
		const today = this.today;
		if (!today) {
			// Still loading: draw nothing rather than flash the lake. Loaded and
			// the style is gone: fall back to the lake.
			return this.styles === null ? { kind: 'none' } : { kind: 'lake' };
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
		this.#scheduleMidnight();

		void this.#load();
	}

	async #load(): Promise<void> {
		try {
			const profile = await getAssistantProfile<{ ui_preferences?: { background?: unknown; background_today?: unknown } }>();
			const stored = profile.ui_preferences?.background;
			const choice = profile.ui_preferences?.background_today;
			this.dayChoice = isDayChoice(choice) ? choice : null;
			this.#remember(typeof stored === 'string' ? stored : '');
		} catch (error) {
			// No server yet (setup) or unreachable: keep the cached setting.
			console.warn('Background setting not loaded:', error);
		}
		if (parseSetting(this.setting).kind === 'path') await this.#loadStyles();
	}

	/** Load the installed styles if they are not loaded yet, for the pickers. */
	async ensureStyles(): Promise<void> {
		if (this.styles === null) await this.#loadStyles();
	}

	async #loadStyles(): Promise<void> {
		try {
			const res = await request<{ styles: StyleManifest[] }>('/backgrounds');
			this.styles = res.styles;
			this.#cacheToday();
		} catch (error) {
			// Without the manifests a path setting shows the cached picture or the lake.
			console.warn('Background styles not loaded:', error);
			this.styles = [];
		}
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

	#scheduleMidnight(): void {
		const now = new Date();
		const next = new Date(now.getFullYear(), now.getMonth(), now.getDate() + 1, 0, 0, 5);
		setTimeout(() => {
			this.day = dayNumber(new Date());
			if (this.styles) this.#cacheToday();
			this.#scheduleMidnight();
		}, next.getTime() - now.getTime());
	}

	/**
	 * The previous or next picture. Under a rotating setting it is today's
	 * picture until midnight; under one picture it becomes the setting.
	 */
	async step(dir: 1 | -1): Promise<void> {
		await this.ensureStyles();
		const current = this.today?.address ?? '';
		if (this.rotating) {
			const next = stepFrom(this.pool, current, dir);
			if (!next) return;
			const choice: DayChoice = { setting: this.setting, day: this.day, address: formatAddress(next) };
			this.dayChoice = choice;
			this.#cacheToday();
			await updateUiPreferences({ background_today: choice });
			return;
		}
		const setting = parseSetting(this.setting);
		if (setting.kind !== 'path' || !this.styles) return;
		const siblings = pool(this.styles, { kind: 'path', style: setting.style, collection: setting.collection });
		const next = stepFrom(siblings, current, dir);
		if (next) await this.set(formatAddress(next));
	}

	/** Keep today's picture: the setting becomes that one picture. */
	async keep(): Promise<void> {
		const address = this.today?.address;
		if (address) await this.set(address);
	}

	/** A small picture for a thumbnail, in the current appearance. */
	thumbnail(style: string, picture: Picture): string {
		return narrowest(picture[this.appearance], fileUrl(style));
	}

	/** Change the setting and save it to the server. */
	async set(setting: string): Promise<void> {
		const parsed = parseSetting(setting);
		const value = parsed.kind === 'default' ? '' : setting;
		this.#remember(value);
		if (parsed.kind === 'path' && this.styles === null) await this.#loadStyles();
		else this.#cacheToday();
		await updateUiPreferences({ background: value });
	}
}

export const background = new BackgroundStore();
