<script lang="ts">
	/**
	 * The glass button in the bottom right corner of home (on the phone, the
	 * chat's ⋯ menu instead), and the popover it opens: today's picture with arrows to the one before and after, the
	 * collections of the current style to swipe through, and Light, Dark,
	 * Match system and Follow the day. Settings › Appearance has the full
	 * picker (agents/plan/backgrounds-plan.md, "The home control").
	 */
	import Icon from '$lib/components/Icon.svelte';
	import { Popover } from '$lib/floating';
	import { background } from '$lib/stores/background.svelte';
	import { parseSetting } from '$lib/backgrounds/resolve';
	import { current, isRotating, keep, neighbors, step, thumbnail } from '$lib/backgrounds/controls';
	import { fade } from '$lib/utils/transitions';
	import {
		getAvailableThemes,
		getThemeChoice,
		getThemeDisplayName,
		setTheme,
		themeMetadata,
		type ThemeChoice,
	} from '$lib/utils/theme';
	import { openSetting } from '$lib/settings/focus';
	import { DEFAULT_ASSISTANT_NAME } from '$lib/settings';
	import { mobileLayout } from '$lib/stores/mobileLayout.svelte';


	// On the phone the popover opens from the chat's ⋯ menu, so its state
	// lives in mobileLayout; the corner button is a desktop thing.
	let localOpen = $state(false);
	const isOpen = $derived(mobileLayout.isMobile ? mobileLayout.backgroundOpen : localOpen);
	function setOpen(next: boolean) {
		if (mobileLayout.isMobile) mobileLayout.backgroundOpen = next;
		else localOpen = next;
	}

	let theme = $state<ThemeChoice>(getThemeChoice());
	const themes = getAvailableThemes();

	$effect(() => {
		if (!isOpen) return;
		theme = getThemeChoice();
		void background.ensureStyles();
		if (setting.kind === 'site') void background.ensureSites();
	});

	const setting = $derived(parseSetting(background.setting));
	const now = $derived(current());

	// The style the swiper shows: the setting's, or the first installed one
	// while no style is chosen.
	const style = $derived.by(() => {
		const styles = background.allStyles;
		const name = setting.kind === 'path' ? setting.style : styles[0]?.style;
		return styles.find((s) => s.style === name) ?? null;
	});

	const caption = $derived.by(() => {
		if (setting.kind === 'plain') return { title: 'Plain', note: 'No picture' };
		if (setting.kind === 'site') return { title: background.siteName(setting.id) ?? 'Your site', note: 'One of your sites' };
		if (setting.kind === 'default' || !now) return { title: 'The lake', note: 'Until you choose a background' };
		return { title: now.title, note: now.collectionTitle };
	});

	const canStep = $derived(setting.kind === 'path' && !!now);

	// The pictures either side of today's, loaded while the popover is open so
	// an arrow shows its picture without waiting for the download.
	const preload = $derived(isOpen && canStep ? neighbors() : []);

	// Short names so each segment fits on one line. Settings › Appearance
	// and the segment's tooltip keep the full ones.
	const SHORT_NAMES: Partial<Record<ThemeChoice, string>> = { system: 'System', day: 'Day cycle' };

	function chooseTheme(next: ThemeChoice) {
		theme = next;
		void setTheme(next);
	}
</script>

