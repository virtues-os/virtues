<script lang="ts">
	import {
		getAvailableThemes,
		getThemeDisplayName,
		setTheme,
		themeDefaultLabel,
		themePreviewColors,
		type Theme
	} from '$lib/utils/theme';
	interface Props {
		value: Theme;
		onchange?: (theme: Theme) => void;
		/** Compact mode shows smaller cards in a 2x2 grid */
		compact?: boolean;
	}

	let { value, onchange, compact = false }: Props = $props();

	const themes = getAvailableThemes();

	async function selectTheme(theme: Theme) {
		await setTheme(theme);
		onchange?.(theme);
	}
</script>

<div class="grid {compact ? 'grid-cols-2 gap-3' : 'grid-cols-4 gap-5'}">
	{#each themes as theme}
		{@const colors = themePreviewColors[theme]}
		{@const isSelected = value === theme}

		<button
			type="button"
			onclick={() => selectTheme(theme)}
			class="group flex flex-col items-center gap-3"
		>
			<!-- Preview card -->
			<div
				class="w-full rounded-xl overflow-hidden border-2 transition-all duration-200
					{isSelected
					? 'border-primary ring-2 ring-primary/30'
					: 'border-border hover:border-primary/50'}"
			>
				<!-- Mini preview window - video aspect ratio -->
				<div
					class="aspect-video p-2"
					style="background-color: {colors.background}"
				>
					<!-- Window chrome -->
					<div
						class="rounded-lg overflow-hidden h-full flex flex-col"
						style="background-color: {colors.surface}; border: 1px solid {colors.surfaceElevated}"
					>
						<!-- Title bar -->
						<div
							class="flex items-center gap-1 px-2 py-1.5"
							style="background-color: {colors.surfaceElevated}"
						>
							<div class="w-1.5 h-1.5 rounded-full bg-red-400"></div>
							<div class="w-1.5 h-1.5 rounded-full bg-yellow-400"></div>
							<div class="w-1.5 h-1.5 rounded-full bg-green-400"></div>
						</div>

						<!-- Content area -->
						<div class="flex flex-1 min-h-0">
							<!-- Sidebar -->
							<div
								class="w-5 flex flex-col gap-1 p-1.5"
								style="background-color: {colors.surfaceElevated}"
							>
								<div
									class="w-full aspect-square rounded-sm"
									style="background-color: {colors.primary}"
								></div>
								<div
									class="w-full aspect-square rounded-sm opacity-40"
									style="background-color: {colors.foregroundMuted}"
								></div>
							</div>

							<!-- Main content -->
							<div class="flex-1 p-2 space-y-1.5">
								<!-- Text lines -->
								<div
									class="h-1 w-3/4 rounded-full"
									style="background-color: {colors.foreground}"
								></div>
								<div
									class="h-1 w-full rounded-full opacity-60"
									style="background-color: {colors.foregroundMuted}"
								></div>
								<div
									class="h-1 w-2/3 rounded-full"
									style="background-color: {colors.primary}"
								></div>
							</div>
						</div>
					</div>
				</div>
			</div>

			<!--
				Name, then its role if it has one. Two lines for every card, not
				just the two that carry a role — a caption that appears on some
				cards and not others shunts the whole row out of alignment, which
				is a worse cost than a little empty space.
			-->
			<span class="theme-caption">
				<span class="text-sm font-medium {isSelected ? 'text-primary' : 'text-foreground'}">
					{getThemeDisplayName(theme)}
				</span>
				<span class="theme-role">{themeDefaultLabel(theme) ?? ''}</span>
			</span>
		</button>
	{/each}
</div>

<style>
	.theme-caption {
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: 0.125rem;
	}
	.theme-role {
		font-size: 0.6875rem;
		line-height: 1rem;
		/* Reserved even when empty, so all captions occupy the same height. */
		min-height: 1rem;
		color: var(--color-foreground-subtle, #9ca3af);
	}
</style>
