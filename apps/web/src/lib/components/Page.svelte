<script lang="ts">
    import { twMerge } from "tailwind-merge";
    import type { Snippet } from "svelte";
    import PageHeading from "./PageHeading.svelte";

    let {
        title,
        description,
        maxWidth = "none",
        padding = "default",
        scrollable = true,
        class: className = "",
        children,
        actions,
        heading,
    }: {
        title?: string;
        description?: string;
        /**
         * Two measures, so views read as siblings: `prose` for reading, `wide`
         * for tables and grids. (Was five — narrow/full were each used once or
         * twice and only made neighbouring pages disagree by a few rem.)
         *
         * This is chrome, chosen by the view. The *reader's* width preference
         * is a different thing and lives in `stores/pageDisplay.svelte.ts`.
         */
        maxWidth?: "none" | "prose" | "wide";
        padding?: "default" | "compact" | "none";
        scrollable?: boolean;
        class?: string;
        children: Snippet;
        actions?: Snippet;
        heading?: Snippet;
    } = $props();

    const paddingClass = $derived({
        default: "p-12",
        compact: "px-6 py-8",
        none: "",
    }[padding]);

    const maxWidthClass = $derived({
        none: "",
        prose: "max-w-3xl mx-auto",
        wide: "max-w-6xl mx-auto",
    }[maxWidth]);
</script>

<div class={twMerge("page-container h-full", paddingClass, scrollable && "overflow-y-auto", className)}>
    {#if maxWidthClass}
        <div class={maxWidthClass}>
            {#if heading}
                {@render heading()}
            {:else if title}
                <PageHeading {title} {description} {actions} />
            {/if}
            {@render children()}
        </div>
    {:else}
        {#if heading}
            {@render heading()}
        {:else if title}
            <PageHeading {title} {description} {actions} />
        {/if}
        {@render children()}
    {/if}
</div>
