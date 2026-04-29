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
 * Notion OAuth proxy route. Same contract as Google.
 * Notion access tokens don't expire and have no refresh_token — /refresh is
 * a no-op that returns the existing access_token unchanged.
 */

interface NotionTokenResponse {
  access_token: string;
  workspace_id?: string;
  workspace_name?: string;
  workspace_icon?: string;
  bot_id?: string;
  owner?: unknown;
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

    const config = oauthConfigs.notion;
    const proxyState = Buffer.from(
      JSON.stringify({ return_url, rust_state: rustState }),
    ).toString('base64');

    const params = new URLSearchParams({
      client_id: config.clientId,
      redirect_uri: config.redirectUri,
      response_type: 'code',
      state: proxyState,
      owner: 'user',
    });
    res.redirect(`${config.authUrl}?${params.toString()}`);
  } catch (error: any) {
    console.error('Notion /start error:', error);
    res
      .status(error.statusCode || 500)
      .json({ error: error.message || 'Failed to initiate Notion OAuth' });
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

    const config = oauthConfigs.notion;
    const tokenResp = await fetch(config.tokenUrl, {
      method: 'POST',
      headers: {
        'Content-Type': 'application/x-www-form-urlencoded',
        Authorization: `Basic ${Buffer.from(
          `${config.clientId}:${config.clientSecret}`,
        ).toString('base64')}`,
      },
      body: new URLSearchParams({
        grant_type: 'authorization_code',
        code: code as string,
        redirect_uri: config.redirectUri,
      }).toString(),
    });

    if (!tokenResp.ok) {
      const errBody = await tokenResp.text();
      throw createError(
        `Notion token exchange failed: ${tokenResp.status} ${errBody}`,
        502,
      );
    }
    const tokens = (await tokenResp.json()) as NotionTokenResponse;

    const exchangeToken = signExchangeToken({
      source_id: 'notion',
      secrets: {
        access_token: tokens.access_token,
        bot_id: tokens.bot_id,
        workspace_id: tokens.workspace_id,
      },
      metadata: {
        workspace_name: tokens.workspace_name,
        workspace_icon: tokens.workspace_icon,
        owner: tokens.owner,
      },
      expires_in: null,
      scopes: null,
    });

    const ret = new URL(return_url);
    ret.searchParams.set('state', rust_state || '');
    ret.searchParams.set('exchange_token', exchangeToken);
    res.redirect(ret.toString());
  } catch (error: any) {
    console.error('Notion /callback error:', error);
    if (return_url && isValidReturnUrl(return_url)) {
      const ret = new URL(return_url);
      ret.searchParams.set('state', rust_state || '');
      ret.searchParams.set('error', 'token_exchange_failed');
      res.redirect(ret.toString());
    } else {
      res
        .status(error.statusCode || 500)
        .json({ error: error.message || 'Notion callback failed' });
    }
  }
});

router.post('/exchange/:token', (req: Request, res: Response) => {
  try {
    const payload = verifyExchangeToken(req.params.token, 'notion');
    const out: NormalizedExchangeResponse = normalize(payload);
    res.json(out);
  } catch (error: any) {
    console.error('Notion /exchange error:', error);
    res.status(400).json({ error: error.message || 'invalid exchange_token' });
  }
});

/**
 * Notion access tokens don't expire. Return the same token wrapped in the
 * normalized shape so credential_refresh cron is a no-op success.
 */
router.post('/refresh', (req: Request, res: Response) => {
  const { refresh_token } = req.body;
  if (!refresh_token) {
    return res.status(400).json({ error: 'Missing refresh_token' });
  }
  // For Notion the "refresh_token" we hold IS the access_token (no refresh
  // model). Echo it back unchanged in the canonical shape.
  const out: NormalizedExchangeResponse = {
    secrets: { access_token: refresh_token },
    metadata: {},
    expires_in: null,
    scopes: null,
  };
  res.json(out);
});

export default router;
