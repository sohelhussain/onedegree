-- Migration: 20261112000005_create_trigger_logs.sql
-- Description: Create trigger_logs table as an event stream (no polarity column)

CREATE TABLE IF NOT EXISTS trigger_logs (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    emotion VARCHAR(64) NOT NULL, -- Core emotions ('happy', 'calm', 'angry', 'sad', 'anxious') or custom tags
    reason TEXT, -- Written reason for the emotion occurrence
    log_date DATE NOT NULL, -- User local date
    logged_at TIMESTAMPTZ NOT NULL DEFAULT now()
    -- No polarity column:
    --   '+' inserts a row with logged_at timestamp and reason
    --   '-' deletes the latest row for that (user_id, emotion, log_date)
);

CREATE INDEX IF NOT EXISTS idx_trigger_logs_user_date_emotion ON trigger_logs(user_id, log_date, emotion);
CREATE INDEX IF NOT EXISTS idx_trigger_logs_user_logged ON trigger_logs(user_id, logged_at DESC);
