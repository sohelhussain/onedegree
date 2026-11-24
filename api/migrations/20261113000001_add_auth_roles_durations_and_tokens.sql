-- Migration: 20261113000001_add_auth_roles_durations_and_tokens.sql
-- Description: Add password auth, email verification, roles, premium fields, duration_minutes, and token tables

-- 1. Create role enum
DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM pg_type WHERE typname = 'user_role') THEN
        CREATE TYPE user_role AS ENUM ('user', 'subadmin', 'admin');
    END IF;
END$$;

-- 2. Alter users table
ALTER TABLE users
    ADD COLUMN IF NOT EXISTS password_hash VARCHAR(255),
    ADD COLUMN IF NOT EXISTS email_verified_at TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS role user_role NOT NULL DEFAULT 'user',
    ADD COLUMN IF NOT EXISTS is_premium BOOLEAN NOT NULL DEFAULT false,
    ADD COLUMN IF NOT EXISTS premium_until TIMESTAMPTZ;

-- 3. Unique index on lower(email) where email is not null
CREATE UNIQUE INDEX IF NOT EXISTS uq_users_lower_email ON users (LOWER(email)) WHERE email IS NOT NULL;

-- 4. daily_actions duration_minutes (smallint, nullable, CHECK between 1 and 600)
ALTER TABLE daily_actions
    ADD COLUMN IF NOT EXISTS duration_minutes SMALLINT CHECK (duration_minutes IS NULL OR (duration_minutes >= 1 AND duration_minutes <= 600));

-- 5. todos duration_minutes (smallint, nullable, CHECK between 1 and 600)
ALTER TABLE todos
    ADD COLUMN IF NOT EXISTS duration_minutes SMALLINT CHECK (duration_minutes IS NULL OR (duration_minutes >= 1 AND duration_minutes <= 600));

-- 6. Email verification tokens (stored hashed, single-use, with expiry)
CREATE TABLE IF NOT EXISTS email_verification_tokens (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    token_hash VARCHAR(255) NOT NULL UNIQUE,
    expires_at TIMESTAMPTZ NOT NULL,
    used_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_email_verification_tokens_user ON email_verification_tokens(user_id);
CREATE INDEX IF NOT EXISTS idx_email_verification_tokens_hash ON email_verification_tokens(token_hash);

-- 7. Password reset tokens (stored hashed, single-use, with expiry)
CREATE TABLE IF NOT EXISTS password_reset_tokens (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    token_hash VARCHAR(255) NOT NULL UNIQUE,
    expires_at TIMESTAMPTZ NOT NULL,
    used_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_password_reset_tokens_user ON password_reset_tokens(user_id);
CREATE INDEX IF NOT EXISTS idx_password_reset_tokens_hash ON password_reset_tokens(token_hash);
