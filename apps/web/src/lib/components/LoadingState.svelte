<script lang="ts">
    import Icon from "$lib/components/Icon.svelte";

    let {
        message,
        class: className = "",
    }: {
        message?: string;
        class?: string;
    } = $props();
</script>

<div class="state-wrap {className}">
    <Icon icon="ri:loader-4-line" width="24" />
    {#if message}
        <p class="state-message">{message}</p>
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
        animation: spin 1s linear infinite;
    }
    .state-message {
        margin: 0;
        font-size: 0.875rem;
    }
    @keyframes spin {
        from { transform: rotate(0deg); }
        to { transform: rotate(360deg); }
    }
</style>
