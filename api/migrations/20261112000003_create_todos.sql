-- Migration: 20261112000003_create_todos.sql
-- Description: Create todos table with date-based querying (no midnight cron job)

CREATE TABLE IF NOT EXISTS todos (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    title VARCHAR(255) NOT NULL,
    target_date DATE NOT NULL, -- Queried dynamically by user's local date (today / tomorrow)
    is_completed BOOLEAN NOT NULL DEFAULT FALSE,
    completed_at TIMESTAMPTZ,
    coin_reward INTEGER NOT NULL DEFAULT 5,
    coins_earned INTEGER NOT NULL DEFAULT 0, -- 0 if completed past the first 3 to-dos limit of the day
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_todos_user_target_date ON todos(user_id, target_date);
CREATE INDEX IF NOT EXISTS idx_todos_user_completed ON todos(user_id, target_date, is_completed);
