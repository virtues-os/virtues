<script lang="ts">
    /**
     * Manual Device Link - Enter Device ID manually (for iOS)
     */
    import { Button, Input } from "$lib";
    import type { DeviceInfo } from "$lib/types/device-pairing";

    interface Props {
        deviceType: string; // e.g., "ios"
        deviceName: string; // The name entered in the wizard step 2
        onSuccess: (sourceId: string, deviceInfo: DeviceInfo) => void;
        onCancel?: () => void;
        onComplete: () => void;
    }

    let { deviceType, deviceName, onSuccess, onCancel, onComplete }: Props =
        $props();

    let deviceId = $state("");
    let isLinking = $state(false);
    let error = $state<string | null>(null);
    let apiEndpoint = $state("");
    let isLoadingEndpoint = $state(true);

    // Fetch server endpoint on mount
    $effect(() => {
        if (typeof window !== "undefined") {
            fetch("/api/app/server-info")
                .then((r) => r.json())
                .then((data) => {
                    apiEndpoint = data.apiEndpoint;
                    isLoadingEndpoint = false;
                })
                .catch(() => {
                    apiEndpoint = `${window.location.origin}/api`;
                    isLoadingEndpoint = false;
                });
        }
    });

    async function copyEndpoint() {
        try {
            await navigator.clipboard.writeText(apiEndpoint);
        } catch (err) {
            console.error("Failed to copy endpoint:", err);
        }
    }

    // Validate UUID format
    let isValidId = $derived(
        /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i.test(
            deviceId.trim(),
        ),
    );

    async function handleLink() {
        if (!isValidId) {
            error = "Please enter a valid Device ID (UUID)";
            return;
        }

        isLinking = true;
        error = null;

        try {
            const res = await fetch("/api/devices/pairing/link", {
                method: "POST",
                headers: { "Content-Type": "application/json" },
                body: JSON.stringify({
                    device_id: deviceId.trim(),
                    name: deviceName || "My Device",
                    device_type: deviceType,
                }),
            });

            if (!res.ok) {
                const data = await res.json();
                throw new Error(data.error || "Linking failed");
            }

            const data = await res.json();

            // Construct mock DeviceInfo since the manual link endpoint returns minimal data
            // The backend currently returns PairingCompleted { source_id, device_token, available_streams }
            // Auto-complete the wizard
            onComplete();
        } catch (err) {
            error = err instanceof Error ? err.message : "Linking failed";
        } finally {
            isLinking = false;
        }
    }
</script>

<div class="space-y-6">
    <div class="bg-surface-elevated p-4 rounded-lg border border-border">
        <div class="flex items-start gap-4 mb-4 pb-4 border-b border-border">
            <img 
                src="/images/app-store-qr.png" 
                alt="Download Virtues from App Store" 
                class="w-20 h-20"
            />
            <div>
                <h4 class="text-sm font-medium text-foreground mb-1">
                    Don't have the app yet?
                </h4>
                <p class="text-sm text-foreground-muted">
                    Scan to download Virtues from the App Store, or visit
                    <a 
                        href="https://apps.apple.com/us/app/virtues-personal-ai/id6756082640" 
                        target="_blank" 
                        rel="noopener noreferrer"
                        class="text-primary hover:underline"
                    >apps.apple.com</a>
                </p>
            </div>
        </div>
        <h4 class="text-sm font-medium text-foreground mb-3">
            Setup Instructions:
        </h4>
        <ol
            class="list-decimal list-inside text-sm text-foreground-muted space-y-2"
        >
            <li>Open <strong>Settings</strong> in the Virtues iOS app</li>
            <li>
                Set <strong>Server Endpoint</strong> to:
                <div
                    class="flex items-center gap-2 mt-1 ml-4 bg-surface p-2 rounded border border-border w-fit max-w-full"
                >
                    <code class="text-xs font-mono text-foreground break-all">
                        {isLoadingEndpoint ? "Loading..." : apiEndpoint}
                    </code>
                    <button
                        class="text-xs text-primary hover:underline whitespace-nowrap"
                        onclick={copyEndpoint}
                        type="button"
                    >
                        Copy
                    </button>
                </div>
            </li>
            <li>Copy your <strong>Device ID</strong> from the app</li>
            <li>Paste it below to link</li>
        </ol>
    </div>

    <div>
        <label
            for="manual-device-id"
            class="block text-sm text-foreground-muted mb-2"
        >
            Device ID (UUID)
        </label>
        <Input
            id="manual-device-id"
            type="text"
            bind:value={deviceId}
            placeholder="e.g. 123e4567-e89b-..."
            class="font-mono"
            disabled={isLinking}
        />
    </div>

    {#if error}
        <div class="p-3 bg-error-subtle border border-error rounded-lg">
            <p class="text-sm text-error">{error}</p>
        </div>
    {/if}

    <div class="flex justify-end gap-3 pt-4 border-t border-border">
        <Button variant="ghost" onclick={onCancel} disabled={isLinking}>
            Cancel
        </Button>
        <Button
            variant="primary"
            onclick={handleLink}
            disabled={!isValidId || isLinking}
        >
            {isLinking ? "Linking..." : "Link Device"}
        </Button>
    </div>
</div>
