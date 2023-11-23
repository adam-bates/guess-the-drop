CREATE TABLE IF NOT EXISTS game_winners (
    game_winner_id SERIAL PRIMARY KEY,
    game_player_id BIGINT UNSIGNED NOT NULL
);

CREATE INDEX idx_game_winners_game_player_id ON game_winners(game_player_id);

