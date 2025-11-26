import dotenv from 'dotenv';
dotenv.config(); // Load environment variables FIRST

import express, { Application, Request, Response } from 'express';
import cors from 'cors';
import helmet from 'helmet';
import rateLimit from 'express-rate-limit';

import { googleRouter } from './routes/google';
import notionRouter from './routes/notion';
import { stravaRouter } from './routes/strava';
import { errorHandler } from './middleware/error-handler';
import { logger } from './middleware/logger';

const app: Application = express();
const PORT = process.env.PORT || 3000;

// Trust proxy for Vercel/serverless environments
app.set('trust proxy', 1);

// Security middleware
app.use(helmet());
app.use(cors({
  origin: process.env.ALLOWED_ORIGINS?.split(',') || ['http://localhost:5173'],
  credentials: true
}));

// Rate limiting
const limiter = rateLimit({
  windowMs: 15 * 60 * 1000, // 15 minutes
  max: 100, // limit each IP to 100 requests per windowMs
  message: 'Too many requests from this IP, please try again later.'
});
app.use(limiter);

// Middleware
app.use(express.json());
app.use(logger);

// Homepage - redirect users who land here by mistake
app.get('/', (req: Request, res: Response) => {
  res.send(`
<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <title>Virtues OAuth</title>
  <style>
    * { margin: 0; padding: 0; box-sizing: border-box; }
    body {
      font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;
      min-height: 100vh;
      display: flex;
      align-items: center;
      justify-content: center;
      background: linear-gradient(135deg, #1a1a2e 0%, #16213e 100%);
      color: #fff;
    }
    .container {
      text-align: center;
      padding: 2rem;
    }
    h1 {
      font-size: 2rem;
      margin-bottom: 1rem;
      opacity: 0.9;
    }
    p {
      font-size: 1.1rem;
      opacity: 0.7;
      margin-bottom: 2rem;
    }
    a {
      display: inline-block;
      background: #fff;
      color: #1a1a2e;
      padding: 0.75rem 2rem;
      border-radius: 8px;
      text-decoration: none;
      font-weight: 600;
      transition: transform 0.2s, box-shadow 0.2s;
    }
    a:hover {
      transform: translateY(-2px);
      box-shadow: 0 4px 20px rgba(255,255,255,0.2);
    }
  </style>
</head>
<body>
  <div class="container">
    <h1>You've reached the OAuth service</h1>
    <p>This is just the authentication backend. You're probably looking for the main app.</p>
    <a href="https://virtues.com">Go to virtues.com</a>
  </div>
</body>
</html>
  `.trim());
});

// Health check
app.get('/health', (req: Request, res: Response) => {
  res.json({ status: 'ok', timestamp: new Date().toISOString() });
});

// OAuth routes
app.use('/google', googleRouter);
app.use('/notion', notionRouter);
app.use('/strava', stravaRouter);

// Error handling
app.use(errorHandler);

// 404 handler
app.use('*', (req: Request, res: Response) => {
  res.status(404).json({ error: 'Route not found' });
});

// Only start server if not in serverless environment (Vercel)
if (process.env.NODE_ENV !== 'production' || !process.env.VERCEL) {
  app.listen(PORT, () => {
    console.log(`🚀 OAuth proxy server running on port ${PORT}`);
    console.log(`🌐 Environment: ${process.env.NODE_ENV || 'development'}`);
    console.log(`📦 Providers: Google, Notion, Microsoft, GitHub, Strava`);
  });
}

export default app;