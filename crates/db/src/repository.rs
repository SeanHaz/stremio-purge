use anyhow::{Context, Result, anyhow};
use sqlx::{SqlitePool, QueryBuilder};
use std::str::FromStr;
use crate::models::User;
use crate::models::{ SYNC_TV, SYNC_MOVIES, SYNC_SERIES, SYNC_ALL };
use::log::info;

pub async fn init_pool(db_url: &str) -> Result<SqlitePool, sqlx::Error> {
    let connect_options = if db_url.starts_with("sqlite:") {
        sqlx::sqlite::SqliteConnectOptions::from_str(&db_url)?
    } else {
        sqlx::sqlite::SqliteConnectOptions::new()
            .filename(&db_url)
            .create_if_missing(true)
    };

    let pool = SqlitePool::connect_with(connect_options).await?;
    Ok(pool)
}

pub async fn create_tables(pool: &SqlitePool) -> Result<()> {
    sqlx::query!(
        r#"
        CREATE TABLE IF NOT EXISTS users (
            id                      TEXT PRIMARY KEY,
            auth_key                TEXT NOT NULL,
            all_timestamp           INTEGER NOT NULL DEFAULT 0,
            series_timestamp        INTEGER NOT NULL DEFAULT 0,
            movies_timestamp        INTEGER NOT NULL DEFAULT 0,
            tv_timestamp            INTEGER NOT NULL DEFAULT 0,
            config_mask             INTEGER NOT NULL DEFAULT 0
        )
        "#
    )
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn update_timestamps(pool: &SqlitePool, mask: i64, id: &str, timestamp: i64) -> Result<()>{
    if mask == SYNC_ALL {
        update_all_timestamp(&pool, &id, mask).await.context("updating all_timestamp failed")?;
        
        return Ok(());
    }

    let mut query_builder: QueryBuilder<sqlx::Sqlite> = QueryBuilder::new("UPDATE users SET ");
    
    let mut no_changes = true;

    
    if (mask & SYNC_MOVIES) != 0 {
        if !no_changes { query_builder.push(", "); }
        query_builder.push("movies_timestamp = ");
        query_builder.push_bind(timestamp);
        no_changes = false;
    }
    if (mask & SYNC_SERIES) != 0 {
        if !no_changes { query_builder.push(", "); }
        query_builder.push("series_timestamp = ");
        query_builder.push_bind(timestamp);
        no_changes = false;
    }
    if (mask & SYNC_TV) != 0 {
        if !no_changes { query_builder.push(", "); }
        query_builder.push("tv_timestamp = ");
        query_builder.push_bind(timestamp);
        no_changes = false;
    }

    if no_changes {
        return Ok(());
    }

    query_builder.push(" WHERE id = ");
    query_builder.push_bind(id);

    let query = query_builder.build();

    query.execute(pool).await.context("Combined timestamp update failed")?;

    Ok(())
}

pub async fn upsert_user_on_login(
    pool: &SqlitePool,
    id: &str,
    auth_key: &str,
) -> Result<()> {
    sqlx::query!(
        r#"
        INSERT INTO users (id, auth_key, all_timestamp, series_timestamp, movies_timestamp, tv_timestamp, config_mask)
        VALUES ($1, $2, 0, 0, 0, 0, 0)
        ON CONFLICT(id) DO UPDATE SET
            auth_key = $2
        "#,
        id,
        auth_key
    )
    .execute(pool)
    .await?;

    Ok(())
}

pub async fn insert_user(pool: &SqlitePool, user: &User) -> Result<()> {
    sqlx::query!(
        r#"
        INSERT OR REPLACE INTO users 
        (id, auth_key, all_timestamp, series_timestamp, movies_timestamp, tv_timestamp, config_mask)
        VALUES ($1, $2, $3, $4, $5, $6, $7)
        "#,
        user.id,
        user.auth_key,
        user.all_timestamp,
        user.series_timestamp,
        user.movies_timestamp,
        user.tv_timestamp,
        user.config_mask
    )
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn get_all_users(pool: &SqlitePool) -> Result<Vec<User>> {
    let users = sqlx::query_as!(
        User,
        r#"
        SELECT 
            id              as "id!: String", 
            auth_key        as "auth_key!: String",
            all_timestamp   as "all_timestamp!: i64",
            series_timestamp   as "series_timestamp!: i64",
            movies_timestamp   as "movies_timestamp!: i64",
            tv_timestamp   as "tv_timestamp!: i64",
            config_mask          as "config_mask!: i64"
        FROM users
        "#
    )
        .fetch_all(pool)
        .await?;
    Ok(users)
}

pub async fn find_user_by_id(pool: &SqlitePool, id: &str) -> Result<Option<User>> {
    let user = sqlx::query_as!(
        User,
        r#"
        SELECT 
            id              as "id!: String",
            auth_key        as "auth_key!: String",
            all_timestamp   as "all_timestamp!: i64",
            series_timestamp   as "series_timestamp!: i64",
            movies_timestamp   as "movies_timestamp!: i64",
            tv_timestamp   as "tv_timestamp!: i64",
            config_mask          as "config_mask!: i64"
        FROM users 
        WHERE id = $1
        "#,
        id
    )
        .fetch_optional(pool)
        .await?;
    Ok(user)
}

async fn update_all_timestamp(pool: &SqlitePool, id: &str, updated_timestamp: i64) -> Result<()> {
    let rows = sqlx::query!(
        "UPDATE users SET all_timestamp = $1 WHERE id = $2",
        updated_timestamp,
        id
    )
        .execute(pool)
        .await.context("sql error updating all_timestamp")?;

    if rows.rows_affected() == 0 {
        return Err(anyhow!("No user found with id {}", id));
    } else {
        info!("→ Updated all_timestamp for id '{}'", id);
    }
    Ok(())
}
