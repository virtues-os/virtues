<script lang="ts">
	/**
	 * The glass button in the bottom right corner of home, and the popover it
	 * opens: today's picture with arrows to the one before and after,
	 * Light, Dark, Match system and Follow the day, and the collections of
	 * the current style to swipe through. Settings › Appearance has the
	 * full picker (agents/plan/backgrounds-plan.md, "The home control").
	 */
	import Icon from '$lib/components/Icon.svelte';
	import { Popover } from '$lib/floating';
	import { background } from '$lib/stores/background.svelte';
	import { parseSetting } from '$lib/backgrounds/resolve';
	import { current, isRotating, keep, step, thumbnail } from '$lib/backgrounds/controls';
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

	let open = $state(false);
	let theme = $state<ThemeChoice>(getThemeChoice());
	const themes = getAvailableThemes();

	$effect(() => {
		if (!open) return;
		theme = getThemeChoice();
		void background.ensureStyles();
	});

	const setting = $derived(parseSetting(background.setting));
	const now = $derived(current());

	// The style the swiper shows: the setting's, or the first installed one
	// while no style is chosen.
	const style = $derived.by(() => {
		const styles = background.styles ?? [];
		const name = setting.kind === 'path' ? setting.style : styles[0]?.style;
		return styles.find((s) => s.style === name) ?? null;
	});

	const caption = $derived.by(() => {
		if (setting.kind === 'plain') return { title: 'Plain', note: 'No picture' };
		if (setting.kind === 'default' || !now) {
			return { title: 'No background chosen', note: style ? 'Pick a collection below' : 'See all backgrounds' };
		}
		return { title: now.title, note: now.collectionTitle };
	});

	const canStep = $derived(setting.kind === 'path' && !!now);

	function chooseTheme(next: ThemeChoice) {
		theme = next;
		void setTheme(next);
	}
</script>

<div class="corner" class:phone={mobileLayout.isMobile}>
	<Popover bind:open placement="top-end" offset={10}>
		{#snippet trigger({ toggle })}
			<button
				type="button"
				class="lq corner-button"
				aria-label="Background and appearance"
				aria-expanded={open}
				aria-haspopup="dialog"
				onclick={toggle}
			>
				<Icon icon="mynaui:image" width="18" />
			</button>
		{/snippet}
		{#snippet children()}
			<div class="panel" role="dialog" aria-label="Background and appearance">
				<div class="now">
					<button type="button" class="arrow" aria-label="Previous picture" disabled={!canStep} onclick={() => step(-1)}>
						<Icon icon="mynaui:chevron-left" width="16" />
					</button>
					<div class="caption">
						<span class="title">{caption.title}</span>
						<span class="note">{caption.note}</span>
					</div>
					<button type="button" class="arrow" aria-label="Next picture" disabled={!canStep} onclick={() => step(1)}>
						<Icon icon="mynaui:chevron-right" width="16" />
					</button>
				</div>
				{#if isRotating() && now}
					<button type="button" class="keep" onclick={() => keep()}>Keep this picture</button>
				{/if}

				<div class="themes" role="radiogroup" aria-label="Appearance">
					{#each themes as choice (choice)}
						<button
							type="button"
							role="radio"
							class="theme"
							class:picked={theme === choice}
							aria-checked={theme === choice}
							onclick={() => chooseTheme(choice)}
						>
							<Icon icon={themeMetadata[choice].icon} width="16" />
							<span>{getThemeDisplayName(choice)}</span>
						</button>
					{/each}
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
								<img src={thumbnail(style.style, c.pictures[0])} alt="" loading="lazy" decoding="async" />
								<span>{c.title}</span>
							</button>
						{/each}
					</div>
				{/if}

				<button
					type="button"
					class="more"
					onclick={() => {
						open = false;
						openSetting('background', DEFAULT_ASSISTANT_NAME);
					}}
				>
					All backgrounds
				</button>
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

	/* On the phone the composer is docked at the bottom, so the button
	   stands above an empty composer. */
	.corner.phone {
		bottom: 120px;
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
		gap: 12px;
		width: 300px;
		padding: 12px;
	}

	.now {
		display: flex;
		align-items: center;
		gap: 8px;
	}

	.caption {
		display: flex;
		flex: 1;
		flex-direction: column;
		min-width: 0;
		text-align: center;
	}

	.title {
		overflow: hidden;
		font-size: var(--text-body);
		color: var(--color-foreground);
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.note {
		font-size: var(--text-label);
		color: var(--color-foreground-muted);
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
	}

	.arrow:hover:not(:disabled) {
		background: var(--hover-bg);
	}

	.arrow:disabled {
		color: var(--color-foreground-subtle);
		cursor: default;
	}

	.keep,
	.more {
		align-self: center;
		padding: 0;
		font-size: var(--text-label);
		color: var(--color-primary);
		background: none;
		border: none;
		cursor: pointer;
	}

	.more {
		color: var(--color-foreground-muted);
	}

	.more:hover {
		color: var(--color-foreground);
	}

	.themes {
		display: grid;
		grid-template-columns: repeat(4, minmax(0, 1fr));
		gap: 4px;
	}

	.theme {
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: 4px;
		padding: 8px 4px;
		font-size: var(--text-caption);
		line-height: 1.2;
		color: var(--color-foreground-muted);
		text-align: center;
		background: none;
		border: none;
		border-radius: var(--radius-md);
		cursor: pointer;
	}

	.theme:hover {
		color: var(--color-foreground);
		background: var(--hover-bg);
	}

	.theme.picked {
		color: var(--color-foreground);
		background: var(--active-bg);
	}

	.collections {
		display: flex;
		gap: 8px;
		margin: 0 -12px;
		padding: 0 12px 4px;
		overflow-x: auto;
		scroll-snap-type: x mandatory;
		scroll-padding: 0 12px;
		scrollbar-width: none;
	}

	.collections::-webkit-scrollbar {
		display: none;
	}

	.collection {
		display: flex;
		flex: 0 0 88px;
		flex-direction: column;
		gap: 4px;
		padding: 0;
		font-size: var(--text-caption);
		color: var(--color-foreground-muted);
		text-align: left;
		background: none;
		border: none;
		cursor: pointer;
		scroll-snap-align: start;
	}

	.collection img {
		display: block;
		width: 88px;
		height: 56px;
		object-fit: cover;
		border-radius: var(--radius-md);
		outline: 1px solid var(--color-border);
		outline-offset: -1px;
	}

	.collection span {
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.collection:hover {
		color: var(--color-foreground);
	}

	.collection.picked {
		color: var(--color-foreground);
	}

	.collection.picked img {
		outline: 2px solid var(--color-primary);
		outline-offset: -2px;
	}
</style>
