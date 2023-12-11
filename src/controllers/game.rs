use std::{collections::HashSet, time::SystemTime};

use super::*;

use crate::{
    models::{
        Game, GameItem, GameItemOutcome, GameItemTemplate, GameItemWithGuessCount, GamePlayer,
        GameTemplate, GameWithHostedSummary, GameWithJoinedSummary, PlayerGuess, User,
        GAME_STATUS_ACTIVE, GAME_STATUS_FINISHED,
    },
    prelude::*,
    pubsub::{HostAction, HostActionType, PlayerAction, PlayerActionType},
    GameBroadcast,
};

use askama::Template;
use axum::{
    extract::{Path, Query, State},
    response::{
        sse::{Event, KeepAlive, Sse},
        IntoResponse, Redirect, Response,
    },
    routing::{get, post, put},
    Form, Router,
};
use serde::Deserialize;
use tokio::{sync::broadcast, task::JoinHandle};
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::StreamExt as _;
use tower_sessions::Session;

pub fn add_routes(router: Router<AppState>) -> Router<AppState> {
    return router
        .route("/join", get(join))
        .route("/games", get(games).post(post_game))
        .route("/games/:game_code", get(game))
        .route("/games/:game_code/finish", post(finish_game))
        .route("/games/:game_code/x/redirect", get(game_x_redirect))
        .route("/games/:game_code/x/board", get(game_x_board))
        .route("/games/:game_code/x/lock", put(game_x_lock))
        .route("/games/:game_code/x/unlock", put(game_x_unlock))
        .route(
            "/games/:game_code/x/clear-guesses",
            put(game_x_clear_guesses),
        )
        .route(
            "/games/:game_code/items/:game_item_id/x/enable",
            put(game_x_enable_item),
        )
        .route(
            "/games/:game_code/items/:game_item_id/x/disable",
            put(game_x_disable_item),
        )
        .route(
            "/games/:game_code/items/:game_item_id/x/choose",
            put(game_x_choose_item),
        )
        .route(
            "/games/:game_code/items/:game_item_id/x/guess",
            put(game_x_guess_item),
        )
        .route("/games/:game_code/sse/host", get(host_sse))
        .route("/games/:game_code/sse/player", get(player_sse));
}

#[derive(Debug, Deserialize)]
struct JoinParams {
    code: Option<String>,
}

#[derive(Template)]
#[template(path = "join.html")]
struct JoinTemplate {
    game_code: String,
}

async fn join(
    Query(params): Query<JoinParams>,
    session: Session,
    State(state): State<AppState>,
) -> Result<Response> {
    let Some(game_code) = params.code else {
        return Ok(Redirect::to("/").into_response());
    };

    if game_code.trim().is_empty() {
        return Ok(Redirect::to("/").into_response());
    }

    let game_code = game_code.to_lowercase();

    let game = sqlx::query("SELECT * FROM games WHERE game_code = ? LIMIT 1")
        .bind(&game_code)
        .fetch_optional(&state.db)
        .await?;

    if game.is_none() {
        return Ok(Redirect::to("/").into_response());
    }

    let sid = utils::session_id(&session)?;
    let user_auth = utils::find_user(&state, &sid).await?;

    if user_auth.is_some() {
        return Ok(Redirect::to(&format!("/games/{}", game_code)).into_response());
    }

    return Ok(Html(JoinTemplate { game_code }).into_response());
}

#[derive(Debug, Deserialize)]
struct PostGame {
    template: u64,
}

async fn post_game(
    session: Session,
    State(state): State<AppState>,
    Form(body): Form<PostGame>,
) -> Result<Response> {
    let sid = utils::session_id(&session)?;
    let (user, _) = utils::require_user(&state, &sid).await?.split();

    let game_template: Option<GameTemplate> = sqlx::query_as(
        "SELECT * FROM game_templates WHERE game_template_id = ? AND user_id = ? LIMIT 1",
    )
    .bind(&body.template)
    .bind(&user.user_id)
    .fetch_optional(&state.db)
    .await?;

    let Some(game_template) = game_template else {
        return Ok((StatusCode::BAD_REQUEST, "Template not found").into_response());
    };

    let game_item_templates: Vec<GameItemTemplate> =
        sqlx::query_as("SELECT * FROM game_item_templates WHERE game_template_id = ?")
            .bind(&game_template.game_template_id)
            .fetch_all(&state.db)
            .await?;

    let game_code = {
        let mut game_code;

        // Keep searching until unique code is found or request times out
        loop {
            const GAME_CODE_LENGTH: usize = 6;
            const GAME_CODE_CHARS: [char; 16] = [
                '1', '2', '3', '4', '5', '6', '7', '8', '9', '0', 'a', 'b', 'c', 'd', 'e', 'f',
            ];

            game_code = nanoid!(GAME_CODE_LENGTH, &GAME_CODE_CHARS);

            let existing: Option<Game> = sqlx::query_as("SELECT * FROM games WHERE game_code = ?")
                .bind(&game_code)
                .fetch_optional(&state.db)
                .await?;

            if existing.is_none() {
                break;
            }
        }

        game_code
    };

    let now = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)?
        .as_secs();

    sqlx::query("INSERT INTO games (user_id, game_code, status, created_at, active_at, name, auto_lock, reward_message, total_reward_message, is_locked) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)")
        .bind(&user.user_id)
        .bind(&game_code)
        .bind(GAME_STATUS_ACTIVE)
        .bind(&now)
        .bind(&now)
        .bind(&game_template.name)
        .bind(&game_template.auto_lock)
        .bind(&game_template.reward_message)
        .bind(&game_template.total_reward_message)
        .bind(&game_template.auto_lock)
        .execute(&state.db)
        .await?;

    let game: Game = sqlx::query_as("SELECT * FROM games WHERE game_code = ?")
        .bind(&game_code)
        .fetch_one(&state.db)
        .await?;

    if !game_item_templates.is_empty() {
        let query = format!(
            "INSERT INTO game_items (game_code, name, image, enabled) VALUES {}",
            game_item_templates
                .iter()
                .map(|_| "(?, ?, ?, ?)")
                .collect::<Vec<&'static str>>()
                .join(",")
        );

        let mut q = sqlx::query(&query);

        for game_item_template in game_item_templates {
            q = q
                .bind(&game.game_code)
                .bind(game_item_template.name)
                .bind(game_item_template.image)
                .bind(game_item_template.start_enabled);
        }

        q.execute(&state.db).await?;
    }

    return Ok(Redirect::to(&format!("/games/{}", game.game_code)).into_response());
}

#[derive(Template, Clone)]
#[template(path = "games.html")]
struct GamesTemplate {
    user: User,
    games_joined: Vec<GameWithJoinedSummary>,
    games_hosted: Vec<GameWithHostedSummary>,
}

