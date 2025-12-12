<script lang="ts">
    import type { Snippet } from "svelte";

    type Variant =
        | "muted"
        | "success"
        | "error"
        | "warning"
        | "info"
        | "primary";

    let {
        variant = "muted",
        outline = false,
        uppercase = false,
        class: className = "",
        children,
    } = $props<{
        variant?: Variant;
        outline?: boolean;
        uppercase?: boolean;
        class?: string;
        children: Snippet;
    }>();

    const baseStyles =
        "inline-flex w-fit items-center gap-1 px-2 py-0.5 text-[10px] font-normal font-mono rounded-full whitespace-nowrap";
    const uppercaseStyles = "tracking-wide";

    function getVariantStyles(v: Variant, isOutline: boolean): string {
        if (isOutline) {
            const outlineStyles: Record<Variant, string> = {
                muted: "border border-border text-foreground-muted bg-transparent",
                success: "border border-success text-success bg-transparent",
                error: "border border-error text-error bg-transparent",
                warning: "border border-warning text-warning bg-transparent",
                info: "border border-info text-info bg-transparent",
                primary: "border border-primary text-primary bg-transparent",
            };
            return outlineStyles[v];
        }
        const fillStyles: Record<Variant, string> = {
            muted: "bg-surface-elevated text-foreground-muted",
            success: "bg-success-subtle text-success",
            error: "bg-error-subtle text-error",
            warning: "bg-warning-subtle text-warning",
            info: "bg-info-subtle text-info",
            primary: "bg-primary/15 text-primary",
        };
        return fillStyles[v];
    }

    const computedClass = $derived(
        `${baseStyles} ${getVariantStyles(variant, outline)} ${uppercase ? uppercaseStyles : ""} ${className}`.trim(),
    );
</script>

<span class={computedClass}>
    {@render children()}
</span>
