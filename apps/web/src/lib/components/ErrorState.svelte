<script lang="ts">
    import Icon from "$lib/components/Icon.svelte";

    let {
        title = "Something went wrong",
        message,
        onRetry,
        class: className = "",
    }: {
        title?: string;
        message?: string;
        onRetry?: () => void;
        class?: string;
    } = $props();
</script>

<div class="state-wrap {className}">
    <Icon icon="ri:error-warning-line" width="32" />
    <p class="state-title">{title}</p>
    {#if message}
        <p class="state-message">{message}</p>
    {/if}
    {#if onRetry}
        <button type="button" class="retry-btn" onclick={onRetry}>
            <Icon icon="ri:refresh-line" width="14" />
            Retry
        </button>
    {/if}
</div>

<style>
    .state-wrap {
        display: flex;
        flex-direction: column;
        align-items: center;
        justify-content: center;
        gap: 0.5rem;
        padding: 3rem 2rem;
        color: var(--color-error);
        text-align: center;
    }
    .state-title {
        margin: 0;
        font-size: 0.9375rem;
        font-weight: 500;
    }
    .state-message {
        margin: 0;
        font-size: 0.875rem;
        color: var(--color-foreground-muted);
    }
    .retry-btn {
        display: inline-flex;
        align-items: center;
        gap: 0.375rem;
        margin-top: 0.5rem;
        padding: 0.375rem 0.75rem;
        font-size: 0.8125rem;
        color: var(--color-primary);
        background: transparent;
        border: 1px solid var(--color-primary);
        border-radius: 6px;
        cursor: pointer;
    }
    .retry-btn:hover {
        background: color-mix(in srgb, var(--color-primary) 10%, transparent);
    }
</style>
