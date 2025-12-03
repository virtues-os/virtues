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
    } = $props<{
        variant?: "primary" | "secondary" | "ghost" | "danger" | "manuscript" | "manuscript-ghost";
        size?: "sm" | "md" | "lg";
        disabled?: boolean;
        type?: "button" | "submit" | "reset";
        onclick?: (e: MouseEvent) => void;
        class?: string;
        children: Snippet;
    }>();

    const baseStyles =
        "inline-flex items-center justify-center font-medium transition-colors focus:outline-none disabled:opacity-50 disabled:cursor-not-allowed cursor-pointer hover:cursor-pointer";

    const baseStylesDefault = "rounded-lg focus:ring-2 focus:ring-offset-2";
    const isManuscript = variant === "manuscript" || variant === "manuscript-ghost";

    const variantStyles = {
        primary: "btn-primary",
        secondary: "btn-secondary",
        ghost: "btn-ghost",
        danger: "bg-error text-surface hover:bg-error/90 focus:ring-error",
        manuscript: "btn-manuscript",
        "manuscript-ghost": "btn-manuscript-ghost",
    };

    const sizeStyles = {
        sm: "px-3 py-1.5 text-sm",
        md: "px-4 py-2 text-sm",
        lg: "px-6 py-3 text-base",
    };

    const computedClass = `${baseStyles} ${isManuscript ? "" : baseStylesDefault} ${variantStyles[variant as keyof typeof variantStyles]} ${isManuscript ? "" : sizeStyles[size as keyof typeof sizeStyles]} ${className}`;
</script>

<button {type} class={computedClass} {disabled} {onclick}>
    {@render children()}
</button>
