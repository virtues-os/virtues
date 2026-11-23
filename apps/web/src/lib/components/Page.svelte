<script lang="ts">
    import type { Snippet } from "svelte";
    import PageHeading from "./PageHeading.svelte";
    import { usePageActions } from "$lib/stores/paneActions.svelte";
    import { usePageWidth, type PageWidth } from "$lib/stores/pageWidth.svelte";

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
         * The width the page is designed for (stores/pageWidth.svelte.ts):
         * `reading` for text and plain lists, `medium` for a page with a side
         * column, `full` for tables and grids. You can change it from the
         * page bar. `none` is a page without a column, which has no width
         * to change.
         */
        maxWidth?: "none" | PageWidth;
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

    // On the desktop the page's title is the last segment of its tab's
    // action toolbar, and its controls sit at the toolbar's end. The phone
    // has no toolbar, so there both stay at the top of the page.
    const toolbar = usePageActions(() => actions);
    const headingTitle = $derived(toolbar.inToolbar ? undefined : title);
    const headingActions = $derived(toolbar.inToolbar ? undefined : actions);
    const showHeading = $derived(!!headingTitle || !!description);

    const width = usePageWidth(() => (maxWidth === "none" ? null : maxWidth));
</script>

<div class={["page-container h-full", paddingClass, scrollable && "overflow-y-auto", className]}>
    {#if maxWidth !== "none"}
        <div class="page-column" style:max-width={width.css}>
            {#if heading}
                {@render heading()}
            {:else if title && showHeading}
                <PageHeading title={headingTitle} {description} actions={headingActions} />
            {/if}
            {@render children()}
        </div>
    {:else}
        {#if heading}
            {@render heading()}
        {:else if title && showHeading && padding === "none"}
            <!-- An edge-to-edge page still gives its title the phone's gutter. -->
            <div class="bare-heading">
                <PageHeading title={headingTitle} {description} actions={headingActions} />
            </div>
        {:else if title && showHeading}
            <PageHeading title={headingTitle} {description} actions={headingActions} />
        {/if}
        {@render children()}
    {/if}
</div>

<style>
    .page-column {
        margin-inline: auto;
        transition: max-width var(--motion-hover-out, 0.2s) var(--ease-premium, ease-out);
    }

    /* The default padding's top room on the phone, with the 24px gutter an
       edge-to-edge page's own rows use. */
    .bare-heading {
        flex: none;
        padding: 40px 24px 0;
    }

    @media (prefers-reduced-motion: reduce) {
        .page-column {
            transition: none;
        }
    }
</style>