<div class="corner" class:phone={mobileLayout.isMobile}>
	<Popover bind:open={() => isOpen, setOpen} placement={mobileLayout.isMobile ? "bottom-end" : "top-end"} offset={10}>
		{#snippet trigger({ toggle })}
			<button
				type="button"
				class="lq corner-button"
				aria-label="Background and appearance"
				aria-expanded={isOpen}
				aria-haspopup="dialog"
				onclick={toggle}
			>
				<Icon icon="mynaui:image" width="18" />
			</button>
		{/snippet}
		{#snippet children()}
			<div class="panel" role="dialog" aria-label="Background and appearance">
				<div class="now">
					<div class="caption">
						{#key caption.title}
							<span class="title" in:fade={{ duration: 220 }}>{caption.title}</span>
						{/key}
						<span class="note">
							<span class="collection-name">{caption.note}</span>
							{#if isRotating() && now}
								<span class="sep" aria-hidden="true">·</span>
								<button type="button" class="keep" onclick={() => keep()}>Keep this picture</button>
							{/if}
						</span>
					</div>
					<div class="arrows">
						<button type="button" class="arrow" aria-label="Previous picture" disabled={!canStep} onclick={() => step(-1)}>
							<Icon icon="mynaui:chevron-left" width="16" />
						</button>
						<button type="button" class="arrow" aria-label="Next picture" disabled={!canStep} onclick={() => step(1)}>
							<Icon icon="mynaui:chevron-right" width="16" />
						</button>
					</div>
				</div>

				{#if style}
					<div class="collections" role="radiogroup" aria-label="Collection">
						{#each style.collections as c (c.id)}
							{@const picked = setting.kind === 'path' && setting.style === style.style && setting.collection === c.id}
							<button
								type="button"
								role="radio"
								class="collection"
								class:picked
								aria-checked={picked}
								onclick={() => background.set(`${style.style}/${c.id}`)}
							>
								<span class="thumb">
									<img src={thumbnail(style.style, c.pictures[0])} alt="" loading="lazy" decoding="async" />
								</span>
								<span class="name">{c.title}</span>
							</button>
						{/each}
					</div>
				{/if}

				<div class="themes" role="radiogroup" aria-label="Appearance">
					{#each themes as choice (choice)}
						<button
							type="button"
							role="radio"
							class="theme"
							class:picked={theme === choice}
							aria-checked={theme === choice}
							aria-label={getThemeDisplayName(choice)}
							title={SHORT_NAMES[choice] ? getThemeDisplayName(choice) : undefined}
							onclick={() => chooseTheme(choice)}
						>
							<Icon icon={themeMetadata[choice].icon} width="16" />
							<span>{SHORT_NAMES[choice] ?? getThemeDisplayName(choice)}</span>
						</button>
					{/each}
				</div>

				<button
					type="button"
					class="more"
					onclick={() => {
						setOpen(false);
						openSetting('background', DEFAULT_ASSISTANT_NAME);
					}}
				>
					<span>All backgrounds</span>
					<Icon icon="mynaui:chevron-right" width="14" />
				</button>

				{#each preload as p (p.key)}
					<picture class="preload" aria-hidden="true">
						<source srcset={p.avif} sizes="100vw" type="image/avif" />
						<img src={p.src} alt="" decoding="async" />
					</picture>
				{/each}
			</div>
		{/snippet}
	</Popover>
</div>

<style>
	.corner {
		position: absolute;
		right: 16px;
		bottom: 16px;
		z-index: 3;
	}

	/* On the phone the ⋯ in the top bar opens it, so the button is hidden
	   and only anchors the popover under the bar's right end. */
	.corner.phone {
		top: 52px;
		right: 16px;
		bottom: auto;
	}
	.corner.phone .corner-button {
		visibility: hidden;
	}

	.corner-button {
		display: grid;
		place-items: center;
		width: 34px;
		height: 34px;
		border-radius: var(--radius-full);
		color: var(--color-foreground);
		cursor: pointer;
	}

	.panel {
		display: flex;
		flex-direction: column;
		width: 312px;
		padding: 6px 0 4px;
	}

	/* Today's picture: its name and collection on the left, the arrows to
	   the one before and after on the right. */
	.now {
		display: flex;
		align-items: center;
		gap: 12px;
		padding: 8px 10px 12px 16px;
	}

	.caption {
		display: grid;
		flex: 1;
		min-width: 0;
		gap: 2px;
	}

	.title {
		overflow: hidden;
		font-size: var(--text-body);
		font-weight: 500;
		color: var(--color-foreground);
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.note {
		display: flex;
		align-items: baseline;
		gap: 6px;
		min-width: 0;
		font-size: var(--text-small);
		color: var(--color-foreground-muted);
		white-space: nowrap;
	}

	.collection-name {
		overflow: hidden;
		text-overflow: ellipsis;
	}

	.sep {
		color: var(--color-foreground-subtle);
	}

	.keep {
		flex-shrink: 0;
		padding: 0;
		font: inherit;
		color: var(--color-primary);
		background: none;
		border: none;
		cursor: pointer;
		transition: opacity var(--motion-hover-out) var(--ease-premium);
	}

	.keep:hover {
		opacity: 0.75;
	}

	.arrows {
		display: flex;
		gap: 2px;
	}

	.arrow {
		display: grid;
		place-items: center;
		width: 30px;
		height: 30px;
		border-radius: var(--radius-full);
		color: var(--color-foreground);
		background: none;
		border: none;
		cursor: pointer;
		transition: background-color var(--motion-hover-out) var(--ease-premium);
	}

	.arrow:hover:not(:disabled) {
		background: var(--hover-bg);
		transition-duration: var(--motion-hover-in);
	}

	.arrow:active:not(:disabled) {
		background: var(--active-bg);
	}

	.arrow:disabled {
		color: var(--color-foreground-subtle);
		cursor: default;
	}

	/* The collections of the current style, to swipe through. The picked one
	   carries a ring that stands off the thumbnail. */
	.collections {
		display: flex;
		gap: 10px;
		padding: 2px 16px 14px;
		overflow-x: auto;
		scroll-snap-type: x mandatory;
		scroll-padding: 0 16px;
		scrollbar-width: none;
	}

	.collections::-webkit-scrollbar {
		display: none;
	}

	.collection {
		display: flex;
		flex: 0 0 84px;
		flex-direction: column;
		gap: 6px;
		padding: 0;
		font-size: var(--text-caption);
		color: var(--color-foreground-muted);
		text-align: center;
		background: none;
		border: none;
		cursor: pointer;
		scroll-snap-align: start;
	}

	.thumb {
		display: block;
		padding: 2px;
		border-radius: calc(var(--radius-md) + 3px);
		/* The ring is an outline, as on the theme cards: a hairline, never a shadow. */
		outline: 1.5px solid transparent;
		outline-offset: -1.5px;
		transition: outline-color var(--motion-hover-out) var(--ease-premium);
	}

	.thumb img {
		display: block;
		width: 100%;
		height: 52px;
		object-fit: cover;
		border-radius: var(--radius-md);
		outline: 1px solid var(--color-border-subtle);
		outline-offset: -1px;
	}

	.name {
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.collection:hover,
	.collection.picked {
		color: var(--color-foreground);
	}

	.collection:hover:not(.picked) .thumb {
		outline-color: var(--color-border);
	}

	.collection.picked .thumb {
		outline-color: var(--color-primary);
	}

	/* Light, Dark, Match system and Follow the day as one segmented track,
	   drawn like Segmented. */
	.themes {
		display: grid;
		grid-template-columns: repeat(4, minmax(0, 1fr));
		gap: 2px;
		margin: 0 12px;
		padding: 2px; /* design-ok: the segments sit 2px inside the track, as in Segmented */
		border-radius: calc(var(--radius-md) + 2px);
		background: var(--fill-bg);
	}

	.theme {
		display: flex;
		flex-direction: column;
		align-items: center;
		justify-content: center;
		gap: 3px;
		min-height: 48px;
		padding: 6px 2px;
		font-size: var(--text-caption);
		line-height: 1.15;
		white-space: nowrap;
		color: var(--color-foreground-muted);
		text-align: center;
		background: none;
		border: 1px solid transparent;
		border-radius: var(--radius-md);
		cursor: pointer;
		transition:
			color var(--motion-hover-out) var(--ease-premium),
			background-color var(--motion-hover-out) var(--ease-premium);
	}

	.theme:hover:not(.picked) {
		color: var(--color-foreground);
	}

	.theme.picked {
		color: var(--color-foreground);
		background: var(--color-surface);
		border-color: var(--color-border);
		cursor: default;
	}

	.more {
		display: flex;
		align-items: center;
		justify-content: space-between;
		height: 34px;
		margin: 10px 4px 0;
		padding: 0 12px;
		font-size: var(--text-small);
		color: var(--color-foreground-muted);
		background: none;
		border: none;
		border-radius: var(--radius-md);
		cursor: pointer;
		transition:
			color var(--motion-hover-out) var(--ease-premium),
			background-color var(--motion-hover-out) var(--ease-premium);
	}

	.more:hover {
		color: var(--color-foreground);
		background: var(--hover-bg);
	}

	/* Loaded, never shown. */
	.preload {
		display: none;
	}

	@media (prefers-reduced-motion: reduce) {
		.arrow,
		.keep,
		.thumb,
		.theme,
		.more {
			transition: none;
		}
	}
</style>
