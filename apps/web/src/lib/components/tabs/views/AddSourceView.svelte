<script lang="ts">
    /**
     * Add Source View - Full wizard for OAuth sources, stream configuration, and source naming.
     * Integrated into the tab system.
     */
    import { Button, Page, Badge, Input } from "$lib";
    import TypedSelect from "$lib/components/TypedSelect.svelte";
    import DevicePairing from "$lib/components/DevicePairing.svelte";
    import ManualDeviceLink from "$lib/components/ManualDeviceLink.svelte";
    import PlaidLink from "$lib/components/PlaidLink.svelte";
    import { onMount } from "svelte";
    import type { Tab } from "$lib/tabs/types";
    import { workspaceStore } from "$lib/stores/workspace.svelte";
    import type { DeviceInfo } from "$lib/types/device-pairing";
    import type { ConnectedAccountSummary } from "$lib/api/client";
    import * as api from "$lib/api/client";
    import { toast } from "svelte-sonner";
    import "iconify-icon";

    let { tab, active }: { tab: Tab; active: boolean } = $props();

    interface CatalogSource {
        name: string;
        display_name: string;
        description: string;
        auth_type: string;
        stream_count: number;
        icon?: string;
        tier: 'free' | 'standard' | 'pro';
    }

    interface Stream {
        stream_name: string;
        display_name: string;
        description: string;
        is_enabled: boolean;
        supports_incremental: boolean;
        default_cron_schedule: string | null;
    }

    let catalog = $state<CatalogSource[]>([]);
    let selectedSource = $state<CatalogSource | null>(null);
    let sourceName = $state("");
    let selectedStreams = $state<Set<string>>(new Set());
    let availableStreams = $state<Stream[]>([]);
    let isLoading = $state(false);
    let isInitialLoading = $state(true);
    let error = $state<string | null>(null);
    let createdSourceId: string | null = $state(null);

    let currentStep = $state<1 | 2 | 3>(1);

    let oauthSources = $derived(
        catalog.filter((s) => s.auth_type === "oauth2"),
    );
    let deviceSources = $derived(
        catalog.filter((s) => s.auth_type === "device"),
    );
    let allSources = $derived(catalog.filter((s) => s.auth_type !== "none"));

    // Device pairing state
    let devicePairingSourceId: string | null = $state(null);
    let devicePairingInfo: DeviceInfo | null = $state(null);

    // Plaid Link state
    let plaidSourceId: string | null = $state(null);
    let plaidInstitutionName: string | null = $state(null);
    let plaidConnectedAccounts: ConnectedAccountSummary[] = $state([]);

    onMount(async () => {
        await loadCatalog();
        initializeFromRoute();
    });

    async function loadCatalog() {
        isInitialLoading = true;
        try {
            const res = await fetch("/api/catalog/sources");
            if (!res.ok) throw new Error("Failed to load catalog");
            catalog = await res.json();
        } catch (e) {
            error = e instanceof Error ? e.message : "Failed to load catalog";
        } finally {
            isInitialLoading = false;
        }
    }

    function initializeFromRoute() {
        const url = new URL(tab.route, window.location.origin);
        const typeParam = url.searchParams.get("type");
        const sourceId = url.searchParams.get("source_id");

        if (typeParam && catalog.length > 0) {
            const source = catalog.find((s) => s.name === typeParam);
            if (source) {
                handleSourceSelect(source);
            }
        }

        if (sourceId) {
            handleOAuthSuccess(sourceId);
        }
    }

    async function handleOAuthSuccess(sourceId: string) {
        isLoading = true;
        try {
            // Fetch streams for OAuth source
            const streams = await api.listStreams(sourceId);
            availableStreams = streams;
            createdSourceId = sourceId;
            selectedStreams = new Set(
                streams.map((s: Stream) => s.stream_name),
            );
            currentStep = 3;
        } catch (e) {
            error = e instanceof Error ? e.message : "Failed to load streams";
        } finally {
            isLoading = false;
        }
    }

    function handleSourceSelect(source: CatalogSource | null) {
        selectedSource = source;
        if (!source) {
            currentStep = 1;
            sourceName = "";
            return;
        }
        sourceName = `${source.display_name} Account`;
        currentStep = 2;
    }

    async function handleAuthorize() {
        if (!selectedSource) return;

        isLoading = true;
        error = null;

        try {
            // Pass the full return URL where user should land after OAuth completes
            // This works for any client (web dev, web prod, iOS, Mac) without
            // the backend needing to know about specific frontend URLs
            const returnUrl = `${window.location.origin}/data/sources/add`;
            const oauthResponse = await api.initiateOAuth(
                selectedSource.name,
                returnUrl,
            );
            window.location.href = oauthResponse.authorization_url;
        } catch (e) {
            error = e instanceof Error ? e.message : "Authorization failed";
            isLoading = false;
        }
    }

    async function handleDevicePairingSuccess(
        sourceId: string,
        deviceInfo: DeviceInfo,
    ) {
        devicePairingSourceId = sourceId;
        devicePairingInfo = deviceInfo;

        try {
            const streams = await api.listStreams(sourceId);
            availableStreams = streams;
            selectedStreams = new Set(
                streams.map((s: Stream) => s.stream_name),
            );
            currentStep = 3;
        } catch (e) {
            error = e instanceof Error ? e.message : "Failed to load streams";
        }
    }

    async function handleDevicePairingComplete() {
        toast.success("Device linked successfully! Default streams enabled.");
        workspaceStore.openTabFromRoute("/data/sources");
        workspaceStore.closeTab(tab.id);
    }

    function handleDevicePairingCancel() {
        selectedSource = null;
        currentStep = 1;
        devicePairingSourceId = null;
        devicePairingInfo = null;
    }

    async function handlePlaidSuccess(
        sourceId: string,
        institutionName?: string,
        connectedAccounts?: ConnectedAccountSummary[],
    ) {
        plaidSourceId = sourceId;
        plaidInstitutionName = institutionName || "Bank Account";
        plaidConnectedAccounts = connectedAccounts || [];

        try {
            const streams = await api.listStreams(sourceId);
            availableStreams = streams;
            selectedStreams = new Set(
                streams.map((s: Stream) => s.stream_name),
            );
            currentStep = 3;
        } catch (e) {
            error = e instanceof Error ? e.message : "Failed to load streams";
        }
    }

    function handlePlaidCancel() {
        selectedSource = null;
        currentStep = 1;
        plaidSourceId = null;
        plaidInstitutionName = null;
        plaidConnectedAccounts = [];
    }

    function toggleStream(streamName: string) {
        const newSet = new Set(selectedStreams);
        if (newSet.has(streamName)) {
            newSet.delete(streamName);
        } else {
            newSet.add(streamName);
        }
        selectedStreams = newSet;
    }

    async function handleEnableStreams() {
        const sourceId =
            devicePairingSourceId || plaidSourceId || createdSourceId;
        if (!sourceId) return;

        isLoading = true;
        error = null;

        try {
            // Build the stream updates for bulk API
            const streamUpdates = availableStreams.map((stream) => ({
                stream_name: stream.stream_name,
                is_enabled: selectedStreams.has(stream.stream_name),
            }));

            // Use bulk update API - single request instead of N+M requests
            await api.bulkUpdateStreams(sourceId, streamUpdates);

            const streamCount = selectedStreams.size;
            toast.success(
                `${streamCount} stream${streamCount === 1 ? "" : "s"} enabled and syncing in the background`,
            );

            workspaceStore.openTabFromRoute("/data/sources");
            workspaceStore.closeTab(tab.id);
        } catch (e) {
            error = e instanceof Error ? e.message : "Failed to enable streams";
            isLoading = false;
        }
    }

    function formatCron(cron: string | null): string {
        if (!cron) return "Manual";
        const map: Record<string, string> = {
            "0 */15 * * * *": "Every 15 minutes",
            "0 */30 * * * *": "Every 30 minutes",
            "0 0 */1 * * *": "Every hour",
            "0 0 */6 * * *": "Every 6 hours",
            "0 0 0 * * *": "Daily at midnight",
        };
        return map[cron] || cron;
    }
