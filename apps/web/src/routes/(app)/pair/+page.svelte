<script lang="ts">
	import { page } from "$app/stores";
	import { onMount } from "svelte";
	import { Input, Button } from "$lib";

	// Check if this is a magic link visit (mac app)
	const isMagicLink = $page.url.searchParams.has("device_id");

	// Initialize from URL param, but allow editing
	let deviceId = $state($page.url.searchParams.get("device_id") || "");
	let deviceName = $state("");
	let pairing = $state(false);
	let paired = $state(false);
	let error = $state("");

	// Determine if valid UUID
	let isValidId = $derived(
		/^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i.test(
			deviceId.trim(),
		),
	);

	async function confirmPairing() {
		if (!isValidId) {
			error = "Please enter a valid Device ID format (UUID)";
			return;
		}

		pairing = true;
		error = "";

		try {
			// Call the Rust backend directly
			const res = await fetch("/api/devices/pairing/link", {
				method: "POST",
				headers: { "Content-Type": "application/json" },
				body: JSON.stringify({
					device_id: deviceId.trim(),
					name:
						deviceName ||
						(isMagicLink ? "My Mac" : "My iOS Device"),
					device_type: isMagicLink ? "mac" : "ios",
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
		{#if paired}
			<div class="text-center">
				<div class="text-success text-6xl mb-4">✅</div>
				<h1 class="text-2xl font-bold text-foreground mb-2">
					Pairing Successful!
				</h1>
				<p class="text-foreground-muted mb-6">
					Your device is now connected to Ariata.
				</p>
				<div
					class="bg-success-subtle border border-success rounded-lg p-4"
				>
					<p class="text-sm text-success">
						<strong>{deviceName || "Your Device"}</strong> is now syncing
						data.
					</p>
				</div>
			</div>
		{:else}
			<div class="text-center mb-6">
				<div class="text-primary text-4xl mb-4">📱</div>
				<h1 class="text-2xl font-bold text-foreground mb-2">
					Pair Your Device
				</h1>
				<p class="text-foreground-muted mb-4">
					Enter the Device ID found in your app settings:
				</p>

				<div class="mb-4 text-left">
					<label
						for="device-id"
						class="block text-sm font-medium text-foreground-muted mb-2"
					>
						Device ID (UUID)
					</label>
					<Input
						id="device-id"
						type="text"
						bind:value={deviceId}
						placeholder="e.g. 123e4567-e89b-..."
						class="font-mono"
					/>
				</div>

				<div class="mb-6 text-left">
					<label
						for="device-name"
						class="block text-sm font-medium text-foreground-muted mb-2"
					>
						Device Name (optional)
					</label>
					<Input
						id="device-name"
						type="text"
						bind:value={deviceName}
						placeholder="My iPhone"
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
					disabled={pairing || !isValidId}
					class="w-full"
				>
					{pairing ? "Pairing..." : "Link Device"}
				</Button>

				<p class="text-xs text-foreground-subtle mt-4">
					By linking this device, you authorize it to sync your
					personal data to Ariata.
				</p>
			</div>
		{/if}
	</div>
</div>