async fn games(session: Session, State(state): State<AppState>) -> Result<Response> {
    let session_id = utils::session_id(&session)?;
    let (user, _) = utils::require_user(&state, &session_id).await?.split();

    let games_joined: Vec<GameWithJoinedSummary> = sqlx::query_as(
        r#"
SELECT *
FROM games
LEFT OUTER JOIN (
	SELECT game_players.game_code AS gp_game_code, COUNT(*) AS players_count
	FROM game_players
	GROUP BY gp_game_code
) AS players_counts ON players_counts.gp_game_code = games.game_code
LEFT OUTER JOIN (
	SELECT game_code AS gp2_game_code, points
	FROM game_players
    WHERE user_id = ?
) AS points ON points.gp2_game_code = games.game_code
LEFT OUTER JOIN (
	SELECT game_winners.game_code AS gw_game_code, COUNT(*) AS winners_count, MAX(points) AS winning_points
	FROM game_winners
		INNER JOIN game_players ON game_players.game_player_id = game_winners.game_player_id
	GROUP BY gw_game_code
) AS winners_counts ON winners_counts.gw_game_code = games.game_code
LEFT OUTER JOIN (
	SELECT game_winners.game_code AS gw_game_code2, COUNT(*) > 0 AS is_winner
    FROM game_winners
		INNER JOIN game_players ON game_players.game_player_id = game_winners.game_player_id
	WHERE game_players.user_id = ?
    GROUP BY gw_game_code2
) AS is_winners ON is_winners.gw_game_code2 = games.game_code
LEFT OUTER JOIN (
    SELECT user_id as u_user_id, username AS host
    FROM users
) AS hosts ON hosts.u_user_id = games.user_id
LEFT OUTER JOIN (
    SELECT game_code as gio_game_code, COUNT(*) AS total_drops
    FROM game_item_outcomes
    GROUP BY gio_game_code
) AS total_drops ON total_drops.gio_game_code = games.game_code
WHERE games.game_code IN (
	SELECT game_code
    FROM game_players
    WHERE user_id = ?
)
ORDER BY status ASC, created_at DESC
        "#,
    )
    .bind(&user.user_id)
    .bind(&user.user_id)
    .bind(&user.user_id)
    .fetch_all(&state.db)
    .await?;

    let games_hosted: Vec<GameWithHostedSummary> = sqlx::query_as(
        r#"
SELECT *
FROM games
LEFT OUTER JOIN (
	SELECT game_players.game_code AS gp_game_code, COUNT(*) AS players_count
	FROM game_players
	GROUP BY gp_game_code
) AS players_counts ON players_counts.gp_game_code = games.game_code
LEFT OUTER JOIN (
	SELECT game_winners.game_code AS gw_game_code, COUNT(*) AS winners_count, MAX(points) AS winning_points
	FROM game_winners
		INNER JOIN game_players ON game_players.game_player_id = game_winners.game_player_id
	GROUP BY gw_game_code
) AS winners_counts ON winners_counts.gw_game_code = games.game_code
LEFT OUTER JOIN (
    SELECT game_code as gio_game_code, COUNT(*) AS total_drops
    FROM game_item_outcomes
    GROUP BY gio_game_code
) AS total_drops ON total_drops.gio_game_code = games.game_code
WHERE games.user_id = ?
ORDER BY status ASC, created_at DESC
        "#,
    )
    .bind(&user.user_id)
    .fetch_all(&state.db)
    .await?;

    return Ok(Html(GamesTemplate {
        user,
        games_joined,
        games_hosted,
    })
    .into_response());
}

#[derive(Template, Clone)]
#[template(path = "game-as-player.html")]
struct GameAsPlayerTemplate {
    game: Game,
    host: User,
    guess: Option<PlayerGuess>,
    items: Vec<GameItemWithGuessCount>,
    user: User,
    player: GamePlayer,
    drops_count: i64,
    img_base_uri: String,
}

#[derive(Template, Clone)]
#[template(path = "game-as-host.html")]
struct GameAsHostTemplate {
    game: Game,
    items: Vec<GameItemWithGuessCount>,
    any_guesses: bool,
    user: User,
    players_count: i64,
    drops_count: i64,
    leaders: String,
    lead_points: i32,
    base_uri: String,
    img_base_uri: String,
}

#[derive(Template, Clone)]
#[template(path = "finished-game-as-player.html")]
struct FinishedGameAsPlayerTemplate {
    game: Game,
    host: User,
    items: Vec<GameItemWithGuessCount>,
    user: User,
    player: GamePlayer,
    drops_count: i64,
    is_winner: bool,
    img_base_uri: String,
}

#[derive(Template, Clone)]
#[template(path = "finished-game-as-host.html")]
struct FinishedGameAsHostTemplate {
    game: Game,
    items: Vec<GameItemWithGuessCount>,
    user: User,
    players_count: i64,
    drops_count: i64,
    leaders: String,
    lead_points: i32,
    img_base_uri: String,
}

