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
        maxWidth?: "none" | "narrow" | "prose" | "wide" | "full";
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
        narrow: "max-w-2xl mx-auto",
        prose: "max-w-3xl mx-auto",
        wide: "max-w-6xl mx-auto",
        full: "max-w-7xl mx-auto",
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
