CREATE TABLE IF NOT EXISTS games (
	game_id SERIAL PRIMARY KEY,
    user_id VARCHAR(128) NOT NULL,

    name VARCHAR(1024) NOT NULL,
    reward_message VARCHAR(1024)
);

CREATE INDEX idx_games_user_id ON games(user_id);

