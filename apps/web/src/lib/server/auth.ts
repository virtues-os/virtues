/**
 * Auth.js configuration for Virtues
 *
 * Single-user per tenant. Magic link authentication via Resend.
 * OWNER_EMAIL env var restricts who can log in (set by Atlas at provisioning).
 */
import { SvelteKitAuth } from '@auth/sveltekit';
import { Resend } from 'resend';
import { env } from '$env/dynamic/private';
import { createPostgresAdapter } from './auth-adapter';

/**
 * Validate email format with stricter rules
 * - Must have exactly one @
 * - Local part: letters, numbers, and common special chars (._+-%)
 * - Domain: letters, numbers, hyphens, with at least one dot
 * - TLD: at least 2 characters
 */
function isValidEmail(email: string): boolean {
	// More comprehensive email regex
	// Allows: letters, numbers, dots, hyphens, underscores, plus, percent in local part
	// Requires: valid domain format with TLD of 2+ chars
	const emailRegex = /^[a-zA-Z0-9._%+-]+@[a-zA-Z0-9]([a-zA-Z0-9-]*[a-zA-Z0-9])?(\.[a-zA-Z0-9]([a-zA-Z0-9-]*[a-zA-Z0-9])?)*\.[a-zA-Z]{2,}$/;

	if (!email || email.length > 254) {
		return false;
	}

	const [localPart] = email.split('@');
	if (localPart && localPart.length > 64) {
		return false;
	}

	return emailRegex.test(email);
}

const OWNER_EMAIL = env.OWNER_EMAIL;

if (!OWNER_EMAIL) {
	console.warn('[Auth] OWNER_EMAIL not set - authentication will be disabled in development');
} else if (!isValidEmail(OWNER_EMAIL)) {
	// Log internally but don't expose the email in error messages
	console.error(`[Auth] Invalid OWNER_EMAIL format detected`);
	throw new Error('Invalid OWNER_EMAIL configuration. Contact administrator.');
}

export const { handle, signIn, signOut } = SvelteKitAuth({
	trustHost: true,
	basePath: '/auth',
	adapter: createPostgresAdapter(),
	providers: [
		{
			id: 'resend',
			name: 'Email',
			type: 'email',
			from: env.EMAIL_FROM || 'Virtues <noreply@virtues.com>',
			maxAge: 24 * 60 * 60, // 24 hours
			async sendVerificationRequest({ identifier: email, url }) {
				// SECURITY: Check authorization BEFORE sending email to prevent enumeration
				// Always return success to UI regardless of authorization status
				if (OWNER_EMAIL && email.toLowerCase() !== OWNER_EMAIL.toLowerCase()) {
					// Log unauthorized attempt but don't send email (prevents enumeration)
					console.log(`[Auth] Unauthorized login attempt from: ${email}`);
					return; // Silent return - UI will still show "check your email"
				}

				// Dev mode: log magic link to console instead of sending email
				if (env.ENVIRONMENT === 'development') {
					console.log('\n========================================');
					console.log('[Auth Dev] Magic link (click to sign in):');
					console.log(url);
					console.log('========================================\n');
					return;
				}

				const resend = new Resend(env.RESEND_API_KEY);
				await resend.emails.send({
					from: env.EMAIL_FROM || 'Virtues <noreply@virtues.com>',
					to: email,
					subject: 'Sign in to Virtues',
					html: `
						<div style="font-family: sans-serif; max-width: 400px; margin: 0 auto; padding: 20px;">
							<h1 style="font-size: 24px; margin-bottom: 16px;">Sign in to Virtues</h1>
							<p style="color: #666; margin-bottom: 24px;">Click the button below to sign in. This link expires in 24 hours.</p>
							<a href="${url}" style="display: inline-block; background: #000; color: #fff; padding: 12px 24px; text-decoration: none; border-radius: 6px; font-weight: 500;">Sign in</a>
							<p style="color: #999; font-size: 12px; margin-top: 24px;">If you didn't request this email, you can safely ignore it.</p>
						</div>
					`
				});
			}
		}
	],
	pages: {
		signIn: '/login',
		error: '/login/error'
	},
	callbacks: {
		/**
		 * Restrict login to the owner email
		 */
		async signIn({ user }) {
			// In development without OWNER_EMAIL, allow any email
			if (!OWNER_EMAIL) {
				return true;
			}
			// Only allow the configured owner email (case-insensitive)
			return user.email?.toLowerCase() === OWNER_EMAIL.toLowerCase();
		},
		/**
		 * Include user ID in the session
		 */
		async session({ session, user }) {
			if (session.user) {
				session.user.id = user.id;
			}
			return session;
		}
	},
	session: {
		strategy: 'database',
		maxAge: 30 * 24 * 60 * 60 // 30 days
	}
});
