<script lang="ts">
	/**
	 * PlaidConnectModal - Wraps PlaidLink in a modal for banking connections.
	 * Plaid Link opens as its own overlay, so this modal provides context.
	 */
	import Modal from "$lib/components/Modal.svelte";
	import PlaidLink from "$lib/components/PlaidLink.svelte";

	interface Props {
		open: boolean;
		onClose: () => void;
		onSuccess: (sourceId: string, institutionName?: string) => void;
	}

	let { open, onClose, onSuccess }: Props = $props();

	function handleSuccess(sourceId: string, institutionName?: string) {
		onSuccess(sourceId, institutionName);
		onClose();
	}

	function handleCancel() {
		onClose();
	}
</script>

<Modal {open} {onClose} title="Connect Bank Account" width="md">
	<PlaidLink 
		onSuccess={handleSuccess}
		onCancel={handleCancel}
	/>
</Modal>