</script>

<Page>
    <div class="max-w-2xl">
        <div class="mb-12">
            <h1 class="text-3xl font-serif font-normal text-foreground mb-3">
                Add Source
            </h1>
            <p class="text-foreground-muted leading-relaxed">
                Connect a new data source to start syncing your personal data.
            </p>
        </div>

        {#if error}
            <div class="mb-8 p-4 border border-border bg-surface-elevated">
                <p class="text-sm font-serif text-foreground">{error}</p>
            </div>
        {/if}

        {#if isInitialLoading}
            <div class="py-12 text-center text-foreground-muted">
                Loading catalog...
            </div>
        {:else}
            <div class="space-y-12">
                <!-- Step 1: Select Provider -->
                <div>
                    <h2
                        class="text-xl font-serif font-normal text-foreground mb-6"
                    >
                        {#if selectedSource && currentStep > 1}
                            <span class="text-success">✓</span>
                        {/if}
                        1. Select Source
                    </h2>

                    <div class="space-y-4 w-2/3">
                        <TypedSelect
                            items={allSources}
                            bind:value={selectedSource}
                            onValueChange={handleSourceSelect}
                            label="Data Source"
                            placeholder="Type to search..."
                            displayKey="display_name"
                            searchKey="display_name"
                        >
                            {#snippet item(source)}
                                <div>
                                    <div class="flex items-center gap-2 mb-1">
                                        <span class="text-foreground">
                                            {source.display_name}
                                        </span>
                                        {#if source.tier !== 'free'}
                                            <Badge variant={source.tier === 'pro' ? 'primary' : 'secondary'} class="uppercase text-[10px]">
                                                {source.tier}
                                            </Badge>
                                        {/if}
                                        {#if source.auth_type === "device"}
                                            <Badge variant="primary"
                                                >Device</Badge
                                            >
                                        {/if}
                                    </div>
                                    <div class="text-sm text-foreground-muted">
                                        {source.description}
                                    </div>
                                </div>
                            {/snippet}
                        </TypedSelect>

                        {#if selectedSource}
                            <div class="pt-4 border-t border-border">
                                <p
                                    class="text-sm text-foreground-muted leading-relaxed"
                                >
                                    {selectedSource.description}
                                </p>
                                <p class="text-sm text-foreground-subtle mt-2">
                                    {selectedSource.stream_count}
                                    {selectedSource.stream_count === 1
                                        ? "stream"
                                        : "streams"} available
                                </p>
                            </div>
                        {/if}
                    </div>
                </div>

                <!-- Step 2: Authorize (OAuth), Pair (Device), or Connect Bank (Plaid) -->
                <div>
                    <h2
                        class="text-xl font-serif font-normal text-foreground mb-6"
                    >
                        {#if devicePairingInfo || plaidSourceId || createdSourceId}
                            <span class="text-success">✓</span>
                        {/if}
                        2. {selectedSource?.auth_type === "device"
                            ? "Pair Device"
                            : selectedSource?.name === "plaid"
                              ? "Connect Bank"
                              : "Authorize"}
                    </h2>

                    {#if currentStep >= 2 && selectedSource}
                        {#if selectedSource.auth_type === "device"}
                            <!-- Device Pairing Flow -->
                            <div class="space-y-6">
                                <div>
                                    <label
                                        class="block text-sm text-foreground-muted mb-2"
                                        for="device-name-input"
                                    >
                                        Device Name
                                    </label>
                                    <Input
                                        id="device-name-input"
                                        type="text"
                                        bind:value={sourceName}
                                        placeholder="e.g., My {selectedSource.display_name}"
                                        disabled={!!devicePairingInfo}
                                    />
                                    <p
                                        class="text-sm text-foreground-subtle mt-2"
                                    >
                                        A memorable name for this device
                                    </p>
                                </div>

                                {#if !devicePairingInfo && sourceName.trim()}
                                    <div class="pt-6 border-t border-border">
                                        {#if selectedSource.name === "ios"}
                                            <ManualDeviceLink
                                                deviceType={selectedSource.name}
                                                deviceName={sourceName}
                                                onSuccess={() => {}}
                                                onComplete={handleDevicePairingComplete}
                                                onCancel={handleDevicePairingCancel}
                                            />
                                        {:else}
                                            <DevicePairing
                                                deviceType={selectedSource.name}
                                                deviceName={sourceName}
                                                onSuccess={handleDevicePairingSuccess}
                                                onCancel={handleDevicePairingCancel}
                                            />
                                        {/if}
                                    </div>
                                {:else if devicePairingInfo}
                                    <div class="pt-6 border-t border-border">
                                        <p
                                            class="text-sm text-foreground-muted"
                                        >
                                            ✓ Device paired: {devicePairingInfo.device_name}
                                        </p>
                                    </div>
                                {/if}
                            </div>
                        {:else if selectedSource.name === "plaid"}
                            <!-- Plaid Link Flow -->
                            <div class="space-y-6">
                                {#if !plaidSourceId}
                                    <PlaidLink
                                        onSuccess={handlePlaidSuccess}
                                        onCancel={handlePlaidCancel}
                                    />
                                {:else}
                                    <div class="pt-6 border-t border-border">
                                        <p
                                            class="text-sm text-foreground-muted mb-3"
                                        >
                                            ✓ Connected to {plaidInstitutionName}
                                        </p>
                                        {#if plaidConnectedAccounts.length > 0}
                                            <div
                                                class="p-4 bg-surface-elevated rounded-lg"
                                            >
                                                <h4
                                                    class="text-sm font-medium text-foreground mb-2"
                                                >
                                                    Connected Accounts
                                                </h4>
                                                <ul
                                                    class="space-y-1 text-sm text-foreground-muted"
                                                >
                                                    {#each plaidConnectedAccounts as account}
                                                        <li
                                                            class="flex items-center gap-2"
                                                        >
                                                            <span
                                                                class="text-success"
                                                                >•</span
                                                            >
                                                            <span
                                                                >{account.name}</span
                                                            >
                                                            <span
                                                                class="text-foreground-subtle"
                                                            >
                                                                ({account.subtype ||
                                                                    account.account_type})
                                                                {#if account.mask}****{account.mask}{/if}
                                                            </span>
                                                        </li>
                                                    {/each}
                                                </ul>
                                            </div>
                                        {/if}
                                    </div>
                                {/if}
                            </div>
                        {:else}
                            <!-- OAuth Flow -->
                            <div class="space-y-6">
                                <div>
                                    <label
                                        class="block text-sm text-foreground-muted mb-2"
                                        for="source-name-input"
                                    >
                                        Source Name
                                    </label>
                                    <Input
                                        id="source-name-input"
                                        type="text"
                                        bind:value={sourceName}
                                        placeholder="e.g., My {selectedSource.display_name} Account"
                                    />
                                    <p
                                        class="text-sm text-foreground-subtle mt-2"
                                    >
                                        A memorable name for this connection
                                    </p>
                                </div>

                                {#if currentStep === 2}
                                    <div class="pt-6 border-t border-border">
                                        <p
                                            class="text-sm text-foreground-muted mb-4 leading-relaxed"
                                        >
                                            You'll be redirected to {selectedSource.display_name}
                                            to authorize access. We request read-only
                                            permissions.
                                        </p>
                                        <Button
                                            onclick={handleAuthorize}
                                            disabled={isLoading ||
                                                !sourceName.trim()}
                                        >
                                            {#if isLoading}
                                                Authorizing...
                                            {:else}
                                                Authorize
                                            {/if}
                                        </Button>
                                    </div>
                                {:else if currentStep > 2}
                                    <div class="pt-6 border-t border-border">
                                        <p
                                            class="text-sm text-foreground-muted"
                                        >
                                            ✓ Connected as "{sourceName}"
                                        </p>
                                    </div>
                                {/if}
                            </div>
                        {/if}
                    {:else}
                        <p class="text-sm text-foreground-subtle">
                            Complete step 1 to continue
                        </p>
                    {/if}
                </div>

                <!-- Step 3: Enable Streams -->
                <div>
                    <h2
                        class="text-xl font-serif font-normal text-foreground mb-6"
                    >
                        {#if currentStep === 3 && isLoading}
                            <span class="animate-pulse">...</span>
                        {/if}
                        3. Enable Streams
                    </h2>

                    {#if currentStep >= 3}
                        <p
                            class="text-sm text-foreground-muted mb-6 leading-relaxed"
                        >
                            Choose which data streams to enable. All streams are
                            selected by default.
                        </p>

                        <div class="space-y-3 mb-8">
                            {#each availableStreams as stream}
                                <label
                                    class="flex items-start gap-4 p-4 border border-border cursor-pointer hover:border-border-subtle transition-colors"
                                >
                                    <input
                                        type="checkbox"
                                        checked={selectedStreams.has(
                                            stream.stream_name,
                                        )}
                                        onclick={() =>
                                            toggleStream(stream.stream_name)}
                                        class="mt-1 w-4 h-4 border-border"
                                    />
                                    <div class="flex-1">
                                        <div
                                            class="flex items-center gap-3 mb-1"
                                        >
                                            <h3
                                                class="font-serif text-foreground"
                                            >
                                                {stream.display_name}
                                            </h3>
                                            {#if stream.supports_incremental}
                                                <span
                                                    class="text-xs text-foreground-subtle"
                                                >
                                                    Incremental
                                                </span>
                                            {/if}
                                        </div>
                                        <p
                                            class="text-sm text-foreground-muted mb-2 leading-relaxed"
                                        >
                                            {stream.description}
                                        </p>
                                        <p
                                            class="text-xs text-foreground-subtle"
                                        >
                                            {formatCron(
                                                stream.default_cron_schedule,
                                            )}
                                        </p>
                                    </div>
                                </label>
                            {/each}
                        </div>

                        <div class="pt-6 border-t border-border">
                            <Button
                                onclick={handleEnableStreams}
                                disabled={isLoading ||
                                    selectedStreams.size === 0}
                            >
                                {#if isLoading}
                                    Enabling...
                                {:else}
                                    Enable {selectedStreams.size}
                                    {selectedStreams.size === 1
                                        ? "Stream"
                                        : "Streams"}
                                {/if}
                            </Button>
                        </div>
                    {:else}
                        <p class="text-sm text-foreground-subtle italic">
                            Complete steps 1 & 2 to continue
                        </p>
                    {/if}
                </div>
            </div>
        {/if}
    </div>
</Page>