async fn game(
    Path(game_code): Path<String>,
    session: Session,
    State(state): State<AppState>,
) -> Result<Response> {
    let session_id = utils::session_id(&session)?;
    let user_auth = utils::find_user(&state, &session_id).await?;

    let Some(user_auth) = user_auth else {
        return Ok(Redirect::to(&format!("/join?code={game_code}")).into_response());
    };
    let (user, _) = user_auth.split();

    if game_code.trim().is_empty() {
        return Ok(Redirect::to("/").into_response());
    }
    let game_code = game_code.to_lowercase();

    let game: Option<Game> = sqlx::query_as("SELECT * FROM games WHERE game_code = ? LIMIT 1")
        .bind(&game_code)
        .fetch_optional(&state.db)
        .await?;

    let Some(game) = game else {
        return Ok(Redirect::to("/").into_response());
    };

    let items: Vec<GameItemWithGuessCount> = sqlx::query_as(
        r#"
SELECT *
FROM game_items
    LEFT OUTER JOIN (
        SELECT item_id, COUNT(*) AS guess_count
        FROM player_guesses
        WHERE
            game_code = ? AND
            outcome_id IS NULL
        GROUP BY item_id
    ) AS guess_counts ON guess_counts.item_id = game_items.game_item_id
WHERE
    game_code = ?
            "#,
    )
    .bind(&game_code)
    .bind(&game_code)
    .fetch_all(&state.db)
    .await?;

    let host = sqlx::query_as("SELECT * FROM users WHERE user_id = ?")
        .bind(&game.user_id)
        .fetch_one(&state.db)
        .await?;

    let drops_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM game_item_outcomes WHERE game_code = ?")
            .bind(&game_code)
            .fetch_optional(&state.db)
            .await?
            .unwrap_or(0);

    if game.status != GAME_STATUS_ACTIVE {
        let lead_points: Option<Option<i32>> =
            sqlx::query_scalar("SELECT MAX(points) FROM game_players WHERE game_code = ?")
                .bind(&game_code)
                .fetch_optional(&state.db)
                .await?;
        let lead_points = lead_points.flatten().unwrap_or(0);

        if game.user_id == user.user_id {
            let players_count: i64 =
                sqlx::query_scalar("SELECT COUNT(*) FROM game_players WHERE game_code = ?")
                    .bind(&game_code)
                    .fetch_optional(&state.db)
                    .await?
                    .unwrap_or(0);

            let leaders: Vec<String> =
        sqlx::query_scalar("SELECT users.username FROM game_players INNER JOIN users ON users.user_id = game_players.user_id WHERE game_code = ? AND points = ?")
            .bind(&game_code)
            .bind(&lead_points)
            .fetch_all(&state.db)
            .await?;
            let leaders = leaders.join(", ");

            return Ok(Html(FinishedGameAsHostTemplate {
                img_base_uri: state.cfg.r2_bucket_public_url.clone(),
                game,
                items,
                user,
                players_count,
                drops_count,
                leaders,
                lead_points,
            })
            .into_response());
        }

        let player: Option<GamePlayer> =
            sqlx::query_as("SELECT * FROM game_players WHERE game_code = ? AND user_id = ?")
                .bind(&game_code)
                .bind(&user.user_id)
                .fetch_optional(&state.db)
                .await?;

        let Some(player) = player else {
            return Ok(Redirect::to("/").into_response());
        };

        return Ok(Html(FinishedGameAsPlayerTemplate {
            img_base_uri: state.cfg.r2_bucket_public_url.clone(),
            game,
            host,
            items,
            user,
            is_winner: player.points == lead_points,
            player,
            drops_count,
        })
        .into_response());
    }

    {
        let broadcasts = &mut *state.game_broadcasts.write().unwrap();

        let _entry = broadcasts.entry(game_code.clone()).or_insert_with(|| {
            let (host_tx, _host_rx) = broadcast::channel(16);
            let (players_tx, _players_rx) = broadcast::channel(16);

            return GameBroadcast {
                to_host: host_tx,
                to_players: players_tx,
            };
        });
    }

    if game.user_id == user.user_id {
        let lead_points: Option<Option<i32>> =
            sqlx::query_scalar("SELECT MAX(points) FROM game_players WHERE game_code = ?")
                .bind(&game_code)
                .fetch_optional(&state.db)
                .await?;
        let lead_points = lead_points.flatten().unwrap_or(0);

        let players_count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM game_players WHERE game_code = ?")
                .bind(&game_code)
                .fetch_optional(&state.db)
                .await?
                .unwrap_or(0);

        let leaders: Vec<String> =
        sqlx::query_scalar("SELECT users.username FROM game_players INNER JOIN users ON users.user_id = game_players.user_id WHERE game_code = ? AND points = ?")
            .bind(&game_code)
            .bind(&lead_points)
            .fetch_all(&state.db)
            .await?;
        let leaders = leaders.join(", ");

        return Ok(Html(GameAsHostTemplate {
            base_uri: state.cfg.server_host_uri.clone(),
            img_base_uri: state.cfg.r2_bucket_public_url.clone(),
            game,
            any_guesses: items
                .iter()
                .any(|item| item.guess_count.map(|c| c > 0).unwrap_or(false)),
            items,
            user,
            players_count,
            drops_count,
            lead_points,
            leaders,
        })
        .into_response());
    }

    let player: Option<GamePlayer> =
        sqlx::query_as("SELECT * FROM game_players WHERE game_code = ? AND user_id = ?")
            .bind(&game_code)
            .bind(&user.user_id)
            .fetch_optional(&state.db)
            .await?;

    let (player, guess) = if let Some(player) = player {
        let guess: Option<PlayerGuess> = sqlx::query_as("SELECT * FROM player_guesses WHERE game_code = ? AND player_id = ? AND outcome_id IS NULL LIMIT 1")
            .bind(&game_code)
            .bind(&player.game_player_id)
            .fetch_optional(&state.db)
            .await?;

        (player, guess)
    } else {
        sqlx::query("INSERT INTO game_players (game_code, user_id, points) VALUES (?, ?, 0)")
            .bind(&game_code)
            .bind(&user.user_id)
            .execute(&state.db)
            .await?;

        let jh1 = {
            let game_code = game_code.clone();
            let user_id = user.user_id.clone();
            let db = state.db.clone();
            let pubsub = state.pubsub.clone();

            tokio::spawn(async move {
                let players_count: i64 =
                    sqlx::query_scalar("SELECT COUNT(*) FROM game_players WHERE game_code = ?")
                        .bind(&game_code)
                        .fetch_optional(&db)
                        .await?
                        .unwrap_or(0);

                return Ok(pubsub
                    .player_actions
                    .publish(PlayerAction {
                        game_code,
                        user_id,
                        typ: PlayerActionType::Join {
                            new_players_count: players_count,
                        },
                    })
                    .await?) as Result;
            })
        };

        let jh2 = if let Ok(now) = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH) {
            let now_s = now.as_secs();
            let game_code = game_code.clone();
            let db = state.db.clone();

            Some(tokio::spawn(async move {
                sqlx::query("UPDATE games SET active_at = ? WHERE game_code = ?")
                    .bind(now_s)
                    .bind(&game_code)
                    .execute(&db)
                    .await?;

                return Ok(()) as Result;
            }))
        } else {
            None
        };

        let jh3: JoinHandle<Result<GamePlayer>> = {
            let game_code = game_code.clone();
            let user_id = user.user_id.clone();
            let db = state.db.clone();

            tokio::spawn(async move {
                return Ok(sqlx::query_as(
                    "SELECT * FROM game_players WHERE game_code = ? AND user_id = ?",
                )
                .bind(&game_code)
                .bind(&user_id)
                .fetch_one(&db)
                .await?);
            })
        };

        let gp = if let Some(jh2) = jh2 {
            let (r1, r2, r3) = tokio::join! {
                jh1, jh2, jh3,
            };
            r1??;
            r2??;
            r3??
        } else {
            let (r1, r3) = tokio::join! {
                jh1, jh3,
            };
            r1??;
            r3??
        };

        (gp, None)
    };

    return Ok(Html(GameAsPlayerTemplate {
        img_base_uri: state.cfg.r2_bucket_public_url.clone(),
        game,
        host,
        guess,
        items,
        user,
        player,
        drops_count,
    })
    .into_response());
}

async fn game_x_redirect(Path(game_code): Path<String>) -> impl IntoResponse {
    return Redirect::to(&format!("/games/{game_code}"));
}

async fn game_x_board(
    Path(game_code): Path<String>,
    session: Session,
    State(state): State<AppState>,
) -> Result<Response> {
    let session_id = utils::session_id(&session)?;
    let (user, _) = utils::require_user(&state, &session_id).await?.split();

    if game_code.trim().is_empty() {
        return Err(anyhow::anyhow!("Missing game_code"))?;
    }
    let game_code = game_code.to_lowercase();

    let game: Option<Game> = sqlx::query_as("SELECT * FROM games WHERE game_code = ? LIMIT 1")
        .bind(&game_code)
        .fetch_optional(&state.db)
        .await?;

    let Some(game) = game else {
        return Err(anyhow::anyhow!("Game not found"))?;
    };

    if game.user_id == user.user_id {
        let (drops_count, lead_points) = {
            let jh1 = {
                let game_code = game_code.clone();
                let db = state.db.clone();

                tokio::spawn(async move {
                    return Ok(sqlx::query_scalar(
                        "SELECT COUNT(*) FROM game_item_outcomes WHERE game_code = ?",
                    )
                    .bind(&game_code)
                    .fetch_optional(&db)
                    .await?
                    .unwrap_or(0i64)) as Result<i64>;
                })
            };

            let jh2 = {
                let game_code = game_code.clone();
                let db = state.db.clone();

                tokio::spawn(async move {
                    let lead_points: Option<Option<i32>> = sqlx::query_scalar(
                        "SELECT MAX(points) FROM game_players WHERE game_code = ?",
                    )
                    .bind(&game_code)
                    .fetch_optional(&db)
                    .await?;
                    return Ok(lead_points.flatten().unwrap_or(0)) as Result<i32>;
                })
            };

            let (r1, r2) = tokio::join! { jh1, jh2 };

            (r1??, r2??)
        };

        let (items, players_count, leaders) = {
            let jh1 = {
                let game_code = game_code.clone();
                let db = state.db.clone();

                tokio::spawn(async move {
                    let items = sqlx::query_as(
                        r#"
SELECT *
FROM game_items
    LEFT OUTER JOIN (
        SELECT item_id, COUNT(*) AS guess_count
        FROM player_guesses
        WHERE
            game_code = ? AND
            outcome_id IS NULL
        GROUP BY item_id
    ) AS guess_counts ON guess_counts.item_id = game_items.game_item_id
WHERE
    game_code = ?
            "#,
                    )
                    .bind(&game_code)
                    .bind(&game_code)
                    .fetch_all(&db)
                    .await?;

                    return Ok(items) as Result<Vec<GameItemWithGuessCount>>;
                })
            };

            let jh2 = {
                let game_code = game_code.clone();
                let db = state.db.clone();

                tokio::spawn(async move {
                    return Ok(sqlx::query_scalar(
                        "SELECT COUNT(*) FROM game_players WHERE game_code = ?",
                    )
                    .bind(&game_code)
                    .fetch_optional(&db)
                    .await?
                    .unwrap_or(0i64)) as Result<i64>;
                })
            };

            let jh3 = {
                let game_code = game_code.clone();
                let db = state.db.clone();

                tokio::spawn(async move {
                    let leaders: Vec<String> =
                    sqlx::query_scalar("SELECT users.username FROM game_players INNER JOIN users ON users.user_id = game_players.user_id WHERE game_code = ? AND points = ?")
                        .bind(&game_code)
                        .bind(&lead_points)
                        .fetch_all(&db)
                        .await?;
                    return Ok(leaders.join(", ")) as Result<String>;
                })
            };

            let (r1, r2, r3) = tokio::join! { jh1, jh2, jh3 };

            (r1??, r2??, r3??)
        };

        return Ok(Html(GameAsHostBoardTemplate {
            img_base_uri: state.cfg.r2_bucket_public_url.clone(),
            game,
            any_guesses: items
                .iter()
                .any(|item| item.guess_count.map(|c| c > 0).unwrap_or(false)),
            items,
            players_count,
            drops_count,
            leaders,
            lead_points,
        })
        .into_response());
    }

    let game_player: Option<GamePlayer> =
        sqlx::query_as("SELECT * FROM game_players WHERE game_code = ? AND user_id = ? LIMIT 1")
            .bind(&game_code)
            .bind(&user.user_id)
            .fetch_optional(&state.db)
            .await?;

    let Some(game_player) = game_player else {
        return Err(anyhow::anyhow!("Player not found"))?;
    };

    let (guess, items, host, drops_count) = {
        let jh1 = {
            let game_code = game_code.clone();
            let game_player_id = game_player.game_player_id.clone();
            let db = state.db.clone();

            tokio::spawn(async move {
                return Ok(sqlx::query_as("SELECT * FROM player_guesses WHERE game_code = ? AND player_id = ? AND outcome_id IS NULL LIMIT 1")
                        .bind(&game_code)
                        .bind(&game_player_id)
                        .fetch_optional(&db)
                        .await?) as Result<Option<PlayerGuess>>;
            })
        };

        let jh2 = {
            let game_code = game_code.clone();
            let db = state.db.clone();

            tokio::spawn(async move {
                return Ok(
                    sqlx::query_as("SELECT * FROM game_items WHERE game_code = ?")
                        .bind(&game_code)
                        .fetch_all(&db)
                        .await?,
                ) as Result<Vec<GameItem>>;
            })
        };

        let jh3 = {
            let user_id = game.user_id.clone();
            let db = state.db.clone();

            tokio::spawn(async move {
                let host: User = sqlx::query_as("SELECT * FROM users WHERE user_id = ?")
                    .bind(&user_id)
                    .fetch_one(&db)
                    .await?;
                return Ok(host) as Result<User>;
            })
        };

        let jh4 = {
            let game_code = game_code.clone();
            let db = state.db.clone();

            tokio::spawn(async move {
                return Ok(sqlx::query_scalar(
                    "SELECT COUNT(*) FROM game_item_outcomes WHERE game_code = ?",
                )
                .bind(&game_code)
                .fetch_optional(&db)
                .await?
                .unwrap_or(0i64)) as Result<i64>;
            })
        };

        let (r1, r2, r3, r4) = tokio::join! { jh1, jh2, jh3, jh4 };

        (r1??, r2??, r3??, r4??)
    };

    return Ok(Html(GameAsPlayerBoardTemplate {
        game,
        host,
        guess,
        items,
        player: game_player,
        drops_count,
        img_base_uri: state.cfg.r2_bucket_public_url.clone(),
    })
    .into_response());
}

