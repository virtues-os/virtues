<script lang="ts">
	import { Page, Input } from "$lib";
	import ThemePicker from "$lib/components/ThemePicker.svelte";
	import "iconify-icon";
	import { onMount } from "svelte";
	import { getTheme, setTheme, type Theme } from "$lib/utils/theme";
	import { invalidate } from "$app/navigation";

	let loading = $state(true);
	let currentTheme = $state<Theme>("light");

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

		// Immediately persist theme to profile and localStorage
		try {
			setTheme(newTheme);
			await fetch("/api/profile", {
				method: "PUT",
				headers: { "Content-Type": "application/json" },
				body: JSON.stringify({ theme: newTheme }),
			});
			// Refresh profile data in background so re-open uses latest
			invalidate("/api/profile");
		} catch (error) {
			console.error("Failed to save theme preference:", error);
		}
	}

	onMount(async () => {
		currentTheme = getTheme();
		await loadProfile();
	});

	async function loadProfile() {
		loading = true;
		try {
			const response = await fetch("/api/profile");
			if (response.ok) {
				const profile = await response.json();

				// Populate fields from profile
				fullName = profile.full_name || "";
				preferredName = profile.preferred_name || "";
				// Format birth_date for HTML date input (requires YYYY-MM-DD format)
				birthDate = profile.birth_date
					? profile.birth_date.split("T")[0]
					: "";
				heightCm = profile.height_cm || "";
				weightKg = profile.weight_kg || "";
				ethnicity = profile.ethnicity || "";
				occupation = profile.occupation || "";
				employer = profile.employer || "";

				// Load theme from profile (overrides localStorage if set)
				if (profile.theme) {
					currentTheme = profile.theme as Theme;
					setTheme(currentTheme);
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

			// Refresh profile data in background
			invalidate("/api/profile");
		} catch (error) {
			console.error(`Failed to save ${field}:`, error);
			throw error; // Re-throw so Input component can show error state
		}
	}
</script>

<Page>
	<div class="max-w-3xl">
		<!-- Header -->
		<div class="mb-8">
			<h1 class="text-3xl font-serif font-medium text-foreground mb-2">
				Profile
			</h1>
			<p class="text-foreground-muted">
				Manage your personal information and preferences
			</p>
		</div>

		{#if loading}
			<div class="text-center py-12 text-foreground-subtle">
				Loading profile...
			</div>
		{:else}
			<!-- Appearance Section - Outside form for immediate theme switching -->
			<div class="space-y-6 mb-6">
				<div class="bg-surface border border-border rounded-lg p-6">
					<h2 class="text-lg font-medium text-foreground mb-4">
						Appearance
					</h2>
					<ThemePicker
						value={currentTheme}
						onchange={handleThemeChange}
					/>
				</div>
			</div>

			<div class="space-y-6">
				<!-- Identity Section -->
				<div class="bg-surface border border-border rounded-lg p-6">
					<h2 class="text-lg font-medium text-foreground mb-4">
						Identity
					</h2>
					<div class="space-y-4">
						<div>
							<label
								for="fullName"
								class="block text-sm font-medium text-foreground-muted mb-2"
							>
								Full Name
							</label>
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

						<div>
							<label
								for="preferredName"
								class="block text-sm font-medium text-foreground-muted mb-2"
							>
								Preferred Name
							</label>
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

						<div>
							<label
								for="birthDate"
								class="block text-sm font-medium text-foreground-muted mb-2"
							>
								Birth Date
							</label>
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
				</div>

				<!-- Physical Details Section -->
				<div class="bg-surface border border-border rounded-lg p-6">
					<h2 class="text-lg font-medium text-foreground mb-4">
						Physical Details
					</h2>
					<div class="space-y-4">
						<div class="grid grid-cols-2 gap-4">
							<div>
								<label
									for="heightCm"
									class="block text-sm font-medium text-foreground-muted mb-2"
								>
									Height (cm)
								</label>
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

							<div>
								<label
									for="weightKg"
									class="block text-sm font-medium text-foreground-muted mb-2"
								>
									Weight (kg)
								</label>
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

						<div>
							<label
								for="ethnicity"
								class="block text-sm font-medium text-foreground-muted mb-2"
							>
								Ethnicity
							</label>
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
				</div>

				<!-- Work Information Section -->
				<div class="bg-surface border border-border rounded-lg p-6">
					<h2 class="text-lg font-medium text-foreground mb-4">
						Work Information
					</h2>
					<div class="space-y-4">
						<div>
							<label
								for="occupation"
								class="block text-sm font-medium text-foreground-muted mb-2"
							>
								Occupation
							</label>
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

						<div>
							<label
								for="employer"
								class="block text-sm font-medium text-foreground-muted mb-2"
							>
								Employer
							</label>
							<Input
								type="text"
								id="employer"
								bind:value={employer}
								placeholder="Company name (optional)"
								autoSave
								onSave={(val) =>
									saveField("employer", val || null)}
							/>
							<p class="text-xs text-foreground-subtle mt-1">
								Leave blank if self-employed or not applicable
							</p>
						</div>
					</div>
				</div>
			</div>
		{/if}
	</div>
</Page>
