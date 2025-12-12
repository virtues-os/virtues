<script lang="ts">
	import { page } from "$app/stores";
	import { onMount } from "svelte";
	import { Input, Button } from "$lib";

	let deviceId = $derived($page.url.searchParams.get("device_id") || "");
	let deviceName = $state("");
	let pairing = $state(false);
	let paired = $state(false);
	let error = $state("");

	// Get computer name on mount
	onMount(async () => {
		if (!deviceId) {
			error = "Missing device_id parameter";
			return;
		}

		// Check if device already exists
		const checkRes = await fetch(
			`/api/pairing/check?device_id=${deviceId}`,
		);
		if (checkRes.ok) {
			const data = await checkRes.json();
			if (data.exists) {
				deviceName = data.deviceName || "Your Mac";
			}
		}
	});

	async function confirmPairing() {
		pairing = true;
		error = "";

		try {
			const res = await fetch("/api/pairing/confirm", {
				method: "POST",
				headers: { "Content-Type": "application/json" },
				body: JSON.stringify({
					device_id: deviceId,
					device_name: deviceName || "My Mac",
					device_type: "mac",
				}),
			});

			if (!res.ok) {
				const data = await res.json();
				throw new Error(data.error || "Pairing failed");
			}

			paired = true;
		} catch (err) {
			error = err instanceof Error ? err.message : "Pairing failed";
		} finally {
			pairing = false;
		}
	}
</script>

<div
	class="min-h-screen flex items-center justify-center bg-surface-elevated p-4"
>
	<div class="max-w-md w-full bg-surface rounded-lg shadow-lg p-8">
		{#if !deviceId}
			<div class="text-center">
				<div class="text-error text-4xl mb-4">⚠️</div>
				<h1 class="text-2xl font-bold text-foreground mb-2">
					Invalid Request
				</h1>
				<p class="text-foreground-muted">
					Missing device identifier. Please try again from your Mac
					app.
				</p>
			</div>
		{:else if paired}
			<div class="text-center">
				<div class="text-success text-6xl mb-4">✅</div>
				<h1 class="text-2xl font-bold text-foreground mb-2">
					Pairing Successful!
				</h1>
				<p class="text-foreground-muted mb-6">
					Your Mac is now connected to Ariata. You can close this
					window and return to your Mac.
				</p>
				<div
					class="bg-success-subtle border border-success rounded-lg p-4"
				>
					<p class="text-sm text-success">
						<strong>{deviceName || "Your Mac"}</strong> is now syncing
						data.
					</p>
				</div>
			</div>
		{:else}
			<div class="text-center mb-6">
				<div class="text-primary text-4xl mb-4">💻</div>
				<h1 class="text-2xl font-bold text-foreground mb-2">
					Pair Your Mac
				</h1>
				<p class="text-foreground-muted mb-4">
					Confirm pairing for this device:
				</p>

				<div
					class="bg-surface-elevated border border-border rounded-lg p-4 mb-6 text-left"
				>
					<div class="text-sm text-foreground-subtle mb-2">
						Device ID
					</div>
					<div
						class="font-mono text-xs text-foreground-muted break-all"
					>
						{deviceId}
					</div>
				</div>

				<div class="mb-6">
					<label
						for="device-name"
						class="block text-sm font-medium text-foreground-muted mb-2 text-left"
					>
						Device Name (optional)
					</label>
					<Input
						id="device-name"
						type="text"
						bind:value={deviceName}
						placeholder="My MacBook Pro"
					/>
				</div>

				{#if error}
					<div
						class="mb-4 bg-error-subtle border border-error rounded-lg p-3"
					>
						<p class="text-sm text-error">{error}</p>
					</div>
				{/if}

				<Button
					variant="primary"
					onclick={confirmPairing}
					disabled={pairing}
					class="w-full"
				>
					{pairing ? "Pairing..." : "Confirm Pairing"}
				</Button>

				<p class="text-xs text-foreground-subtle mt-4">
					By pairing this device, you authorize it to sync your
					personal data to Ariata.
				</p>
			</div>
		{/if}
	</div>
</div>