#[derive(Template, Clone)]
#[template(path = "game-as-host-board.html")]
struct GameAsHostBoardTemplate {
    game: Game,
    items: Vec<GameItemWithGuessCount>,
    any_guesses: bool,
    players_count: i64,
    drops_count: i64,
    leaders: String,
    lead_points: i32,
    img_base_uri: String,
}

async fn game_x_lock(
    Path(game_code): Path<String>,
    session: Session,
    State(state): State<AppState>,
) -> Result<Response> {
    let session_id = utils::session_id(&session)?;
    let (user, _) = utils::require_user(&state, &session_id).await?.split();

    if game_code.trim().is_empty() {
        return Err(anyhow::anyhow!("Missing game_code"))?;
    }
    let game_code = game_code.to_lowercase();

    let game: Option<Game> = sqlx::query_as(
        "SELECT * FROM games WHERE game_code = ? AND user_id = ? AND status = 'ACTIVE' LIMIT 1",
    )
    .bind(&game_code)
    .bind(&user.user_id)
    .fetch_optional(&state.db)
    .await?;

    let Some(mut game) = game else {
        return Err(anyhow::anyhow!("Game not found"))?;
    };

    if !game.is_locked {
        sqlx::query("UPDATE games SET is_locked = true WHERE game_code = ?")
            .bind(&game_code)
            .execute(&state.db)
            .await?;
        game.is_locked = true;
    }

    state
        .pubsub
        .host_actions
        .publish(HostAction {
            game_code: game_code.clone(),
            typ: HostActionType::Lock,
        })
        .await?;

    let items: Vec<GameItemWithGuessCount> = sqlx::query_as(
        r#"
SELECT *
FROM game_items
    LEFT OUTER JOIN (
        SELECT item_id, COUNT(*) AS guess_count
        FROM player_guesses
        WHERE
            game_code = ? AND
            outcome_id IS NULL
        GROUP BY item_id
    ) AS guess_counts ON guess_counts.item_id = game_items.game_item_id
WHERE
    game_code = ?
            "#,
    )
    .bind(&game_code)
    .bind(&game_code)
    .fetch_all(&state.db)
    .await?;

    let players_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM game_players WHERE game_code = ?")
            .bind(&game_code)
            .fetch_optional(&state.db)
            .await?
            .unwrap_or(0);

    let drops_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM game_item_outcomes WHERE game_code = ?")
            .bind(&game_code)
            .fetch_optional(&state.db)
            .await?
            .unwrap_or(0);

    let lead_points: Option<Option<i32>> =
        sqlx::query_scalar("SELECT MAX(points) FROM game_players WHERE game_code = ?")
            .bind(&game_code)
            .fetch_optional(&state.db)
            .await?;
    let lead_points = lead_points.flatten().unwrap_or(0);

    let leaders: Vec<String> =
        sqlx::query_scalar("SELECT users.username FROM game_players INNER JOIN users ON users.user_id = game_players.user_id WHERE game_code = ? AND points = ?")
            .bind(&game_code)
            .bind(&lead_points)
            .fetch_all(&state.db)
            .await?;
    let leaders = leaders.join(", ");

    return Ok(Html(GameAsHostBoardTemplate {
        img_base_uri: state.cfg.r2_bucket_public_url.clone(),
        game,
        any_guesses: items
            .iter()
            .any(|item| item.guess_count.map(|c| c > 0).unwrap_or(false)),
        items,
        players_count,
        drops_count,
        lead_points,
        leaders,
    })
    .into_response());
}

