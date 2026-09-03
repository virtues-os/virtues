<!--
  Plan — what you are on, what is left, and what it went to.

  Billing and Usage were two nav rows answering one question. "What am I
  paying" and "what am I using" are the same subject seen from two ends, and
  splitting them meant the balance was on one page while the calls that drew
  it down were on another — so neither page could answer "is that number
  going where I think it is?", which is the only reason anyone opens either.

  Order is deliberate: what you are ON, then what is LEFT, then what SPENT it.
  A ledger reads downward from the commitment to its consequences.
-->
<script lang="ts">
	import type { Tab } from "$lib/tabs/types";
	import { Page } from "$lib";
	import BillingView from "./BillingView.svelte";
	import UsageView from "./UsageView.svelte";

	let { tab, active }: { tab: Tab; active: boolean } = $props();
</script>

<Page
	title="Plan"
	description="Your subscription, your balance, and what has been drawing it down."
	maxWidth="wide"
>
	<BillingView {tab} {active} />
	<UsageView />
</Page>
