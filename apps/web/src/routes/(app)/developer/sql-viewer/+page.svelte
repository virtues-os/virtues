<script lang="ts">
    import { Page } from "$lib";
    import "iconify-icon";
</script>

<Page>
    <div
        class="flex h-full flex-col items-center justify-center p-8 text-center"
    >
        <div class="mb-6 rounded-full bg-surface-elevated p-6">
            <iconify-icon
                icon="ri:database-2-line"
                class="text-4xl text-foreground-subtle"
            ></iconify-icon>
        </div>
        <h1 class="mb-2 text-2xl font-serif font-medium text-foreground">
            SQL Viewer
        </h1>
        <p class="max-w-md text-foreground-muted">
            Execute SQL queries and inspect the database directly. This feature
            is coming soon.
        </p>
    </div>
</Page>
