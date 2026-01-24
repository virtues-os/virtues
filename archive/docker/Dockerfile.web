# Development stage
FROM node:20-slim AS development

# Install pnpm
RUN npm install -g pnpm

WORKDIR /app

# Copy package files
COPY package.json pnpm-lock.yaml ./

# Install dependencies
RUN pnpm install --frozen-lockfile

# Copy source code
COPY . .

# Expose dev server port
EXPOSE 5173

# Default dev command (can be overridden)
CMD ["pnpm", "dev", "--host", "0.0.0.0"]

# Build stage
FROM node:20-slim AS builder

# Install pnpm
RUN npm install -g pnpm

WORKDIR /app

# Copy package files
COPY package.json pnpm-lock.yaml ./

# Install dependencies (including devDependencies for build)
RUN pnpm install --frozen-lockfile

# Copy source code
COPY . .

# Build the app with Node adapter for Docker deployment
ENV ADAPTER=node
RUN pnpm build

# Production stage
FROM node:20-slim AS production

# Install pnpm for production dependency installation
RUN npm install -g pnpm

WORKDIR /app

# Copy package files and install production dependencies only
# This ensures runtime dependencies like @auth/core are available
COPY --from=builder /app/package.json ./
COPY --from=builder /app/pnpm-lock.yaml ./
RUN pnpm install --prod --frozen-lockfile

# Copy built app from builder (adapter-node outputs to /build)
COPY --from=builder /app/build ./build

# Expose production port
EXPOSE 3000

# Set production environment
ENV NODE_ENV=production

# Run the built app
CMD ["node", "build"]
