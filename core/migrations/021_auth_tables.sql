-- Auth.js tables for magic link authentication
-- These tables are used by Auth.js to manage users, sessions, and verification tokens

-- User table (minimal - Auth.js only needs email)
CREATE TABLE app.auth_user (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    email TEXT UNIQUE NOT NULL,
    email_verified TIMESTAMPTZ
);

-- Session table for database-backed sessions
CREATE TABLE app.auth_session (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    session_token TEXT UNIQUE NOT NULL,
    user_id UUID NOT NULL REFERENCES app.auth_user(id) ON DELETE CASCADE,
    expires TIMESTAMPTZ NOT NULL
);

-- Verification token table for magic link tokens
CREATE TABLE app.auth_verification_token (
    identifier TEXT NOT NULL,
    token TEXT NOT NULL,
    expires TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (identifier, token)
);

-- Index for session lookups
CREATE INDEX idx_auth_session_token ON app.auth_session(session_token);
CREATE INDEX idx_auth_session_user_id ON app.auth_session(user_id);

-- Index for cleaning up expired sessions (used by cleanup job)
CREATE INDEX idx_auth_session_expires ON app.auth_session(expires);

-- Index for cleaning up expired tokens
CREATE INDEX idx_auth_verification_token_expires ON app.auth_verification_token(expires);
