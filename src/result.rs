use std::fmt::Display;

use anyhow;
use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    BoxError,
};

#[derive(Debug)]
pub struct AppError(pub anyhow::Error);

impl Display for AppError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

pub type Result<T = (), E = AppError> = anyhow::Result<T, E>;

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Something went wrong: {}", self.0),
        )
            .into_response()
    }
}

impl<E> From<E> for AppError
where
    E: Into<anyhow::Error>,
{
    fn from(err: E) -> Self {
        Self(err.into())
    }
}

impl Into<BoxError> for AppError {
    fn into(self) -> BoxError {
        return self.0.into();
    }
}
