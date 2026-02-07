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
	let updateCheckHour = $state<number>(8);

	// Convert UTC hour (0-23) to a local time label like "3:00 AM"
	function utcHourToLocalLabel(utcHour: number): string {
		const date = new Date();
		date.setUTCHours(utcHour, 0, 0, 0);
		return date.toLocaleTimeString([], {
			hour: "numeric",
			minute: "2-digit",
		});
	}

	// Convert a local hour (0-23) to UTC hour
	function localHourToUtc(localHour: number): number {
		const date = new Date();
		date.setHours(localHour, 0, 0, 0);
		return date.getUTCHours();
	}

	// Convert UTC hour to local hour for the select value
	function utcHourToLocalHour(utcHour: number): number {
		const date = new Date();
		date.setUTCHours(utcHour, 0, 0, 0);
		return date.getHours();
	}

	// Generate hour options (0-23) displayed in local time, stored as UTC
	const hourOptions = Array.from({ length: 24 }, (_, i) => ({
		localHour: i,
		utcHour: localHourToUtc(i),
		label: utcHourToLocalLabel(localHourToUtc(i)),
	}));

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
				updateCheckHour = profile.update_check_hour ?? 8;

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

				<!-- Updates -->
				<section>
					<h2 class="section-title">Updates</h2>
					<div class="fields">
						<div class="field">
							<label for="updateCheckHour"
								>Automatic update time</label
							>
							<select
								id="updateCheckHour"
								class="update-select"
								value={utcHourToLocalHour(updateCheckHour)}
								onchange={(e) => {
									const localHour = parseInt(
										e.currentTarget.value,
									);
									const utcHour = localHourToUtc(localHour);
									updateCheckHour = utcHour;
									saveField("update_check_hour", utcHour);
								}}
							>
								{#each hourOptions as opt}
									<option value={opt.localHour}
										>{opt.label}</option
									>
								{/each}
							</select>
							<span class="field-hint">
								Virtues automatically updates during this hour.
								Updates take about 30 seconds.
							</span>
						</div>
					</div>
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

	.update-select {
		appearance: none;
		background: var(--surface);
		border: 1px solid var(--border);
		border-radius: 6px;
		padding: 8px 12px;
		font-size: 13px;
		color: var(--foreground);
		cursor: pointer;
		max-width: 200px;
		background-image: url("data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' width='12' height='12' viewBox='0 0 12 12'%3E%3Cpath fill='%23888' d='M3 4.5L6 7.5L9 4.5'/%3E%3C/svg%3E");
		background-repeat: no-repeat;
		background-position: right 10px center;
		padding-right: 28px;
	}

	.update-select:focus {
		outline: none;
		border-color: var(--primary);
	}
</style>
