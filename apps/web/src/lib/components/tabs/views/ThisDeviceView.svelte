<!--
  This device on the iPhone and iPad: the iOS half of `devices/this`, the way
  DeviceView is the desktop half. MobileDeviceScreen reads the phone's own
  plugins, which exist only on the phone: its streams, its link to the
  server, its sync queue and its notifications.

  The phone's drawer opens it, and the Settings landing page's This device
  line does too, so its back link goes to Settings, where that line is.
-->
<script lang="ts">
	import SettingsPage from '$lib/components/settings/SettingsPage.svelte';
	import MobileDeviceScreen from '$lib/components/mobile/MobileDeviceScreen.svelte';
	import { isIPad } from '$lib/utils/platform';
</script>

<SettingsPage
	title={isIPad ? 'This iPad' : 'This iPhone'}
	lede="What it sends, how it reaches your server, and the app."
>
	<MobileDeviceScreen />
</SettingsPage>
