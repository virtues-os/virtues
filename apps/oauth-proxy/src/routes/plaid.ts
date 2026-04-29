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
 * Plaid proxy route.
 *
 * Plaid is **not OAuth** — it's a Hosted Link flow. The user clicks Connect,
 * we mint a `link_token` server-side, the browser opens Plaid's hosted UI,
 * the user picks a bank, and Plaid hands the browser a `public_token` which
 * we exchange server-side for a permanent `access_token` + `item_id`.
 *
 * Same contract surface as the OAuth providers:
 *   GET  /plaid/start                — accepts return_url + state, mints
 *                                      link_token, redirects to Hosted Link
 *   GET  /plaid/callback             — receives public_token, exchanges for
 *                                      access_token, signs exchange_token,
 *                                      bounces back to user instance
 *   POST /plaid/exchange/:token      — Virtues server fetches secrets here
 *   POST /plaid/refresh              — no-op; Plaid tokens are permanent
 */

const router: Router = express.Router();

const PLAID_LINK_TOKEN_URL = 'https://production.plaid.com/link/token/create';
const PLAID_PUBLIC_TOKEN_EXCHANGE_URL =
  'https://production.plaid.com/item/public_token/exchange';

interface LinkTokenResponse {
  link_token: string;
  expiration: string;
}

interface PublicTokenExchangeResponse {
  access_token: string;
  item_id: string;
  request_id: string;
}

router.get('/start', async (req: Request, res: Response) => {
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

    const config = oauthConfigs.plaid;
    const linkResp = await fetch(PLAID_LINK_TOKEN_URL, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({
        client_id: config.clientId,
        secret: config.clientSecret,
        client_name: 'Virtues',
        user: { client_user_id: 'virtues-user' },
        products: ['transactions'],
        // Optional products: include if user later connects investment / loan
        // accounts. Plaid will skip if institution doesn't support them.
        optional_products: ['investments', 'liabilities'],
        country_codes: ['US'],
        language: 'en',
        redirect_uri: config.redirectUri,
      }),
    });

    if (!linkResp.ok) {
      const errBody = await linkResp.text();
      throw createError(
        `Plaid link_token failed: ${linkResp.status} ${errBody}`,
        502,
      );
    }
    const linkJson = (await linkResp.json()) as LinkTokenResponse;

    const proxyState = Buffer.from(
      JSON.stringify({ return_url, rust_state: rustState }),
    ).toString('base64');

    const hosted = new URL('https://cdn.plaid.com/link/v2/stable/link.html');
    hosted.searchParams.set('isWebview', 'true');
    hosted.searchParams.set('token', linkJson.link_token);
    hosted.searchParams.set('receivedRedirectUri', config.redirectUri);
    hosted.searchParams.set('state', proxyState);
    res.redirect(hosted.toString());
  } catch (error: any) {
    console.error('Plaid /start error:', error);
    res
      .status(error.statusCode || 500)
      .json({ error: error.message || 'Plaid start failed' });
  }
});

router.get('/callback', async (req: Request, res: Response) => {
  let return_url: string | undefined;
  let rust_state: string | undefined;
  try {
    const { public_token, state, error } = req.query;
    if (error) throw createError(`Plaid Link error: ${error}`, 400);
    if (!public_token || !state)
      throw createError('Missing public_token or state', 400);

    const decoded = JSON.parse(
      Buffer.from(state as string, 'base64').toString(),
    );
    return_url = decoded.return_url;
    rust_state = decoded.rust_state;
    if (!return_url || !isValidReturnUrl(return_url)) {
      throw createError('Invalid return_url in state', 400);
    }

    const config = oauthConfigs.plaid;
    const exchangeResp = await fetch(PLAID_PUBLIC_TOKEN_EXCHANGE_URL, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({
        client_id: config.clientId,
        secret: config.clientSecret,
        public_token,
      }),
    });

    if (!exchangeResp.ok) {
      const errBody = await exchangeResp.text();
      throw createError(
        `Plaid exchange failed: ${exchangeResp.status} ${errBody}`,
        502,
      );
    }
    const exchangeJson = (await exchangeResp.json()) as PublicTokenExchangeResponse;

    const exchangeToken = signExchangeToken({
      source_id: 'plaid',
      secrets: { access_token: exchangeJson.access_token },
      metadata: { item_id: exchangeJson.item_id },
      expires_in: null,
      scopes: null,
    });

    const ret = new URL(return_url);
    ret.searchParams.set('state', rust_state || '');
    ret.searchParams.set('exchange_token', exchangeToken);
    res.redirect(ret.toString());
  } catch (error: any) {
    console.error('Plaid /callback error:', error);
    if (return_url && isValidReturnUrl(return_url)) {
      const ret = new URL(return_url);
      ret.searchParams.set('state', rust_state || '');
      ret.searchParams.set('error', 'plaid_exchange_failed');
      res.redirect(ret.toString());
    } else {
      res
        .status(error.statusCode || 500)
        .json({ error: error.message || 'Plaid callback failed' });
    }
  }
});

router.post('/exchange/:token', (req: Request, res: Response) => {
  try {
    const payload = verifyExchangeToken(req.params.token, 'plaid');
    const out: NormalizedExchangeResponse = normalize(payload);
    res.json(out);
  } catch (error: any) {
    console.error('Plaid /exchange error:', error);
    res.status(400).json({ error: error.message || 'invalid exchange_token' });
  }
});

/**
 * Plaid access tokens never expire — the credential_refresh cron should not
 * actually call this for Plaid (we leave `next_refresh_at` null). If it does,
 * echo the token back unchanged in the canonical shape so the cron passes.
 */
router.post('/refresh', (req: Request, res: Response) => {
  const { refresh_token } = req.body;
  if (!refresh_token) {
    return res.status(400).json({ error: 'Missing refresh_token' });
  }
  const out: NormalizedExchangeResponse = {
    secrets: { access_token: refresh_token },
    metadata: {},
    expires_in: null,
    scopes: null,
  };
  res.json(out);
});

export { router as plaidRouter };
