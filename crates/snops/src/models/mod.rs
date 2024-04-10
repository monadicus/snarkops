use sqlx::{prelude::FromRow, Pool, Sqlite};
use thiserror::Error;

pub type Db = Pool<Sqlite>;

#[derive(Debug, FromRow)]
pub struct Environment {}

#[derive(Debug, Error)]
pub enum DbError {
    #[error("{0} with id:`{1}` not found")]
    NotFound(&'static str, String),
}