async fn game_x_unlock(
    Path(game_code): Path<String>,
    session: Session,
    State(state): State<AppState>,
) -> Result<Response> {
    let session_id = utils::session_id(&session)?;
    let (user, _) = utils::require_user(&state, &session_id).await?.split();

    if game_code.trim().is_empty() {
        return Err(anyhow::anyhow!("Missing game_code"))?;
    }
    let game_code = game_code.to_lowercase();

    let game: Option<Game> = sqlx::query_as(
        "SELECT * FROM games WHERE game_code = ? AND user_id = ? AND status = 'ACTIVE' LIMIT 1",
    )
    .bind(&game_code)
    .bind(&user.user_id)
    .fetch_optional(&state.db)
    .await?;

    let Some(mut game) = game else {
        return Err(anyhow::anyhow!("Game not found"))?;
    };

    if game.is_locked {
        sqlx::query("UPDATE games SET is_locked = false WHERE game_code = ?")
            .bind(&game_code)
            .execute(&state.db)
            .await?;
        game.is_locked = false;
    }

    state
        .pubsub
        .host_actions
        .publish(HostAction {
            game_code: game_code.clone(),
            typ: HostActionType::Unlock,
        })
        .await?;

    let items: Vec<GameItemWithGuessCount> = sqlx::query_as(
        r#"
SELECT *
FROM game_items
    LEFT OUTER JOIN (
        SELECT item_id, COUNT(*) AS guess_count
        FROM player_guesses
        WHERE
            game_code = ? AND
            outcome_id IS NULL
        GROUP BY item_id
    ) AS guess_counts ON guess_counts.item_id = game_items.game_item_id
WHERE
    game_code = ?
            "#,
    )
    .bind(&game_code)
    .bind(&game_code)
    .fetch_all(&state.db)
    .await?;

    let players_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM game_players WHERE game_code = ?")
            .bind(&game_code)
            .fetch_optional(&state.db)
            .await?
            .unwrap_or(0);

    let drops_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM game_item_outcomes WHERE game_code = ?")
            .bind(&game_code)
            .fetch_optional(&state.db)
            .await?
            .unwrap_or(0);

    let lead_points: Option<Option<i32>> =
        sqlx::query_scalar("SELECT MAX(points) FROM game_players WHERE game_code = ?")
            .bind(&game_code)
            .fetch_optional(&state.db)
            .await?;
    let lead_points = lead_points.flatten().unwrap_or(0);

    let leaders: Vec<String> =
        sqlx::query_scalar("SELECT users.username FROM game_players INNER JOIN users ON users.user_id = game_players.user_id WHERE game_code = ? AND points = ?")
            .bind(&game_code)
            .bind(&lead_points)
            .fetch_all(&state.db)
            .await?;
    let leaders = leaders.join(", ");

    return Ok(Html(GameAsHostBoardTemplate {
        img_base_uri: state.cfg.r2_bucket_public_url.clone(),
        game,
        any_guesses: items
            .iter()
            .any(|item| item.guess_count.map(|c| c > 0).unwrap_or(false)),
        items,
        players_count,
        drops_count,
        leaders,
        lead_points,
    })
    .into_response());
}

async fn game_x_choose_item(
    Path((game_code, game_item_id)): Path<(String, u64)>,
    session: Session,
    State(state): State<AppState>,
) -> Result<Response> {
    let session_id = utils::session_id(&session)?;
    let (user, _) = utils::require_user(&state, &session_id).await?.split();

    if game_code.trim().is_empty() {
        return Err(anyhow::anyhow!("Missing game_code"))?;
    }
    let game_code = game_code.to_lowercase();

    let game: Option<Game> = sqlx::query_as(
        "SELECT * FROM games WHERE game_code = ? AND user_id = ? AND status = 'ACTIVE' LIMIT 1",
    )
    .bind(&game_code)
    .bind(&user.user_id)
    .fetch_optional(&state.db)
    .await?;

    let Some(mut game) = game else {
        return Err(anyhow::anyhow!("Game not found"))?;
    };

    let game_item: Option<GameItem> =
        sqlx::query_as("SELECT * FROM game_items WHERE game_code = ? AND game_item_id = ? LIMIT 1")
            .bind(&game_code)
            .bind(&game_item_id)
            .fetch_optional(&state.db)
            .await?;

    let Some(game_item) = game_item else {
        return Err(anyhow::anyhow!("Item not found"))?;
    };

    if !game_item.enabled {
        return Err(anyhow::anyhow!("Item is disabled"))?;
    }

    sqlx::query("INSERT INTO game_item_outcomes (game_code, item_id) VALUES (?, ?)")
        .bind(&game_code)
        .bind(&game_item_id)
        .execute(&state.db)
        .await?;

    let outcome: GameItemOutcome = sqlx::query_as(
        "SELECT * FROM game_item_outcomes WHERE item_id = ? ORDER BY outcome_id DESC LIMIT 1",
    )
    .bind(&game_item_id)
    .fetch_one(&state.db)
    .await?;

    let correct_guesses: Vec<(u64, String, String)> = sqlx::query_as(
        "
SELECT game_players.game_player_id, users.username, game_items.name
FROM player_guesses
INNER JOIN game_players ON player_guesses.player_id = game_players.game_player_id
INNER JOIN users ON game_players.user_id = users.user_id
INNER JOIN game_items ON player_guesses.item_id = game_items.game_item_id
WHERE
    player_guesses.game_code = ? AND
    player_guesses.item_id = ? AND
    player_guesses.outcome_id IS NULL
",
    )
    .bind(&game_code)
    .bind(&game_item_id)
    .fetch_all(&state.db)
    .await?;

    if !correct_guesses.is_empty() {
        let correct_guess_ids: HashSet<u64> = correct_guesses.iter().map(|x| x.0).collect();

        let q = format!(
            "UPDATE game_players SET points = points + 1 WHERE game_player_id IN ({})",
            correct_guess_ids
                .iter()
                .map(|_| "?")
                .collect::<Vec<_>>()
                .join(", ")
        );

        let mut query = sqlx::query(&q);

        for id in correct_guess_ids {
            query = query.bind(id);
        }

        query.execute(&state.db).await?;

        if let Some(template_message) = &game.reward_message {
            let mut messages = vec![];

            for (_, username, item_name) in correct_guesses {
                let msg = template_message
                    .clone()
                    .replace("<USER>", &username)
                    .replace("<ITEM>", &item_name);

                messages.push(msg);
            }

            let values = messages
                .iter()
                .map(|_| "(?, ?, NULL, false)")
                .collect::<Vec<_>>()
                .join(", ");

            let q = format!(
                "INSERT INTO chat_messages (game_code, message, lock_id, sent) VALUES {values}"
            );

            let mut q = sqlx::query(&q);

            for msg in messages {
                q = q.bind(&game.game_code).bind(msg);
            }

            q.execute(&state.db).await?;
        }
    }

    sqlx::query(
        "UPDATE player_guesses SET outcome_id = ? WHERE game_code = ? AND outcome_id IS NULL",
    )
    .bind(&outcome.outcome_id)
    .bind(&game_code)
    .execute(&state.db)
    .await?;

    sqlx::query("UPDATE game_items SET enabled = false WHERE game_code = ? AND game_item_id = ?")
        .bind(&game_code)
        .bind(&game_item_id)
        .execute(&state.db)
        .await?;

    if game.is_locked != game.auto_lock {
        sqlx::query("UPDATE games SET is_locked = ? WHERE game_code = ?")
            .bind(&game.auto_lock)
            .bind(&game_code)
            .execute(&state.db)
            .await?;
        game.is_locked = game.auto_lock;
    }

    state
        .pubsub
        .host_actions
        .publish(HostAction {
            game_code: game_code.clone(),
            typ: HostActionType::Choose {
                item_id: game_item_id.clone(),
            },
        })
        .await?;

    let items: Vec<GameItemWithGuessCount> = sqlx::query_as(
        r#"
SELECT *
FROM game_items
    LEFT OUTER JOIN (
        SELECT item_id, COUNT(*) AS guess_count
        FROM player_guesses
        WHERE
            game_code = ? AND
            outcome_id IS NULL
        GROUP BY item_id
    ) AS guess_counts ON guess_counts.item_id = game_items.game_item_id
WHERE
    game_code = ?
            "#,
    )
    .bind(&game_code)
    .bind(&game_code)
    .fetch_all(&state.db)
    .await?;

    let players_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM game_players WHERE game_code = ?")
            .bind(&game_code)
            .fetch_optional(&state.db)
            .await?
            .unwrap_or(0);

    let drops_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM game_item_outcomes WHERE game_code = ?")
            .bind(&game_code)
            .fetch_optional(&state.db)
            .await?
            .unwrap_or(0);

    let lead_points: Option<Option<i32>> =
        sqlx::query_scalar("SELECT MAX(points) FROM game_players WHERE game_code = ?")
            .bind(&game_code)
            .fetch_optional(&state.db)
            .await?;
    let lead_points = lead_points.flatten().unwrap_or(0);

    let leaders: Vec<String> =
        sqlx::query_scalar("SELECT users.username FROM game_players INNER JOIN users ON users.user_id = game_players.user_id WHERE game_code = ? AND points = ?")
            .bind(&game_code)
            .bind(&lead_points)
            .fetch_all(&state.db)
            .await?;
    let leaders = leaders.join(", ");

    return Ok(Html(GameAsHostBoardTemplate {
        img_base_uri: state.cfg.r2_bucket_public_url.clone(),
        game,
        any_guesses: items
            .iter()
            .any(|item| item.guess_count.map(|c| c > 0).unwrap_or(false)),
        items,
        players_count,
        drops_count,
        leaders,
        lead_points,
    })
    .into_response());
}

