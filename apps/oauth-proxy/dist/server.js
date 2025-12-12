"use strict";
var __importDefault = (this && this.__importDefault) || function (mod) {
    return (mod && mod.__esModule) ? mod : { "default": mod };
};
Object.defineProperty(exports, "__esModule", { value: true });
const dotenv_1 = __importDefault(require("dotenv"));
dotenv_1.default.config(); // Load environment variables FIRST
const express_1 = __importDefault(require("express"));
const cors_1 = __importDefault(require("cors"));
const helmet_1 = __importDefault(require("helmet"));
const express_rate_limit_1 = __importDefault(require("express-rate-limit"));
const google_1 = require("./routes/google");
const notion_1 = __importDefault(require("./routes/notion"));
const strava_1 = require("./routes/strava");
const error_handler_1 = require("./middleware/error-handler");
const logger_1 = require("./middleware/logger");
const app = (0, express_1.default)();
const PORT = process.env.PORT || 3000;
// Trust proxy for Vercel/serverless environments
app.set('trust proxy', 1);
// Security middleware
app.use((0, helmet_1.default)());
app.use((0, cors_1.default)({
    origin: process.env.ALLOWED_ORIGINS?.split(',') || ['http://localhost:5173'],
    credentials: true
}));
// Rate limiting
const limiter = (0, express_rate_limit_1.default)({
    windowMs: 15 * 60 * 1000, // 15 minutes
    max: 100, // limit each IP to 100 requests per windowMs
    message: 'Too many requests from this IP, please try again later.'
});
app.use(limiter);
// Middleware
app.use(express_1.default.json());
app.use(logger_1.logger);
// Homepage - redirect users who land here by mistake
app.get('/', (req, res) => {
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
app.get('/health', (req, res) => {
    res.json({ status: 'ok', timestamp: new Date().toISOString() });
});
// OAuth routes
app.use('/google', google_1.googleRouter);
app.use('/notion', notion_1.default);
app.use('/strava', strava_1.stravaRouter);
// Error handling
app.use(error_handler_1.errorHandler);
// 404 handler
app.use('*', (req, res) => {
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
exports.default = app;
//# sourceMappingURL=server.js.map