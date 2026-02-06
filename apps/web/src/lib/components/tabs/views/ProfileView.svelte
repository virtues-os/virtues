<script lang="ts">
	import type { Tab } from "$lib/tabs/types";
	import { Page, Input } from "$lib";
	import Icon from "$lib/components/Icon.svelte";
	import ThemePicker from "$lib/components/ThemePicker.svelte";
	import { onMount } from "svelte";
	import {
		getTheme,
		setTheme,
		type Theme,
		isValidTheme,
	} from "$lib/utils/theme";
	import { invalidate } from "$app/navigation";

	let { tab, active }: { tab: Tab; active: boolean } = $props();

	let loading = $state(true);
	let currentTheme = $state<Theme>(getTheme());

	// Profile fields
	let fullName = $state("");
	let preferredName = $state("");
	let birthDate = $state("");
	let heightCm = $state("");
	let weightKg = $state("");
	let ethnicity = $state("");
	let occupation = $state("");
	let employer = $state("");

	async function handleThemeChange(newTheme: Theme) {
		currentTheme = newTheme;

		try {
			setTheme(newTheme);
			await fetch("/api/profile", {
				method: "PUT",
				headers: { "Content-Type": "application/json" },
				body: JSON.stringify({ theme: newTheme }),
			});
			invalidate("/api/profile");
		} catch (error) {
			console.error("Failed to save theme preference:", error);
		}
	}

	onMount(async () => {
		await loadProfile();
	});

	async function loadProfile() {
		loading = true;
		try {
			const response = await fetch("/api/profile");
			if (response.ok) {
				const profile = await response.json();

				fullName = profile.full_name || "";
				preferredName = profile.preferred_name || "";
				birthDate = profile.birth_date
					? profile.birth_date.split("T")[0]
					: "";
				heightCm = profile.height_cm || "";
				weightKg = profile.weight_kg || "";
				ethnicity = profile.ethnicity || "";
				occupation = profile.occupation || "";
				employer = profile.employer || "";

				if (profile.theme && isValidTheme(profile.theme)) {
					currentTheme = profile.theme as Theme;
				}
			}
		} catch (error) {
			console.error("Failed to load profile:", error);
		} finally {
			loading = false;
		}
	}

	async function saveField(field: string, value: string | number | null) {
		try {
			const response = await fetch("/api/profile", {
				method: "PUT",
				headers: { "Content-Type": "application/json" },
				body: JSON.stringify({ [field]: value }),
			});

			if (!response.ok) {
				throw new Error(`Failed to save ${field}`);
			}

			invalidate("/api/profile");
		} catch (error) {
			console.error(`Failed to save ${field}:`, error);
			throw error;
		}
	}
</script>

<Page>
	<div class="max-w-3xl">
		<div class="mb-8">
			<h1 class="text-3xl font-serif font-medium text-foreground mb-2">
				Profile
			</h1>
			<p class="text-foreground-muted">
				Your personal information and preferences
			</p>
		</div>

		{#if loading}
			<div class="flex items-center justify-center h-64">
				<Icon icon="ri:loader-4-line" width="20" class="spin" />
			</div>
		{:else}
			<div class="space-y-8">
				<!-- Appearance -->
				<section>
					<h2 class="section-title">Appearance</h2>
					<ThemePicker
						value={currentTheme}
						onchange={handleThemeChange}
					/>
				</section>

				<!-- Identity -->
				<section>
					<h2 class="section-title">Identity</h2>
					<div class="fields">
						<div class="field">
							<label for="fullName">Full Name</label>
							<Input
								type="text"
								id="fullName"
								bind:value={fullName}
								placeholder="Your full legal name"
								autoSave
								onSave={(val) =>
									saveField("full_name", val || null)}
							/>
						</div>

						<div class="field">
							<label for="preferredName">Preferred Name</label>
							<Input
								type="text"
								id="preferredName"
								bind:value={preferredName}
								placeholder="How should the assistant address you?"
								autoSave
								onSave={(val) =>
									saveField("preferred_name", val || null)}
							/>
						</div>

						<div class="field">
							<label for="birthDate">Birth Date</label>
							<Input
								type="date"
								id="birthDate"
								bind:value={birthDate}
								autoSave
								onSave={(val) =>
									saveField("birth_date", val || null)}
							/>
						</div>
					</div>
				</section>

				<!-- Physical Details -->
				<section>
					<h2 class="section-title">Physical Details</h2>
					<div class="fields">
						<div class="field-row">
							<div class="field">
								<label for="heightCm">Height (cm)</label>
								<Input
									type="number"
									step="0.01"
									id="heightCm"
									bind:value={heightCm}
									placeholder="175.5"
									autoSave
									onSave={(val) => {
										const num = parseFloat(
											String(val ?? ""),
										);
										saveField(
											"height_cm",
											isNaN(num) ? null : num,
										);
									}}
								/>
							</div>

							<div class="field">
								<label for="weightKg">Weight (kg)</label>
								<Input
									type="number"
									step="0.01"
									id="weightKg"
									bind:value={weightKg}
									placeholder="70.5"
									autoSave
									onSave={(val) => {
										const num = parseFloat(
											String(val ?? ""),
										);
										saveField(
											"weight_kg",
											isNaN(num) ? null : num,
										);
									}}
								/>
							</div>
						</div>

						<div class="field">
							<label for="ethnicity">Ethnicity</label>
							<Input
								type="text"
								id="ethnicity"
								bind:value={ethnicity}
								placeholder="Optional"
								autoSave
								onSave={(val) =>
									saveField("ethnicity", val || null)}
							/>
						</div>
					</div>
				</section>

				<!-- Work -->
				<section>
					<h2 class="section-title">Work</h2>
					<div class="fields">
						<div class="field">
							<label for="occupation">Occupation</label>
							<Input
								type="text"
								id="occupation"
								bind:value={occupation}
								placeholder="Software Engineer, Designer, Student, etc."
								autoSave
								onSave={(val) =>
									saveField("occupation", val || null)}
							/>
						</div>

						<div class="field">
							<label for="employer">Employer</label>
							<Input
								type="text"
								id="employer"
								bind:value={employer}
								placeholder="Company name (optional)"
								autoSave
								onSave={(val) =>
									saveField("employer", val || null)}
							/>
							<span class="field-hint"
								>Leave blank if self-employed or not applicable</span
							>
						</div>
					</div>
				</section>
			</div>
		{/if}
	</div>
</Page>

<style>
	.section-title {
		font-size: 14px;
		font-weight: 500;
		color: var(--foreground-muted);
		margin-bottom: 12px;
	}

	.fields {
		display: flex;
		flex-direction: column;
		gap: 16px;
	}

	.field {
		display: flex;
		flex-direction: column;
		gap: 6px;
	}

	.field label {
		font-size: 13px;
		font-weight: 500;
		color: var(--foreground);
	}

	.field-row {
		display: grid;
		grid-template-columns: 1fr 1fr;
		gap: 16px;
	}

	.field-hint {
		font-size: 12px;
		color: var(--foreground-subtle);
	}
</style>
