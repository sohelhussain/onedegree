-- Migration: 20261112000002_create_goals_and_actions.sql
-- Description: Create goals, daily_actions, and action_logs tables

CREATE TABLE IF NOT EXISTS goals (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    title VARCHAR(255) NOT NULL,
    description TEXT,
    start_date DATE NOT NULL DEFAULT CURRENT_DATE,
    target_milestone_days INTEGER NOT NULL DEFAULT 90,
    color_hex VARCHAR(16) NOT NULL DEFAULT '#10b981',
    is_archived BOOLEAN NOT NULL DEFAULT FALSE, -- Archived, never deleted
    archived_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_goals_user_id ON goals(user_id);
CREATE INDEX IF NOT EXISTS idx_goals_is_archived ON goals(user_id, is_archived);

CREATE TABLE IF NOT EXISTS daily_actions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    goal_id UUID NOT NULL REFERENCES goals(id) ON DELETE CASCADE,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    title VARCHAR(255) NOT NULL,
    coin_reward INTEGER NOT NULL DEFAULT 10,
    is_archived BOOLEAN NOT NULL DEFAULT FALSE, -- Archived, never deleted
    archived_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_daily_actions_goal_id ON daily_actions(goal_id);
CREATE INDEX IF NOT EXISTS idx_daily_actions_user_id ON daily_actions(user_id);
CREATE INDEX IF NOT EXISTS idx_daily_actions_is_archived ON daily_actions(user_id, is_archived);

CREATE TABLE IF NOT EXISTS action_logs (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    action_id UUID NOT NULL REFERENCES daily_actions(id) ON DELETE CASCADE,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    log_date DATE NOT NULL, -- User's local calendar date
    status VARCHAR(16) NOT NULL DEFAULT 'done' CHECK (status IN ('done', 'missed')), -- Status ('done' | 'missed'). Missed rows earn 0 coins.
    completed_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    coins_earned INTEGER NOT NULL DEFAULT 0, -- 0 for missed rows or backdated ticks, >0 only for current day done ticks
    notes TEXT,
    CONSTRAINT uq_action_logs_action_date UNIQUE (action_id, log_date)
);

CREATE INDEX IF NOT EXISTS idx_action_logs_user_date ON action_logs(user_id, log_date);
CREATE INDEX IF NOT EXISTS idx_action_logs_action_id ON action_logs(action_id);
