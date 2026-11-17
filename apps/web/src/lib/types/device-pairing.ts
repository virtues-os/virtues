/**
 * Device pairing types for the "+ Add Device" sheet (DevicePairModal), as
 * `initiatePairing` and `getPairingStatus` in `$lib/api/client` shape them
 * from `/api/pair/mint` and `/api/pair/status/:id`.
 */

// Device information structure
export interface DeviceInfo {
	device_id: string;
	device_name: string;
	device_model: string;
	os_version: string;
	app_version?: string;
	/** IANA timezone of this device at pairing time (e.g. "America/Chicago").
	 *  Used by the box as a cross-check for home_timezone when its own system
	 *  clock reads UTC (cloud deploys). See agents/record/timezone-model.md. */
	timezone?: string;
}

// Response when initiating device pairing. Backed by the unified
// `/api/pair/mint` flow: `source_id` carries the pair-token id (polled via
// `/api/pair/status/:id`), and `token` is the code the new device types.
export interface PairingInitResponse {
	source_id: string;
	/** Raw pair token, the code the person types into the new device. */
	token?: string;
	/** The server's `/pair#t=<token>` URL. The sheet shows only its origin,
	 *  as the address to type when this computer has no pairing door. */
	pair_url?: string;
}

// Pairing status variants
export interface PairingStatusPending {
	status: 'pending';
}

export interface PairingStatusActive {
	status: 'active';
	device_info: DeviceInfo;
}

export interface PairingStatusRevoked {
	status: 'revoked';
}

export type PairingStatus = PairingStatusPending | PairingStatusActive | PairingStatusRevoked;
