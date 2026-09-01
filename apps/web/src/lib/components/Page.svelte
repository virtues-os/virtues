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

    /**
     * Mobile-first, because 48px of gutter on each side of a 375px screen left
     * 279px of content — a quarter of the phone spent on margin, and narrow
     * enough that rows with an intrinsic width (a dot-leader ledger, a toolbar)
     * pushed the page into sideways scrolling. The `md:` step is 768px, the
     * same line `mobileLayout` draws, so the desktop measure is unchanged.
     *
     * Viewport-keyed rather than container-keyed on purpose: `container-type`
     * implies `contain: layout`, which would make every page a containing block
     * for its `position: fixed` descendants — modals, popovers, the editor's
     * footer bar. Not a trade worth making for a gutter.
     */
    /**
     * The phone has no bottom chrome anymore — just the home indicator's
     * safe-area to stay clear of.
     */
    const phoneBottomRoom = "pb-[calc(1.5rem+env(safe-area-inset-bottom))]";

    /**
     * A touch more room at the top on the phone than the old `py-8`, because
     * the shell no longer puts anything above the view — no toolbar, no back
     * chevron, nothing. The page begins directly under the status bar, and
     * without a little air it reads as though it has been shoved up against
     * the notch.
     */
    const paddingClass = $derived({
        default: `px-5 pt-10 ${phoneBottomRoom} md:p-12`,
        compact: `px-4 pt-8 ${phoneBottomRoom} md:px-6 md:py-8`,
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
