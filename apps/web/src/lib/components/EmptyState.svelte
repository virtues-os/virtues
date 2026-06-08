<script lang="ts">
    import Icon from "$lib/components/Icon.svelte";
    import type { Snippet } from "svelte";

    let {
        icon = "ri:inbox-line",
        title,
        message,
        class: className = "",
        actions,
    }: {
        icon?: string;
        title?: string;
        message?: string;
        class?: string;
        actions?: Snippet;
    } = $props();
</script>

<div class="state-wrap {className}">
    <Icon {icon} width="40" />
    {#if title}
        <p class="state-title">{title}</p>
    {/if}
    {#if message}
        <p class="state-message">{message}</p>
    {/if}
    {#if actions}
        <div class="state-actions">{@render actions()}</div>
    {/if}
</div>

<style>
    .state-wrap {
        display: flex;
        flex-direction: column;
        align-items: center;
        justify-content: center;
        gap: 0.75rem;
        padding: 3rem 2rem;
        color: var(--color-foreground-muted);
        text-align: center;
    }
    .state-wrap :global(svg) {
        opacity: 0.5;
    }
    .state-title {
        margin: 0;
        font-size: 0.9375rem;
        font-weight: 500;
        color: var(--color-foreground);
    }
    .state-message {
        margin: 0;
        font-size: 0.875rem;
        color: var(--color-foreground-muted);
    }
    .state-actions {
        margin-top: 0.5rem;
    }
</style>
