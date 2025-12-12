<script lang="ts">
    import type { Snippet } from "svelte";

    let {
        variant = "primary",
        size = "md",
        disabled = false,
        type = "button",
        onclick,
        class: className = "",
        children,
    }: {
        variant?: "primary" | "secondary" | "ghost" | "danger";
        size?: "sm" | "md" | "lg";
        disabled?: boolean;
        type?: "button" | "submit" | "reset";
        onclick?: (e: MouseEvent) => void;
        class?: string;
        children: Snippet;
    } = $props();

    const variantStyles: Record<string, string> = {
        primary: "btn-primary",
        secondary: "btn-secondary",
        ghost: "btn-ghost",
        danger: "bg-error text-surface hover:bg-error/90 focus:ring-error",
    };

    const sizeStyles: Record<string, string> = {
        sm: "px-3 py-1.5 text-sm",
        md: "px-4 py-2 text-sm",
        lg: "px-6 py-3 text-base",
    };
</script>

<button
    {type}
    class="inline-flex items-center justify-center font-medium transition-colors rounded-lg focus:outline-none focus:ring-2 focus:ring-offset-2 disabled:opacity-50 disabled:cursor-not-allowed cursor-pointer {variantStyles[
        variant
    ]} {sizeStyles[size]} {className}"
    {disabled}
    {onclick}
>
    {@render children()}
</button>
