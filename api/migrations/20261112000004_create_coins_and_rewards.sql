-- Migration: 20261112000004_create_coins_and_rewards.sql
-- Description: Create coin_transactions (with UNIQUE ref_type, ref_id), rewards, and reward_redemptions

CREATE TABLE IF NOT EXISTS coin_transactions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    amount INTEGER NOT NULL, -- Positive for credits, negative for debits
    ref_type VARCHAR(32) NOT NULL, -- 'habit_tick', 'habit_reversal', 'todo_completed', 'reward_redeemed'
    ref_id UUID NOT NULL, -- action_logs.id (for habit_tick & habit_reversal), todo.id, or redemption.id
    description VARCHAR(255) NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT uq_coin_transactions_ref UNIQUE (ref_type, ref_id) -- Prevents duplicate payouts / duplicate reversals on retry
);

CREATE INDEX IF NOT EXISTS idx_coin_transactions_user_created ON coin_transactions(user_id, created_at DESC);

CREATE TABLE IF NOT EXISTS rewards (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    title VARCHAR(255) NOT NULL,
    description TEXT,
    coin_cost INTEGER NOT NULL CHECK (coin_cost > 0),
    icon VARCHAR(64) NOT NULL DEFAULT 'gift',
    is_archived BOOLEAN NOT NULL DEFAULT FALSE, -- Archived, never deleted
    archived_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_rewards_user_archived ON rewards(user_id, is_archived);

CREATE TABLE IF NOT EXISTS reward_redemptions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    reward_id UUID NOT NULL REFERENCES rewards(id) ON DELETE CASCADE,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    coins_spent INTEGER NOT NULL,
    redeemed_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_reward_redemptions_user ON reward_redemptions(user_id, redeemed_at DESC);