#[derive(Template, Clone)]
#[template(path = "game-as-player-board.html")]
struct GameAsPlayerBoardTemplate {
    game: Game,
    host: User,
    guess: Option<PlayerGuess>,
    items: Vec<GameItem>,
    player: GamePlayer,
    drops_count: i64,
    img_base_uri: String,
}

async fn game_x_guess_item(
    Path((game_code, game_item_id)): Path<(String, u64)>,
    session: Session,
    State(state): State<AppState>,
) -> Result<Response> {
    let session_id = utils::session_id(&session)?;
    let (user, _) = utils::require_user(&state, &session_id).await?.split();

    if game_code.trim().is_empty() {
        return Err(anyhow::anyhow!("Missing game_code"))?;
    }
    let game_code = game_code.to_lowercase();

    let game: Option<Game> = sqlx::query_as(
        "SELECT * FROM games WHERE game_code = ? AND user_id != ? AND status = 'ACTIVE' LIMIT 1",
    )
    .bind(&game_code)
    .bind(&user.user_id)
    .fetch_optional(&state.db)
    .await?;

    let Some(game) = game else {
        return Err(anyhow::anyhow!("Game not found"))?;
    };

    let game_item: Option<GameItem> =
        sqlx::query_as("SELECT * FROM game_items WHERE game_code = ? AND game_item_id = ? LIMIT 1")
            .bind(&game_code)
            .bind(&game_item_id)
            .fetch_optional(&state.db)
            .await?;

    let Some(game_item) = game_item else {
        return Err(anyhow::anyhow!("Item not found"))?;
    };

    let game_player: Option<GamePlayer> =
        sqlx::query_as("SELECT * FROM game_players WHERE game_code = ? AND user_id = ? LIMIT 1")
            .bind(&game_code)
            .bind(&user.user_id)
            .fetch_optional(&state.db)
            .await?;

    let Some(game_player) = game_player else {
        return Err(anyhow::anyhow!("Player not found"))?;
    };

    let guess: Option<PlayerGuess> = sqlx::query_as("SELECT * FROM player_guesses WHERE game_code = ? AND player_id = ? AND outcome_id IS NULL LIMIT 1")
        .bind(&game_code)
        .bind(&game_player.game_player_id)
        .fetch_optional(&state.db)
        .await?;

    let guess = if let Some(mut guess) = guess {
        sqlx::query("UPDATE player_guesses SET item_id = ? WHERE game_code = ? AND player_id = ? AND outcome_id IS NULL LIMIT 1")
            .bind(&game_item.game_item_id)
            .bind(&game_code)
            .bind(&game_player.game_player_id)
            .execute(&state.db)
            .await?;

        let from_new_guess_count = sqlx::query_scalar("SELECT COUNT(*) FROM player_guesses WHERE game_code = ? AND item_id = ? AND outcome_id IS NULL")
            .bind(&game_code)
            .bind(&guess.item_id)
            .fetch_optional(&state.db)
            .await?
            .unwrap_or(0);

        let to_new_guess_count = sqlx::query_scalar("SELECT COUNT(*) FROM player_guesses WHERE game_code = ? AND item_id = ? AND outcome_id IS NULL")
            .bind(&game_code)
            .bind(&game_item.game_item_id)
            .fetch_optional(&state.db)
            .await?
            .unwrap_or(0);

        state
            .pubsub
            .player_actions
            .publish(PlayerAction {
                game_code: game_code.clone(),
                user_id: user.user_id.clone(),
                typ: PlayerActionType::UndoGuess {
                    item_id: guess.item_id.clone(),
                    new_guess_count: from_new_guess_count,
                },
            })
            .await?;

        state
            .pubsub
            .player_actions
            .publish(PlayerAction {
                game_code: game_code.clone(),
                user_id: user.user_id.clone(),
                typ: PlayerActionType::Guess {
                    item_id: game_item.game_item_id.clone(),
                    new_guess_count: to_new_guess_count,
                },
            })
            .await?;

        guess.item_id = game_item.game_item_id;

        guess
    } else {
        sqlx::query("INSERT INTO player_guesses (game_code, player_id, item_id, outcome_id) VALUES (?, ?, ?, ?)")
            .bind(&game_code)
            .bind(&game_player.game_player_id)
            .bind(&game_item.game_item_id)
            .bind(None as Option<u64>)
            .execute(&state.db)
            .await?;

        let new_guess_count = sqlx::query_scalar("SELECT COUNT(*) FROM player_guesses WHERE game_code = ? AND item_id = ? AND outcome_id IS NULL")
            .bind(&game_code)
            .bind(&game_item.game_item_id)
            .fetch_optional(&state.db)
            .await?
            .unwrap_or(1);

        state
            .pubsub
            .player_actions
            .publish(PlayerAction {
                game_code: game_code.clone(),
                user_id: user.user_id.clone(),
                typ: PlayerActionType::Guess {
                    item_id: game_item.game_item_id.clone(),
                    new_guess_count,
                },
            })
            .await?;

        if new_guess_count == 1 {
            state
                .pubsub
                .player_actions
                .publish(PlayerAction {
                    game_code: game_code.clone(),
                    user_id: user.user_id.clone(),
                    typ: PlayerActionType::EnableClearGuesses,
                })
                .await?;
        }

        let guess: PlayerGuess = sqlx::query_as("SELECT * FROM player_guesses WHERE game_code = ? AND player_id = ? AND item_id = ? AND outcome_id IS NULL LIMIT 1")
            .bind(&game_code)
            .bind(&game_player.game_player_id)
            .bind(&game_item.game_item_id)
            .fetch_one(&state.db)
            .await?;

        guess
    };

    let items = sqlx::query_as("SELECT * FROM game_items WHERE game_code = ?")
        .bind(&game_code)
        .fetch_all(&state.db)
        .await?;

    let host = sqlx::query_as("SELECT * FROM users WHERE user_id = ?")
        .bind(&game.user_id)
        .fetch_one(&state.db)
        .await?;

    let drops_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM game_item_outcomes WHERE game_code = ?")
            .bind(&game_code)
            .fetch_optional(&state.db)
            .await?
            .unwrap_or(0);

    return Ok(Html(GameAsPlayerBoardTemplate {
        game,
        host,
        guess: Some(guess),
        items,
        player: game_player,
        drops_count,
        img_base_uri: state.cfg.r2_bucket_public_url.clone(),
    })
    .into_response());
}

async fn game_x_clear_guesses(
    Path(game_code): Path<String>,
    session: Session,
    State(state): State<AppState>,
) -> Result<Response> {
    let session_id = utils::session_id(&session)?;
    let (user, _) = utils::require_user(&state, &session_id).await?.split();

    if game_code.trim().is_empty() {
        return Err(anyhow::anyhow!("Missing game_code"))?;
    }
    let game_code = game_code.to_lowercase();

    let game: Option<Game> = sqlx::query_as(
        "SELECT * FROM games WHERE game_code = ? AND user_id = ? AND status = 'ACTIVE' LIMIT 1",
    )
    .bind(&game_code)
    .bind(&user.user_id)
    .fetch_optional(&state.db)
    .await?;

    let Some(_game) = game else {
        return Err(anyhow::anyhow!("Game not found"))?;
    };

    sqlx::query("DELETE FROM player_guesses WHERE game_code = ? AND outcome_id IS NULL")
        .bind(&game_code)
        .execute(&state.db)
        .await?;

    state
        .pubsub
        .host_actions
        .publish(HostAction {
            game_code: game_code.clone(),
            typ: HostActionType::ClearGuesses,
        })
        .await?;

    return Ok(Redirect::to(&format!("/games/{game_code}")).into_response());
}

