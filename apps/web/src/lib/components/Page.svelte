<script lang="ts">
    import { twMerge } from "tailwind-merge";
    let { children, className = "", scrollable = true } = $props();
</script>

<div class={twMerge("page-container bg-background p-12", scrollable && "overflow-y-auto h-full", className)}>
    {@render children()}
</div>

<style>
    .page-container {
        /* Animation removed - was causing unwanted slide-in effect on workspace switches */
    }
</style>
