<!--
	MentionNode: a mention in a block page, an inline widget.

	Drawn by `Ref`: the hover preview, the live title of a page, chat or
	project, the words written for a person or place (record/names.md); a
	click opens it in the tab you are on and ⌘ or Ctrl-click in a new tab. A
	mention of a record is a citation instead: a click opens its RecordCard,
	which the page draws (DocumentEditor).
	`label` is the text written in the page, which is what the markdown
	export holds; `to` stays a route so redirects work. Right-click or a long
	press: Open in new tab, Open beside (`openMenuItems`), Copy link, Edit,
	Remove.
-->
<script lang="ts">
	import { onMount } from "svelte";
	import Ref from "$lib/components/Ref.svelte";
	import { onContextGesture } from "$lib/components/contextMenu/gesture";
	import { contextMenu } from "$lib/stores/contextMenu.svelte";
	import { linkEditor } from "$lib/stores/linkEditor.svelte";
	import { openMenuItems } from "$lib/utils/openMenuItems";
	import { isRoute } from "$lib/document/schema";
	import { shareableHref } from "$lib/document/links";
	import type { ViewState } from "$lib/document/nodeviews.svelte";

	let { view }: { view: ViewState } = $props();

	const label = $derived(String(view.attrs.label ?? ""));
	const to = $derived(String(view.attrs.to ?? ""));

	let el: HTMLSpanElement;

	function edit(x: number, y: number) {
		linkEditor.show(
			{ label, href: to },
			({ label: nextLabel, href }) => {
				if (isRoute(href)) {
					view.setAttrs({ label: nextLabel, to: href });
					return;
				}
				// An address elsewhere is a link, not a mention.
				const { schema } = view.editor;
				view.replaceWith(schema.text(nextLabel, [schema.marks.link.create({ href })]));
			},
			{ x, y, width: 0, height: 0 },
		);
	}

	onMount(() => {
		onContextGesture(el, (x, y) => {
			contextMenu.show({ x, y }, [
				...openMenuItems(to, label),
				{
					id: "copy-link",
					label: "Copy link",
					icon: "mynaui:copy",
					action: () => void navigator.clipboard?.writeText(shareableHref(to)),
				},
				...(view.editable
					? [
							{
								id: "edit",
								label: "Edit",
								icon: "mynaui:edit",
								dividerBefore: true,
								action: () => edit(x, y),
							},
							{
								id: "remove",
								label: "Remove",
								icon: "mynaui:trash",
								variant: "destructive" as const,
								action: () => view.remove(),
							},
						]
					: []),
			]);
		});
	});
</script>

<span bind:this={el} class="doc-mention" class:selected={view.selected}
	><Ref displayName={label} url={to} variant="quiet" /></span
>

<style>
	/* A browser draws Ref's button as one box: a mention shorter than the
	   column moves to the next line whole, and a longer one wraps inside it
	   rather than widen the page, which would take its title and text
	   sideways on a phone. */
	.doc-mention {
		overflow-wrap: anywhere;
	}

	.doc-mention.selected {
		outline: 2px solid var(--color-primary);
		outline-offset: 1px;
		border-radius: var(--radius-md);
	}
</style>
