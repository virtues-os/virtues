<script lang="ts">
    import type { Snippet } from "svelte";

    let {
        title,
        description,
        level = 1,
        class: className = "",
        children,
        actions,
    }: {
        title?: string;
        description?: string;
        level?: 1 | 2 | 3;
        class?: string;
        children?: Snippet;
        actions?: Snippet;
    } = $props();

    const titleClasses: Record<1 | 2 | 3, string> = {
        1: "text-3xl font-serif font-medium text-foreground",
        2: "text-2xl font-serif font-medium text-foreground",
        3: "text-xl font-serif font-medium text-foreground",
    };

    const wrapperMargin: Record<1 | 2 | 3, string> = {
        1: "mb-6",
        2: "mb-4",
        3: "mb-3",
    };
</script>

<div class="page-heading {wrapperMargin[level]} {className}">
    <div class="page-heading-row">
        <div class="page-heading-titles">
            {#if title}
                {#if level === 1}
                    <h1 class={titleClasses[1]}>{title}</h1>
                {:else if level === 2}
                    <h2 class={titleClasses[2]}>{title}</h2>
                {:else}
                    <h3 class={titleClasses[3]}>{title}</h3>
                {/if}
            {:else if children}
                {@render children()}
            {/if}
            {#if description}
                <p class="page-heading-description">{description}</p>
            {/if}
        </div>
        {#if actions}
            <div class="page-heading-actions">{@render actions()}</div>
        {/if}
    </div>
</div>

<style>
    .page-heading-row {
        display: flex;
        align-items: flex-start;
        justify-content: space-between;
        gap: 1rem;
    }
    .page-heading-titles {
        min-width: 0;
        flex: 1;
    }
    .page-heading-description {
        margin-top: 0.25rem;
        color: var(--color-foreground-muted);
        font-size: 0.875rem;
        line-height: 1.5;
    }
    .page-heading-actions {
        flex-shrink: 0;
    }
</style>
