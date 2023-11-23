CREATE TABLE IF NOT EXISTS game_players (
    game_player_id SERIAL PRIMARY KEY,

    game_code VARCHAR(128) NOT NULL,
    user_id VARCHAR(128) NOT NULL,

	points INT NOT NULL
);

CREATE INDEX idx_game_players_game_code ON game_players(game_code);
CREATE INDEX idx_game_players_user_id ON game_players(user_id);

