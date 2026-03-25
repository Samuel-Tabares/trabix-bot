pub mod models;
pub mod queries;

use sqlx::{postgres::PgPoolOptions, PgPool};

const POSTGRES_SESSION_TIMEZONE: &str = "America/Bogota";

pub async fn init_pool(database_url: &str) -> Result<PgPool, sqlx::Error> {
    PgPoolOptions::new()
        .max_connections(5)
        .after_connect(|conn, _meta| {
            Box::pin(async move {
                let statement = format!("SET TIME ZONE '{POSTGRES_SESSION_TIMEZONE}'");
                sqlx::query(&statement).execute(conn).await?;
                Ok(())
            })
        })
        .connect(database_url)
        .await
}

#[cfg(test)]
mod tests {
    use super::POSTGRES_SESSION_TIMEZONE;

    #[test]
    fn uses_bogota_postgres_session_timezone() {
        assert_eq!(POSTGRES_SESSION_TIMEZONE, "America/Bogota");
    }
}
