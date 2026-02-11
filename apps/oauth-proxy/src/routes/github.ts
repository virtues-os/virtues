import express, { Router, Request, Response } from 'express';
import { oauthConfigs } from '../config/oauth-apps';
import { createError } from '../middleware/error-handler';
import { isValidReturnUrl } from '../utils/url-validator';

const router: Router = express.Router();

// Generate state parameter for CSRF protection
const generateState = () => {
  return Math.random().toString(36).substring(2, 15) +
         Math.random().toString(36).substring(2, 15);
};

// Initiate GitHub OAuth flow
router.get('/auth', (req: Request, res: Response) => {
  try {
    const { return_url, state: originalState } = req.query;

    if (!return_url || typeof return_url !== 'string') {
      throw createError('Missing return_url parameter', 400);
    }

    // Validate return_url to prevent open redirect attacks
    if (!isValidReturnUrl(return_url)) {
      throw createError('Invalid return_url parameter', 400);
    }

    const state = generateState();
    const config = oauthConfigs.github;

    // Store state and return_url (in production, use Redis or similar)
    // For now, encode in state parameter
    const stateData = {
      state: originalState || state,
      return_url,
      timestamp: Date.now()
    };

    const encodedState = Buffer.from(JSON.stringify(stateData)).toString('base64');

    const authUrl = new URL(config.authUrl);
    authUrl.searchParams.set('client_id', config.clientId);
    authUrl.searchParams.set('redirect_uri', config.redirectUri);
    authUrl.searchParams.set('scope', config.scopes.join(' '));
    authUrl.searchParams.set('state', encodedState);

    res.redirect(authUrl.toString());

  } catch (error) {
    console.error('GitHub auth error:', error);
    res.status(500).json({ error: 'Failed to initiate GitHub OAuth' });
  }
});

// Handle GitHub OAuth callback
router.get('/callback', async (req: Request, res: Response) => {
  try {
    const { code, state, error } = req.query;

    if (error) {
      throw createError(`OAuth error: ${error}`, 400);
    }

    if (!code || !state) {
      throw createError('Missing code or state parameter', 400);
    }

    // Decode state to get return_url and original state
    const stateData = JSON.parse(Buffer.from(state as string, 'base64').toString());
    const { return_url, state: originalState } = stateData;

    if (!return_url) {
      throw createError('Invalid state parameter', 400);
    }

    // Validate return_url again
    if (!isValidReturnUrl(return_url)) {
      throw createError('Invalid return_url in state', 400);
    }

    // Exchange code for tokens
    const tokens = await exchangeCodeForTokens(code as string);

    // Redirect back to user's instance with the tokens
    const returnUrl = new URL(return_url);
    returnUrl.searchParams.set('access_token', tokens.access_token);
    if (tokens.refresh_token) {
      returnUrl.searchParams.set('refresh_token', tokens.refresh_token);
    }
    // GitHub tokens don't expire by default, but include scope info
    if (tokens.scope) {
      returnUrl.searchParams.set('scope', tokens.scope);
    }
    returnUrl.searchParams.set('provider', 'github');
    if (originalState) {
      returnUrl.searchParams.set('state', originalState);
    }

    res.redirect(returnUrl.toString());

  } catch (error) {
    console.error('GitHub callback error:', error);

    // Redirect to user's instance with error
    try {
      const stateData = JSON.parse(Buffer.from(req.query.state as string, 'base64').toString());
      const returnUrl = new URL(stateData.return_url);
      returnUrl.searchParams.set('error', 'token_exchange_failed');
      res.redirect(returnUrl.toString());
    } catch {
      res.status(500).json({ error: 'Failed to process GitHub OAuth callback' });
    }
  }
});

// Exchange authorization code for tokens
async function exchangeCodeForTokens(code: string) {
  const config = oauthConfigs.github;

  const body = new URLSearchParams({
    code,
    client_id: config.clientId,
    client_secret: config.clientSecret
  });

  // GitHub requires Accept: application/json to get JSON response
  const response = await fetch(config.tokenUrl, {
    method: 'POST',
    headers: {
      'Content-Type': 'application/x-www-form-urlencoded',
      'Accept': 'application/json'
    },
    body: body.toString()
  });

  if (!response.ok) {
    const errorData = await response.text();
    throw new Error(`Token exchange failed: ${response.status} ${errorData}`);
  }

  const tokens = await response.json();

  if (!tokens.access_token) {
    throw new Error('No access token received');
  }

  return tokens;
}

// Refresh endpoint (GitHub tokens don't expire by default, but provided for consistency)
router.post('/refresh', async (req: Request, res: Response) => {
  try {
    const { refresh_token } = req.body;

    if (!refresh_token) {
      // GitHub tokens typically don't expire, return appropriate message
      throw createError('GitHub tokens do not expire by default. Re-authorize if token is revoked.', 400);
    }

    // If a refresh token is provided (GitHub Apps can have expiring tokens),
    // attempt to refresh it
    const config = oauthConfigs.github;

    const body = new URLSearchParams({
      refresh_token,
      client_id: config.clientId,
      client_secret: config.clientSecret,
      grant_type: 'refresh_token'
    });

    const response = await fetch(config.tokenUrl, {
      method: 'POST',
      headers: {
        'Content-Type': 'application/x-www-form-urlencoded',
        'Accept': 'application/json'
      },
      body: body.toString()
    });

    if (!response.ok) {
      const errorData = await response.text();
      console.error('Token refresh failed:', response.status, errorData);
      throw createError(`Token refresh failed: ${response.status}`, response.status);
    }

    const tokens: any = await response.json();

    res.json({
      access_token: tokens.access_token,
      refresh_token: tokens.refresh_token || refresh_token,
      expires_in: tokens.expires_in,
      token_type: tokens.token_type || 'Bearer'
    });

  } catch (error: any) {
    console.error('Token refresh error:', error);

    if (error.statusCode) {
      res.status(error.statusCode).json({
        error: error.message,
        code: 'refresh_failed'
      });
    } else {
      res.status(500).json({
        error: 'Failed to refresh token',
        code: 'refresh_failed'
      });
    }
  }
});

export { router as githubRouter };
