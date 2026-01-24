<script lang="ts">
    import { enhance } from "$app/forms";
    import "iconify-icon";

    let loading = $state(false);
    let success = $state(false);

    function handleSubmit(e: SubmitEvent) {
        e.preventDefault();
        loading = true;

        // Simulate network request since backend is out of scope for now
        setTimeout(() => {
            loading = false;
            success = true;

            // Reset success message after a few seconds
            setTimeout(() => {
                success = false;
                // access form element safely to reset
                (e.target as HTMLFormElement).reset();
            }, 3000);
        }, 1000);
    }
</script>

<div class="flex h-full w-full flex-col overflow-hidden bg-[var(--surface-1)]">
    <!-- Header -->
    <div
        class="border-b border-[var(--border-subtle)] bg-[var(--surface-1)] px-8 py-6"
    >
        <div class="mx-auto max-w-2xl">
            <h1
                class="text-2xl font-semibold tracking-tight text-[var(--color-foreground)]"
            >
                Feedback
            </h1>
            <p class="mt-2 text-[var(--color-foreground-muted)]">
                Help us improve. Share your thoughts, report bugs, or request
                new features.
            </p>
        </div>
    </div>

    <!-- Content -->
    <div class="flex-1 overflow-y-auto px-8 py-8">
        <div class="mx-auto max-w-2xl">
            {#if success}
                <div
                    class="mb-6 flex items-center gap-3 rounded-lg border border-emerald-500/20 bg-emerald-500/10 px-4 py-3 text-emerald-500"
                >
                    <iconify-icon
                        icon="ri:checkbox-circle-line"
                        width="20"
                        height="20"
                    ></iconify-icon>
                    <span class="font-medium"
                        >Thanks for your feedback! We'll take a look.</span
                    >
                </div>
            {/if}

            <form onsubmit={handleSubmit} class="flex flex-col gap-6">
                <div class="flex flex-col gap-2">
                    <label
                        for="type"
                        class="text-sm font-medium text-[var(--color-foreground)]"
                    >
                        Type
                    </label>
                    <div class="grid grid-cols-3 gap-3">
                        <label class="cursor-pointer">
                            <input
                                type="radio"
                                name="type"
                                value="feedback"
                                class="peer sr-only"
                                checked
                            />
                            <div
                                class="flex items-center justify-center gap-2 rounded-md border border-[var(--border-subtle)] bg-[var(--surface-2)] py-2.5 text-sm font-medium text-[var(--color-foreground-muted)] transition-all peer-checked:border-[var(--color-foreground)] peer-checked:text-[var(--color-foreground)] hover:border-[var(--border-strong)]"
                            >
                                <iconify-icon
                                    icon="ri:chat-smile-2-line"
                                    width="16"
                                ></iconify-icon>
                                General
                            </div>
                        </label>
                        <label class="cursor-pointer">
                            <input
                                type="radio"
                                name="type"
                                value="bug"
                                class="peer sr-only"
                            />
                            <div
                                class="flex items-center justify-center gap-2 rounded-md border border-[var(--border-subtle)] bg-[var(--surface-2)] py-2.5 text-sm font-medium text-[var(--color-foreground-muted)] transition-all peer-checked:border-[var(--color-foreground)] peer-checked:text-[var(--color-foreground)] hover:border-[var(--border-strong)]"
                            >
                                <iconify-icon icon="ri:bug-line" width="16"
                                ></iconify-icon>
                                Bug
                            </div>
                        </label>
                        <label class="cursor-pointer">
                            <input
                                type="radio"
                                name="type"
                                value="feature"
                                class="peer sr-only"
                            />
                            <div
                                class="flex items-center justify-center gap-2 rounded-md border border-[var(--border-subtle)] bg-[var(--surface-2)] py-2.5 text-sm font-medium text-[var(--color-foreground-muted)] transition-all peer-checked:border-[var(--color-foreground)] peer-checked:text-[var(--color-foreground)] hover:border-[var(--border-strong)]"
                            >
                                <iconify-icon
                                    icon="ri:lightbulb-flash-line"
                                    width="16"
                                ></iconify-icon>
                                Feature
                            </div>
                        </label>
                    </div>
                </div>

                <div class="flex flex-col gap-2">
                    <label
                        for="message"
                        class="text-sm font-medium text-[var(--color-foreground)]"
                    >
                        Message
                    </label>
                    <textarea
                        id="message"
                        name="message"
                        rows="6"
                        required
                        placeholder="What's on your mind?"
                        class="w-full resize-none rounded-lg border border-[var(--border-subtle)] bg-[var(--surface-2)] p-3 text-sm text-[var(--color-foreground)] placeholder-[var(--color-foreground-subtle)] outline-none transition-all focus:border-[var(--color-primary)] focus:ring-1 focus:ring-[var(--color-primary)]"
                    ></textarea>
                </div>

                <div class="flex justify-end">
                    <button
                        type="submit"
                        disabled={loading}
                        class="flex items-center gap-2 rounded-lg bg-[var(--color-foreground)] px-4 py-2 text-sm font-medium text-[var(--surface-1)] transition-transform hover:scale-[1.02] active:scale-[0.98] disabled:opacity-50"
                    >
                        {#if loading}
                            <iconify-icon
                                icon="ri:loader-4-line"
                                class="animate-spin"
                                width="16"
                            ></iconify-icon>
                            Sending...
                        {:else}
                            <span>Send Feedback</span>
                            <iconify-icon icon="ri:send-plane-fill" width="16"
                            ></iconify-icon>
                        {/if}
                    </button>
                </div>
            </form>
        </div>
    </div>
</div>
