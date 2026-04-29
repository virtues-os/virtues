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
 * Strava OAuth proxy route. Same contract as Google.
 * Strava returns `expires_at` (unix seconds) and rotates refresh_token on
 * each refresh call.
 */

interface StravaTokenResponse {
  access_token: string;
  refresh_token?: string;
  expires_at?: number;
  expires_in?: number;
  token_type?: string;
  athlete?: unknown;
}

const router: Router = express.Router();

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

    const config = oauthConfigs.strava;
    const proxyState = Buffer.from(
      JSON.stringify({ return_url, rust_state: rustState }),
    ).toString('base64');

    const authUrl = new URL(config.authUrl);
    authUrl.searchParams.set('client_id', config.clientId);
    authUrl.searchParams.set('redirect_uri', config.redirectUri);
    authUrl.searchParams.set('scope', config.scopes.join(' '));
    authUrl.searchParams.set('response_type', 'code');
    authUrl.searchParams.set('approval_prompt', 'auto');
    authUrl.searchParams.set('state', proxyState);
    res.redirect(authUrl.toString());
  } catch (error: any) {
    console.error('Strava /start error:', error);
    res
      .status(error.statusCode || 500)
      .json({ error: error.message || 'Failed to initiate Strava OAuth' });
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
      source_id: 'strava',
      secrets: {
        access_token: tokens.access_token,
        refresh_token: tokens.refresh_token,
        expires_at: tokens.expires_at,
      },
      metadata: { athlete: tokens.athlete },
      expires_in: tokens.expires_in ?? null,
      scopes: null,
    });

    const ret = new URL(return_url);
    ret.searchParams.set('state', rust_state || '');
    ret.searchParams.set('exchange_token', exchangeToken);
    res.redirect(ret.toString());
  } catch (error: any) {
    console.error('Strava /callback error:', error);
    if (return_url && isValidReturnUrl(return_url)) {
      const ret = new URL(return_url);
      ret.searchParams.set('state', rust_state || '');
      ret.searchParams.set('error', 'token_exchange_failed');
      res.redirect(ret.toString());
    } else {
      res
        .status(error.statusCode || 500)
        .json({ error: error.message || 'Strava callback failed' });
    }
  }
});

router.post('/exchange/:token', (req: Request, res: Response) => {
  try {
    const payload = verifyExchangeToken(req.params.token, 'strava');
    const out: NormalizedExchangeResponse = normalize(payload);
    res.json(out);
  } catch (error: any) {
    console.error('Strava /exchange error:', error);
    res.status(400).json({ error: error.message || 'invalid exchange_token' });
  }
});

router.post('/refresh', async (req: Request, res: Response) => {
  try {
    const { refresh_token } = req.body;
    if (!refresh_token) throw createError('Missing refresh_token', 400);

    const config = oauthConfigs.strava;
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

    const tokens = (await response.json()) as StravaTokenResponse;
    if (!tokens.access_token) {
      throw createError('No access_token in refresh response', 502);
    }

    const out: NormalizedExchangeResponse = {
      secrets: {
        access_token: tokens.access_token,
        refresh_token: tokens.refresh_token || refresh_token,
        expires_at: tokens.expires_at,
      },
      metadata: {},
      expires_in: tokens.expires_in ?? 21600,
      scopes: null,
    };
    res.json(out);
  } catch (error: any) {
    console.error('Strava /refresh error:', error);
    res
      .status(error.statusCode || 500)
      .json({ error: error.message || 'Refresh failed' });
  }
});

async function exchangeCodeForTokens(code: string): Promise<StravaTokenResponse> {
  const config = oauthConfigs.strava;
  const body = new URLSearchParams({
    code,
    client_id: config.clientId,
    client_secret: config.clientSecret,
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
  const tokens = (await response.json()) as StravaTokenResponse;
  if (!tokens.access_token) throw new Error('No access_token from Strava');
  return tokens;
}

export { router as stravaRouter };
