import express, { Router, Request, Response } from 'express';
import { oauthConfigs } from '../config/oauth-apps';
import { createError } from '../middleware/error-handler';
import { isValidReturnUrl } from '../utils/url-validator';
import {
  signExchangeToken,
  verifyExchangeToken,
  normalize,
  NormalizedExchangeResponse,
} from '../utils/exchange-token';

/**
 * Google OAuth proxy route.
 *
 * Contract (matches `crates/virtues-helpers/src/auth/proxy.rs`):
 *   GET  /google/start              — kick off the dance
 *   GET  /google/callback           — Google redirects here; we exchange,
 *                                      sign an exchange_token, and bounce
 *                                      back to the user's instance
 *   POST /google/exchange/:token    — Virtues server fetches secrets here
 *   POST /google/refresh            — refresh access_token using refresh_token
 */

const router: Router = express.Router();

interface GoogleTokenResponse {
  access_token: string;
  refresh_token?: string;
  expires_in?: number;
  scope?: string;
  token_type?: string;
  id_token?: string;
}

router.get('/start', (req: Request, res: Response) => {
  try {
    const { return_url, state: rustState } = req.query;

    if (!return_url || typeof return_url !== 'string') {
      throw createError('Missing return_url parameter', 400);
    }
    if (!isValidReturnUrl(return_url)) {
      throw createError('Invalid return_url parameter', 400);
    }
    if (!rustState || typeof rustState !== 'string') {
      throw createError('Missing state parameter', 400);
    }

    const config = oauthConfigs.google;
    const proxyState = Buffer.from(
      JSON.stringify({ return_url, rust_state: rustState }),
    ).toString('base64');

    const authUrl = new URL(config.authUrl);
    authUrl.searchParams.set('client_id', config.clientId);
    authUrl.searchParams.set('redirect_uri', config.redirectUri);
    authUrl.searchParams.set('scope', config.scopes.join(' '));
    authUrl.searchParams.set('response_type', 'code');
    authUrl.searchParams.set('access_type', 'offline');
    authUrl.searchParams.set('prompt', 'consent');
    authUrl.searchParams.set('state', proxyState);

    res.redirect(authUrl.toString());
  } catch (error: any) {
    console.error('Google /start error:', error);
    res
      .status(error.statusCode || 500)
      .json({ error: error.message || 'Failed to initiate Google OAuth' });
  }
});

router.get('/callback', async (req: Request, res: Response) => {
  let return_url: string | undefined;
  let rust_state: string | undefined;

  try {
    const { code, state, error } = req.query;
    if (error) throw createError(`OAuth error: ${error}`, 400);
    if (!code || !state) throw createError('Missing code or state', 400);

    const decoded = JSON.parse(
      Buffer.from(state as string, 'base64').toString(),
    );
    return_url = decoded.return_url;
    rust_state = decoded.rust_state;

    if (!return_url || !isValidReturnUrl(return_url)) {
      throw createError('Invalid return_url in state', 400);
    }

    const tokens = await exchangeCodeForTokens(code as string);

    const exchangeToken = signExchangeToken({
      source_id: 'google',
      secrets: {
        access_token: tokens.access_token,
        refresh_token: tokens.refresh_token,
      },
      metadata: {
        // Google sends `scope` as space-separated; expose for debugging.
        granted_scopes: tokens.scope,
      },
      expires_in: tokens.expires_in ?? null,
      scopes: tokens.scope ? tokens.scope.split(' ') : null,
    });

    const ret = new URL(return_url);
    ret.searchParams.set('state', rust_state || '');
    ret.searchParams.set('exchange_token', exchangeToken);
    res.redirect(ret.toString());
  } catch (error: any) {
    console.error('Google /callback error:', error);
    if (return_url && isValidReturnUrl(return_url)) {
      const ret = new URL(return_url);
      ret.searchParams.set('state', rust_state || '');
      ret.searchParams.set('error', 'token_exchange_failed');
      res.redirect(ret.toString());
    } else {
      res
        .status(error.statusCode || 500)
        .json({ error: error.message || 'Google callback failed' });
    }
  }
});

router.post('/exchange/:token', (req: Request, res: Response) => {
  try {
    const payload = verifyExchangeToken(req.params.token, 'google');
    const out: NormalizedExchangeResponse = normalize(payload);
    res.json(out);
  } catch (error: any) {
    console.error('Google /exchange error:', error);
    res.status(400).json({ error: error.message || 'invalid exchange_token' });
  }
});

router.post('/refresh', async (req: Request, res: Response) => {
  try {
    const { refresh_token } = req.body;
    if (!refresh_token) throw createError('Missing refresh_token', 400);

    const config = oauthConfigs.google;
    const body = new URLSearchParams({
      refresh_token,
      client_id: config.clientId,
      client_secret: config.clientSecret,
      grant_type: 'refresh_token',
    });
    const response = await fetch(config.tokenUrl, {
      method: 'POST',
      headers: { 'Content-Type': 'application/x-www-form-urlencoded' },
      body: body.toString(),
    });

    if (!response.ok) {
      const errBody = await response.text();
      if (errBody.includes('invalid_grant')) {
        throw createError('Refresh token invalid or expired', 401);
      }
      throw createError(`Refresh failed: ${response.status}`, response.status);
    }

    const tokens = (await response.json()) as GoogleTokenResponse;
    if (!tokens.access_token) {
      throw createError('No access_token in refresh response', 502);
    }

    const out: NormalizedExchangeResponse = {
      secrets: {
        access_token: tokens.access_token,
        // Google may not return a new refresh_token — preserve old one.
        refresh_token: tokens.refresh_token || refresh_token,
      },
      metadata: { granted_scopes: tokens.scope },
      expires_in: tokens.expires_in ?? null,
      scopes: tokens.scope ? tokens.scope.split(' ') : null,
    };
    res.json(out);
  } catch (error: any) {
    console.error('Google /refresh error:', error);
    res
      .status(error.statusCode || 500)
      .json({ error: error.message || 'Refresh failed' });
  }
});

async function exchangeCodeForTokens(code: string): Promise<GoogleTokenResponse> {
  const config = oauthConfigs.google;
  const body = new URLSearchParams({
    code,
    client_id: config.clientId,
    client_secret: config.clientSecret,
    redirect_uri: config.redirectUri,
    grant_type: 'authorization_code',
  });

  const response = await fetch(config.tokenUrl, {
    method: 'POST',
    headers: { 'Content-Type': 'application/x-www-form-urlencoded' },
    body: body.toString(),
  });

  if (!response.ok) {
    const errBody = await response.text();
    throw new Error(`Token exchange failed: ${response.status} ${errBody}`);
  }
  const tokens = (await response.json()) as GoogleTokenResponse;
  if (!tokens.access_token) throw new Error('No access_token from Google');
  return tokens;
}

export { router as googleRouter };
