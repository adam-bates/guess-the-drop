CREATE TABLE IF NOT EXISTS games (
    game_code VARCHAR(128) NOT NULL PRIMARY KEY,
    user_id VARCHAR(128) NOT NULL,

	status VARCHAR(128) NOT NULL,
	created_at BIGINT UNSIGNED NOT NULL,
	active_at BIGINT UNSIGNED NOT NULL,

    name VARCHAR(1024) NOT NULL,
    reward_message VARCHAR(1024),
    total_reward_message VARCHAR(1024),

    is_locked BOOLEAN NOT NULL
);

CREATE INDEX idx_games_user_id ON games(user_id);

