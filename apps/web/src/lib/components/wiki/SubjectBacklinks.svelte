<script lang="ts">
	/**
	 * "Mentioned in" — every piece of prose in the record that links here.
	 *
	 * This is the half that makes the wiki a wiki rather than a set of generated
	 * report pages. Forward links already existed: the day narrator is handed an
	 * allowlist of entities and copies their exact markdown, so
	 * `[Maya](/person/person_ab12)` has been in day prose for a while. Nothing
	 * ever read those links back, so the traffic was one-way and the graph was
	 * invisible from the side that would have made it navigable.
	 *
	 * The edge points at a SUBJECT, never at an article. Articles are opt-in, so
	 * most subjects will never have prose of their own — and a mention of
	 * someone with no article is exactly the case that matters, because this
	 * section is the only place it would ever surface.
	 *
	 * Renders nothing when there are no mentions. An empty "Mentioned in 0" on
	 * 573 entity pages is a heading that means "we looked", which is not worth a
	 * line of anyone's attention.
	 */
	import { onMount } from 'svelte';
	import WikiCollapsibleSection from './WikiCollapsibleSection.svelte';
	import { getSubjectBacklinks, type SubjectBacklink } from '$lib/wiki/api';

	interface Props {
		subjectType: 'person' | 'place' | 'organization' | 'day';
		subjectId: string;
	}

	let { subjectType, subjectId }: Props = $props();

	let links = $state<SubjectBacklink[]>([]);
	let loaded = $state(false);

	onMount(async () => {
		try {
			links = await getSubjectBacklinks(subjectType, subjectId);
		} catch {
			// A failed backlink read must not take the entity page with it: the
			// record below is the more important half.
			links = [];
		} finally {
			loaded = true;
		}
	});
</script>

{#if loaded && links.length > 0}
	<section class="section" id="mentioned-in">
		<WikiCollapsibleSection title="Mentioned in" count={links.length}>
			<ul class="mentions">
				{#each links as link (link.page_id)}
					<li>
						<a href={link.route} class="mention">{link.title}</a>
					</li>
				{/each}
			</ul>
		</WikiCollapsibleSection>
	</section>
{/if}

<style>
	@reference "../../../app.css";

	.mentions {
		list-style: none;
		margin: 0;
		padding: 0;
	}

	.mentions li + li {
		margin-top: 0.25rem;
	}

	.mention {
		font-size: 0.875rem;
		color: var(--color-foreground);
		text-decoration: none;
	}

	.mention:hover {
		text-decoration: underline;
		text-underline-offset: 2px;
	}
</style>
