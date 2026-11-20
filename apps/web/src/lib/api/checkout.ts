/**
 * Opening a subscription checkout from the app.
 *
 * A checkout always belongs to the account this server is linked to: the
 * server asks atlas with its own key, and atlas binds the Stripe session to
 * that account. So a server that is not linked yet has to sign in first
 * (an emailed link, which also creates the account for a new address), and
 * only then can it pay. The email typed at Stripe never chooses an account.
 *
 * Shared by Setup's subscription step and the getting-started room, which
 * offer the same Subscribe button.
 */
import { subscribeBilling } from "./client";

export type CheckoutStart =
	| { kind: "opened"; url: string }
	/** The server has no account yet: sign in, then try again. */
	| { kind: "needs_signin" }
	| { kind: "already_subscribed" }
	| { kind: "error"; message: string };

/** Ask the server for a checkout URL. The caller opens it. */
export async function startCheckout(): Promise<CheckoutStart> {
	const d = await subscribeBilling<{ url?: string; error?: string; code?: string }>();
	if (d.url) return { kind: "opened", url: d.url };
	if (d.code === "not_linked") return { kind: "needs_signin" };
	if (d.code === "already_subscribed") return { kind: "already_subscribed" };
	return { kind: "error", message: d.error || "Couldn't open checkout. Try again." };
}
