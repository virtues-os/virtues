<script lang="ts">
	import { onMount } from "svelte";

	const MIN = 2;
	const MAX = 12;
	const DEFAULT = 8;

	let loading = $state(true);
	let saving = $state(false);
	let memberCount = $state(DEFAULT);
	// Preserve the rest of ui_preferences so saving council settings doesn't clobber them.
	let uiPreferences = $state<Record<string, unknown>>({});

	onMount(async () => {
		try {
			const res = await fetch("/api/assistant-profile");
			if (res.ok) {
				const profile = await res.json();
				uiPreferences = profile.ui_preferences ?? {};
				const council = (uiPreferences.council ?? {}) as {
					memberCount?: number;
				};
				if (typeof council.memberCount === "number") {
					memberCount = Math.min(
						MAX,
						Math.max(MIN, council.memberCount),
					);
				}
			}
		} catch (e) {
			console.error("Failed to load council settings:", e);
		} finally {
			loading = false;
		}
	});

	async function save(next: number) {
		const clamped = Math.min(MAX, Math.max(MIN, next));
		memberCount = clamped;
		saving = true;
		try {
			const council = {
				...((uiPreferences.council as object) ?? {}),
				memberCount: clamped,
			};
			const ui_preferences = { ...uiPreferences, council };
			uiPreferences = ui_preferences;
			await fetch("/api/assistant-profile", {
				method: "PUT",
				headers: { "Content-Type": "application/json" },
				body: JSON.stringify({ ui_preferences }),
			});
		} catch (e) {
			console.error("Failed to save council settings:", e);
		} finally {
			saving = false;
		}
	}
</script>

<div class="bg-surface border border-border rounded-lg">
	<div class="flex items-center justify-between px-4 py-3 border-b border-border">
		<h2 class="text-sm font-medium text-foreground">Council</h2>
	</div>

	{#if loading}
		<div class="text-center py-6 text-sm text-foreground-muted">
			Loading…
		</div>
	{:else}
		<div class="p-4">
			<div class="flex items-center justify-between">
				<div>
					<div class="text-sm font-medium text-foreground">
						Members
						<span class="font-normal text-foreground-subtle"
							>· minds consulted per question</span
						>
					</div>
					<div class="text-xs text-foreground-subtle mt-1">
						Each member is a separate model call ({memberCount} + 1 for
						synthesis = {memberCount + 1} calls per message).
					</div>
				</div>
				<div class="flex items-center gap-2 shrink-0 ml-4">
					<button
						type="button"
						class="w-7 h-7 flex items-center justify-center bg-background border border-border rounded-md text-foreground hover:border-border-strong disabled:opacity-40"
						disabled={saving || memberCount <= MIN}
						onclick={() => save(memberCount - 1)}
						aria-label="Fewer members">−</button
					>
					<span
						class="w-8 text-center text-sm font-medium text-foreground tabular-nums"
						>{memberCount}</span
					>
					<button
						type="button"
						class="w-7 h-7 flex items-center justify-center bg-background border border-border rounded-md text-foreground hover:border-border-strong disabled:opacity-40"
						disabled={saving || memberCount >= MAX}
						onclick={() => save(memberCount + 1)}
						aria-label="More members">+</button
					>
				</div>
			</div>
		</div>
	{/if}
</div>