#[derive(Template, Clone)]
#[template(path = "game-as-host-item.html")]
struct GameAsHostItemTemplate {
    game: Game,
    item: GameItemWithGuessCount,
    img_base_uri: String,
}

async fn game_x_enable_item(
    Path((game_code, game_item_id)): Path<(String, u64)>,
    session: Session,
    State(state): State<AppState>,
) -> Result<Response> {
    let session_id = utils::session_id(&session)?;
    let (user, _) = utils::require_user(&state, &session_id).await?.split();

    if game_code.trim().is_empty() {
        return Err(anyhow::anyhow!("Missing game_code"))?;
    }
    let game_code = game_code.to_lowercase();

    let game: Option<Game> = sqlx::query_as(
        "SELECT * FROM games WHERE game_code = ? AND user_id = ? AND status = 'ACTIVE' LIMIT 1",
    )
    .bind(&game_code)
    .bind(&user.user_id)
    .fetch_optional(&state.db)
    .await?;

    let Some(game) = game else {
        return Err(anyhow::anyhow!("Game not found"))?;
    };

    let game_item: Option<GameItemWithGuessCount> = sqlx::query_as(
        r#"
SELECT *
FROM game_items
    LEFT OUTER JOIN (
        SELECT item_id, COUNT(*) AS guess_count
        FROM player_guesses
        WHERE
            game_code = ? AND
            outcome_id IS NULL
        GROUP BY item_id
        HAVING item_id = ?
    ) AS guess_counts ON guess_counts.item_id = game_items.game_item_id
WHERE
    game_code = ? AND
    game_item_id = ?
LIMIT 1
            "#,
    )
    .bind(&game_code)
    .bind(&game_item_id)
    .bind(&game_code)
    .bind(&game_item_id)
    .fetch_optional(&state.db)
    .await?;

    let Some(mut game_item) = game_item else {
        return Err(anyhow::anyhow!("Item not found"))?;
    };

    if !game_item.enabled {
        sqlx::query(
            "UPDATE game_items SET enabled = true WHERE game_code = ? AND game_item_id = ?",
        )
        .bind(&game_code)
        .bind(&game_item_id)
        .execute(&state.db)
        .await?;

        game_item.enabled = true;

        state
            .pubsub
            .host_actions
            .publish(HostAction {
                game_code: game_code.clone(),
                typ: HostActionType::Enable {
                    item_id: game_item_id.clone(),
                },
            })
            .await?;
    }

    return Ok(Html(GameAsHostItemTemplate {
        img_base_uri: state.cfg.r2_bucket_public_url.clone(),
        game,
        item: game_item,
    })
    .into_response());
}

async fn game_x_disable_item(
    Path((game_code, game_item_id)): Path<(String, u64)>,
    session: Session,
    State(state): State<AppState>,
) -> Result<Response> {
    let session_id = utils::session_id(&session)?;
    let (user, _) = utils::require_user(&state, &session_id).await?.split();

    if game_code.trim().is_empty() {
        return Err(anyhow::anyhow!("Missing game_code"))?;
    }
    let game_code = game_code.to_lowercase();

    let game: Option<Game> = sqlx::query_as(
        "SELECT * FROM games WHERE game_code = ? AND user_id = ? AND status = 'ACTIVE' LIMIT 1",
    )
    .bind(&game_code)
    .bind(&user.user_id)
    .fetch_optional(&state.db)
    .await?;

    let Some(game) = game else {
        return Err(anyhow::anyhow!("Game not found"))?;
    };

    let game_item: Option<GameItemWithGuessCount> = sqlx::query_as(
        r#"
SELECT *
FROM game_items
    LEFT OUTER JOIN (
        SELECT item_id, COUNT(*) AS guess_count
        FROM player_guesses
        WHERE
            game_code = ? AND
            outcome_id IS NULL
        GROUP BY item_id
        HAVING item_id = ?
    ) AS guess_counts ON guess_counts.item_id = game_items.game_item_id
WHERE
    game_code = ? AND
    game_item_id = ?
LIMIT 1
            "#,
    )
    .bind(&game_code)
    .bind(&game_item_id)
    .bind(&game_code)
    .bind(&game_item_id)
    .fetch_optional(&state.db)
    .await?;

    let Some(mut game_item) = game_item else {
        return Err(anyhow::anyhow!("Item not found"))?;
    };

    if game_item.enabled {
        sqlx::query(
            "UPDATE game_items SET enabled = false WHERE game_code = ? AND game_item_id = ?",
        )
        .bind(&game_code)
        .bind(&game_item_id)
        .execute(&state.db)
        .await?;

        game_item.enabled = false;

        state
            .pubsub
            .host_actions
            .publish(HostAction {
                game_code: game_code.clone(),
                typ: HostActionType::Disable {
                    item_id: game_item_id.clone(),
                },
            })
            .await?;
    }

    return Ok(Html(GameAsHostItemTemplate {
        img_base_uri: state.cfg.r2_bucket_public_url.clone(),
        game,
        item: game_item,
    })
    .into_response());
}

async fn finish_game(
    Path(game_code): Path<String>,
    session: Session,
    State(state): State<AppState>,
) -> Result<Response> {
    let session_id = utils::session_id(&session)?;
    let (user, _) = utils::require_user(&state, &session_id).await?.split();

    if game_code.trim().is_empty() {
        return Err(anyhow::anyhow!("Missing game_code"))?;
    }
    let game_code = game_code.to_lowercase();

    let game: Option<Game> = sqlx::query_as(
        "SELECT * FROM games WHERE game_code = ? AND user_id = ? AND status = ? LIMIT 1",
    )
    .bind(&game_code)
    .bind(&user.user_id)
    .bind(GAME_STATUS_ACTIVE)
    .fetch_optional(&state.db)
    .await?;

    let Some(mut game) = game else {
        return Err(anyhow::anyhow!("Game not found"))?;
    };

    if game.status != GAME_STATUS_ACTIVE {
        return Err(anyhow::anyhow!("Game is not active"))?;
    }

    sqlx::query("UPDATE games SET status = ? WHERE game_code = ?")
        .bind(GAME_STATUS_FINISHED)
        .bind(&game.game_code)
        .execute(&state.db)
        .await?;
    game.status = GAME_STATUS_FINISHED.to_string();

    let winners: Vec<(u64, i32, String)> = sqlx::query_as(
        r#"
SELECT game_players.game_player_id, game_players.points, users.username
FROM game_players
    INNER JOIN users ON users.user_id = game_players.user_id
WHERE
    game_players.game_code = ? AND
    points != 0 AND
    points = (
        SELECT MAX(points)
        FROM game_players
        WHERE game_players.game_code = ?
    )
"#,
    )
    .bind(&game_code)
    .bind(&game_code)
    .fetch_all(&state.db)
    .await?;

    if !winners.is_empty() {
        let values = winners
            .iter()
            .map(|_| "(?, ?)")
            .collect::<Vec<_>>()
            .join(", ");

        let q = format!("INSERT INTO game_winners (game_player_id, game_code) VALUES {values}");
        let mut query = sqlx::query(&q);

        for (game_player_id, _, _) in &winners {
            query = query.bind(game_player_id).bind(&game_code);
        }

        query.execute(&state.db).await?;

        if let Some(template_message) = &game.total_reward_message {
            let total: i64 =
                sqlx::query_scalar("SELECT COUNT(*) FROM game_item_outcomes WHERE game_code = ?")
                    .bind(&game_code)
                    .fetch_optional(&state.db)
                    .await?
                    .unwrap_or(0);

            let mut messages = vec![];

            for (_, points, username) in winners {
                let msg = template_message
                    .clone()
                    .replace("<USER>", &username)
                    .replace("<POINTS>", &points.to_string())
                    .replace("<TOTAL>", &total.to_string());

                messages.push(msg);
            }

            let values = messages
                .iter()
                .map(|_| "(?, ?, NULL, false)")
                .collect::<Vec<_>>()
                .join(", ");

            let q = format!(
                "INSERT INTO chat_messages (game_code, message, lock_id, sent) VALUES {values}"
            );

            let mut q = sqlx::query(&q);

            for msg in messages {
                q = q.bind(&game.game_code).bind(msg);
            }

            q.execute(&state.db).await?;
        }
    }

    state
        .pubsub
        .host_actions
        .publish(HostAction {
            game_code: game_code.clone(),
            typ: HostActionType::Finish,
        })
        .await?;

    return Ok(Redirect::to(&format!("/games/{game_code}")).into_response());
}

