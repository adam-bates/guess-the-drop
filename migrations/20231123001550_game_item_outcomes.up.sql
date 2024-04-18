CREATE TABLE IF NOT EXISTS game_item_outcomes (
    outcome_id SERIAL PRIMARY KEY,

    game_code VARCHAR(128) NOT NULL,
    item_id BIGINT NOT NULL
);

CREATE INDEX idx_game_item_outcomes_game_code ON game_item_outcomes(game_code);

