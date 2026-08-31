<!--
  This device — the iOS/iPadOS half of `devices/this`, the way DeviceView is
  the macOS half.

  Exists because MobileDeviceScreen used not to be route-driven at all: it
  rendered on a `menuView` flag inside the phone's More tab. So when Devices
  grew a page per row, a phone had two doors to itself — the menu's rich local
  panel, and the box-side page, which closed by saying the live detail "can
  only be seen there" while *there* was the device in your hand. That sentence
  was true from a Mac and a lie on the phone.

  Both doors now land here. The More tab's "This device" keeps its promoted
  position — on a phone the device IS the sensor, so its health is the first
  question, not a settings detail — but it is a route push like every other
  entry beside it, and the `menuView` toggle it needed is gone.

  Page-wrapped rather than bare because MobileDeviceScreen carries no gutter of
  its own — the menu's `.body` supplied `0 16px`, and `padding="compact"` is the
  same measure.
-->
<script lang="ts">
	import { Page } from '$lib';
	import MobileDeviceScreen from '$lib/components/mobile/MobileDeviceScreen.svelte';
</script>

<!-- No "← Devices" chrome: on the phone this page is entered from the drawer,
     not the Devices list, so the link pointed somewhere the reader never was.
     Swipe-back / the drawer are the ways out. -->
<Page
	title="This device"
	description="What this phone collects, and whether it is reaching your server."
	padding="compact"
>
	<MobileDeviceScreen />
</Page>