#[derive(Template)]
#[template(
    source = r#"
{% if let 0 = guess_count %}
{% else if let 1 = guess_count %}
    <span class="badge">1 guess</span>
{% else %}
    <span class="badge">{{ guess_count }} guesses</span>
{% endif %}
"#,
    ext = "html"
)]
struct GuessCountTemplate {
    guess_count: i32,
}

async fn host_sse(
    Path(game_code): Path<String>,
    session: Session,
    State(state): State<AppState>,
) -> Result<Response> {
    let session_id = utils::session_id(&session)?;
    let (user, _) = utils::require_user(&state, &session_id).await?.split();

    if game_code.trim().is_empty() {
        return Err(anyhow::anyhow!("Missing game_code"))?;
    }
    let game_code = game_code.to_lowercase();

    let game: Option<Game> = sqlx::query_as(
        "SELECT * FROM games WHERE game_code = ? AND user_id = ? AND status = ? LIMIT 1",
    )
    .bind(&game_code)
    .bind(&user.user_id)
    .bind(GAME_STATUS_ACTIVE)
    .fetch_optional(&state.db)
    .await?;

    let Some(game) = game else {
        return Err(anyhow::anyhow!("Game not found"))?;
    };

    if game.status != GAME_STATUS_ACTIVE {
        return Err(anyhow::anyhow!("Game is not active"))?;
    }

    let guard = state.game_broadcasts.read().unwrap();
    let game_broadcasts = &*guard;

    let broadcast = match game_broadcasts.get(&game_code) {
        Some(broadcast) => broadcast.clone(),
        None => {
            drop(guard);

            let broadcasts = &mut *state.game_broadcasts.write().unwrap();

            broadcasts
                .entry(game_code.clone())
                .or_insert_with(|| {
                    let (host_tx, _host_rx) = broadcast::channel(16);
                    let (players_tx, _players_rx) = broadcast::channel(16);

                    return GameBroadcast {
                        to_host: host_tx,
                        to_players: players_tx,
                    };
                })
                .clone()
        }
    };

    let rx = broadcast.to_host.subscribe();

    let stream = BroadcastStream::new(rx).map(move |event| -> Result<Event> {
        let event = event?;

        match &event.typ {
            PlayerActionType::EnableClearGuesses => {
                return Ok(Event::default()
                    .event("enable_clear_guesses")
                    .data(format!(r##"<button hx-put="/games/{}/x/clear-guesses" hx-disabled-elt="this" hx-indicator="#clear_ind" class="btn btn-ghost sm:btn-lg lg:btn-md">Clear guesses</button>"##, event.game_code)))
            }

            PlayerActionType::Join {
                new_players_count: player_count,
            } => {
                return Ok(Event::default()
                    .event("players_count")
                    .data(player_count.to_string()))
            }

            PlayerActionType::Guess {
                item_id,
                new_guess_count,
            } => {
                let data = GuessCountTemplate {
                    guess_count: *new_guess_count,
                }
                .render()?;

                return Ok(Event::default()
                    .event(format!("guesses_{item_id}"))
                    .data(data));
            }

            PlayerActionType::UndoGuess {
                item_id,
                new_guess_count,
            } => {
                let data = GuessCountTemplate {
                    guess_count: *new_guess_count,
                }
                .render()?;

                return Ok(Event::default()
                    .event(format!("guesses_{item_id}"))
                    .data(data));
            }
        }
    });

    return Ok(Sse::new(stream)
        .keep_alive(
            KeepAlive::new()
                .interval(std::time::Duration::from_secs(3))
                .text("keep-alive"),
        )
        .into_response());
}

async fn player_sse(
    Path(game_code): Path<String>,
    session: Session,
    State(state): State<AppState>,
) -> Result<Response> {
    let session_id = utils::session_id(&session)?;
    let (user, _) = utils::require_user(&state, &session_id).await?.split();

    if game_code.trim().is_empty() {
        return Ok(Redirect::to("/").into_response());
    }
    let game_code = game_code.to_lowercase();

    let game: Option<Game> = sqlx::query_as(
        "SELECT * FROM games WHERE game_code = ? AND user_id != ? AND status = ? LIMIT 1",
    )
    .bind(&game_code)
    .bind(&user.user_id)
    .bind(GAME_STATUS_ACTIVE)
    .fetch_optional(&state.db)
    .await?;

    let Some(game) = game else {
        return Ok(Redirect::to("/").into_response());
    };

    if game.status != GAME_STATUS_ACTIVE {
        return Ok(Redirect::to(&format!("/games/{game_code}")).into_response());
    }

    let game_player: Option<GamePlayer> =
        sqlx::query_as("SELECT * FROM game_players WHERE game_code = ? AND user_id = ? LIMIT 1")
            .bind(&game_code)
            .bind(&user.user_id)
            .fetch_optional(&state.db)
            .await?;

    let Some(_) = game_player else {
        return Ok(Redirect::to(&format!("/games/{game_code}")).into_response());
    };

    let guard = state.game_broadcasts.read().unwrap();
    let game_broadcasts = &*guard;

    let broadcast = match game_broadcasts.get(&game_code) {
        Some(broadcast) => broadcast.clone(),
        None => {
            drop(guard);

            let broadcasts = &mut *state.game_broadcasts.write().unwrap();

            broadcasts
                .entry(game_code.clone())
                .or_insert_with(|| {
                    let (host_tx, _host_rx) = broadcast::channel(16);
                    let (players_tx, _players_rx) = broadcast::channel(16);

                    return GameBroadcast {
                        to_host: host_tx,
                        to_players: players_tx,
                    };
                })
                .clone()
        }
    };

    let rx = broadcast.to_players.subscribe();

    let stream = BroadcastStream::new(rx).map(|event| -> Result<Event> {
        let event = event?;

        match &event.typ {
            HostActionType::Finish => {
                return Ok(Event::default().event(format!("force_refresh")).data(""))
            }

            // HostActionType::Disable { item_id } => {
            //     return Ok(Event::default()
            //         .event(format!("update_item_{item_id}"))
            //         .data(""))
            // }
            HostActionType::Lock
            | HostActionType::Unlock
            | HostActionType::ClearGuesses
            | HostActionType::Enable { .. }
            | HostActionType::Disable { .. }
            | HostActionType::Choose { .. } => {
                return Ok(Event::default().event("host_action").data(""))
            }
        }
    });

    return Ok(Sse::new(stream)
        .keep_alive(
            KeepAlive::new()
                .interval(std::time::Duration::from_secs(3))
                .text("keep-alive"),
        )
        .into_response());
}
