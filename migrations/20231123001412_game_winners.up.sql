CREATE TABLE IF NOT EXISTS game_winners (
    game_winner_id SERIAL PRIMARY KEY,
    game_player_id BIGINT NOT NULL,
    game_code VARCHAR(128) NOT NULL
);

CREATE INDEX idx_game_winners_game_player_id ON game_winners(game_player_id);
CREATE INDEX idx_game_winners_game_code ON game_winners(game_code);

