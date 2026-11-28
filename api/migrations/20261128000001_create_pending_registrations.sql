-- Migration: 20261128000001_create_pending_registrations.sql
-- Description: Store unverified email registrations in pending_registrations until password-confirmed verification

CREATE TABLE IF NOT EXISTS pending_registrations (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    email VARCHAR(255) NOT NULL UNIQUE,
    full_name VARCHAR(255),
    password_hash VARCHAR(255) NOT NULL,
    token_hash VARCHAR(255) UNIQUE,
    expires_at TIMESTAMPTZ NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    last_sent_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    send_count INT NOT NULL DEFAULT 1,
    failed_attempts INT NOT NULL DEFAULT 0
);

CREATE INDEX IF NOT EXISTS idx_pending_registrations_expires_at
    ON pending_registrations(expires_at);
