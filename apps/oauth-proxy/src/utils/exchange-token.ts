/**
 * Exchange-token signing.
 *
 * After a successful provider callback (Google/Notion/Strava OAuth, Plaid Link),
 * the proxy signs the normalized `{secrets, metadata, expires_in, scopes}`
 * payload as an HMAC-SHA256 token and redirects the user's browser back to
 * their Virtues instance with `?exchange_token=<token>`.
 *
 * The Virtues `oauth_callback` handler then POSTs to
 * `{proxy}/{source}/exchange/{token}` to retrieve the payload server-side.
 *
 * Why not put tokens in the URL? Tokens in browser history / referrer headers
 * leak. The exchange_token is short-lived (5 min) and only meaningful to the
 * proxy; the actual secrets are pulled over a server-to-server POST.
 *
 * Why HMAC instead of a stateful map? Vercel serverless: no shared memory.
 * Stateless JWT-style signing means any proxy instance can verify any token.
 */

import { createHmac, timingSafeEqual } from 'crypto';

const TTL_SECONDS = 5 * 60;

interface ExchangeTokenPayload {
  source_id: string;
  secrets: Record<string, unknown>;
  metadata?: Record<string, unknown>;
  expires_in?: number | null;
  scopes?: string[] | null;
}

interface SignedClaims extends ExchangeTokenPayload {
  iat: number;
  exp: number;
}

function getSecret(): string {
  const s = process.env.OAUTH_PROXY_EXCHANGE_SECRET;
  if (!s || s.length < 32) {
    throw new Error(
      'OAUTH_PROXY_EXCHANGE_SECRET env var must be set to a >=32 char value',
    );
  }
  return s;
}

function b64urlEncode(buf: Buffer): string {
  return buf
    .toString('base64')
    .replace(/\+/g, '-')
    .replace(/\//g, '_')
    .replace(/=+$/, '');
}

function b64urlDecode(s: string): Buffer {
  const pad = s.length % 4 === 0 ? '' : '='.repeat(4 - (s.length % 4));
  return Buffer.from(s.replace(/-/g, '+').replace(/_/g, '/') + pad, 'base64');
}

export function signExchangeToken(payload: ExchangeTokenPayload): string {
  const now = Math.floor(Date.now() / 1000);
  const claims: SignedClaims = {
    ...payload,
    iat: now,
    exp: now + TTL_SECONDS,
  };

  const body = b64urlEncode(Buffer.from(JSON.stringify(claims), 'utf8'));
  const sig = createHmac('sha256', getSecret()).update(body).digest();
  return `${body}.${b64urlEncode(sig)}`;
}

export function verifyExchangeToken(
  token: string,
  expectedSourceId: string,
): ExchangeTokenPayload {
  const parts = token.split('.');
  if (parts.length !== 2) {
    throw new Error('malformed exchange_token');
  }
  const [body, sigB64] = parts;

  const expected = createHmac('sha256', getSecret()).update(body).digest();
  const provided = b64urlDecode(sigB64);
  if (
    expected.length !== provided.length ||
    !timingSafeEqual(expected, provided)
  ) {
    throw new Error('exchange_token signature mismatch');
  }

  const claims = JSON.parse(b64urlDecode(body).toString('utf8')) as SignedClaims;
  const now = Math.floor(Date.now() / 1000);
  if (claims.exp < now) {
    throw new Error('exchange_token expired');
  }
  if (claims.source_id !== expectedSourceId) {
    throw new Error(
      `exchange_token source mismatch: token=${claims.source_id} expected=${expectedSourceId}`,
    );
  }

  return {
    source_id: claims.source_id,
    secrets: claims.secrets,
    metadata: claims.metadata,
    expires_in: claims.expires_in,
    scopes: claims.scopes,
  };
}

/**
 * Shared response shape for both /exchange/:token and /refresh. Mirrors
 * `ProxyExchangeResponse` in `crates/virtues-helpers/src/auth/proxy.rs`.
 */
export interface NormalizedExchangeResponse {
  secrets: Record<string, unknown>;
  metadata: Record<string, unknown>;
  expires_in: number | null;
  scopes: string[] | null;
}

export function normalize(
  payload: ExchangeTokenPayload,
): NormalizedExchangeResponse {
  return {
    secrets: payload.secrets,
    metadata: payload.metadata ?? {},
    expires_in: payload.expires_in ?? null,
    scopes: payload.scopes ?? null,
  };
}
