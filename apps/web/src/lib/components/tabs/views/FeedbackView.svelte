<script lang="ts">
    import "iconify-icon";
    import type { Tab } from "$lib/tabs/types";
    import { Page, Textarea, Button } from "$lib";

    let { tab, active }: { tab: Tab; active: boolean } = $props();

    let loading = $state(false);
    let success = $state(false);
    let type = $state("feedback");
    let message = $state("");

    async function handleSubmit(e?: SubmitEvent | MouseEvent) {
        e?.preventDefault();
        loading = true;

        try {
            const res = await fetch("/api/feedback", {
                method: "POST",
                headers: { "Content-Type": "application/json" },
                body: JSON.stringify({ type: type, message: message }),
            });

            if (res.ok) {
                success = true;
                message = ""; // clear message

                // Reset success message after a few seconds
                setTimeout(() => {
                    success = false;
                }, 3000);
            } else {
                console.error("Failed to submit feedback");
            }
        } catch (err) {
            console.error("Error submitting feedback:", err);
        } finally {
            loading = false;
        }
    }
</script>

<Page>
    <div class="mx-auto max-w-2xl py-8">
        <div class="mb-8">
            <h1 class="text-3xl font-serif font-medium text-foreground mb-2">
                Feedback
            </h1>
            <p class="text-foreground-muted">
                Help us improve. Share your thoughts, report bugs, or request
                new features.
            </p>
        </div>

        {#if success}
            <div
                class="mb-6 flex items-center gap-3 rounded-lg border border-success/20 bg-success/10 px-4 py-3 text-success animate-in fade-in slide-in-from-top-2"
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

        <form onsubmit={(e) => handleSubmit(e)} class="flex flex-col gap-6">
            <div class="flex flex-col gap-2">
                <label for="type" class="text-sm font-medium text-foreground">
                    Type
                </label>
                <div class="grid grid-cols-3 gap-3">
                    {#each [{ value: "feedback", label: "General", icon: "ri:chat-smile-2-line" }, { value: "bug", label: "Bug", icon: "ri:bug-line" }, { value: "feature", label: "Feature", icon: "ri:lightbulb-flash-line" }] as option}
                        <label class="cursor-pointer group relative">
                            <input
                                type="radio"
                                name="type"
                                value={option.value}
                                bind:group={type}
                                class="peer sr-only"
                            />
                            <div
                                class="flex items-center justify-center gap-2 rounded-lg border border-border bg-surface px-4 py-3 text-sm font-medium text-foreground-muted transition-all peer-checked:border-foreground peer-checked:text-foreground peer-checked:bg-surface-elevated hover:bg-surface-elevated hover:text-foreground"
                            >
                                <iconify-icon icon={option.icon} width="16"
                                ></iconify-icon>
                                {option.label}
                            </div>
                        </label>
                    {/each}
                </div>
            </div>

            <Textarea
                label="Message"
                bind:value={message}
                placeholder="What's on your mind?"
                rows={6}
                required
            />

            <div class="flex justify-end">
                <Button
                    type="submit"
                    disabled={loading || !message.trim()}
                    onclick={handleSubmit}
                >
                    {#if loading}
                        <div class="flex items-center gap-2">
                            <iconify-icon
                                icon="ri:loader-4-line"
                                class="animate-spin"
                                width="16"
                            ></iconify-icon>
                            <span>Sending...</span>
                        </div>
                    {:else}
                        Send Feedback
                    {/if}
                </Button>
            </div>
        </form>
    </div>
</Page>
