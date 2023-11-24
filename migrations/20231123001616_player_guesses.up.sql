CREATE TABLE IF NOT EXISTS player_guesses (
    player_guess_id SERIAL PRIMARY KEY,
    game_code VARCHAR(128) NOT NULL,
    player_id BIGINT UNSIGNED NOT NULL,
    item_id BIGINT UNSIGNED NOT NULL,

    outcome_id BIGINT UNSIGNED
);

CREATE INDEX idx_player_guesses_game_code ON player_guesses(game_code);

