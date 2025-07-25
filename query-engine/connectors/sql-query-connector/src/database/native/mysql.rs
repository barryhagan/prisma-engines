use crate::database::{catch, connection::SqlConnection};
use crate::{FromSource, SqlError};
use async_trait::async_trait;
use connector_interface::{
    self as connector, Connection, Connector,
    error::{ConnectorError, ErrorKind},
};
use quaint::connector::Queryable;
use quaint::{pooled::Quaint, prelude::ConnectionInfo};
use std::time::Duration;

pub struct Mysql {
    pool: Quaint,
    connection_info: ConnectionInfo,
    features: psl::PreviewFeatures,
}

impl Mysql {
    /// Get MySQL's preview features.
    pub fn features(&self) -> psl::PreviewFeatures {
        self.features
    }
}

fn get_connection_info(url: &str) -> connector::Result<ConnectionInfo> {
    let database_str = url;

    let connection_info = ConnectionInfo::from_url(database_str).map_err(|err| {
        ConnectorError::from_kind(ErrorKind::InvalidDatabaseUrl {
            details: err.to_string(),
            url: database_str.to_string(),
        })
    })?;

    Ok(connection_info)
}

#[async_trait]
impl FromSource for Mysql {
    async fn from_source(
        _: &psl::Datasource,
        url: &str,
        features: psl::PreviewFeatures,
        _tracing_enabled: bool,
    ) -> connector_interface::Result<Mysql> {
        let connection_info = get_connection_info(url)?;

        let mut builder = Quaint::builder(url)
            .map_err(SqlError::from)
            .map_err(|sql_error| sql_error.into_connector_error(connection_info.as_native()))?;

        builder.health_check_interval(Duration::from_secs(15));
        builder.test_on_check_out(true);

        let pool = builder.build();
        let connection_info = pool.connection_info().to_owned();

        Ok(Mysql {
            pool,
            connection_info,
            features: features.to_owned(),
        })
    }
}

#[async_trait]
impl Connector for Mysql {
    async fn get_connection<'a>(&'a self) -> connector::Result<Box<dyn Connection + Send + Sync + 'static>> {
        catch(&self.connection_info, async move {
            let runtime_conn = self.pool.check_out().await?;

            // Note: `runtime_conn` must be `Sized`, as that's required by `TransactionCapable`
            let mut conn_info = self.connection_info.clone();
            // MySQL has its version grabbed at connection time. We know it's infallible.
            let db_version = runtime_conn.version().await.unwrap();

            // On Vitess, we allow connecting without a database name to support sharding.
            // Vitess routes queries to the correct MySQL database based on which keyspace
            // each table belongs to and on shard key values.
            // Otherwise, we use `mysql` as the default database.
            if self.connection_info.dbname().is_none() && !db_version.as_ref().is_some_and(|v| v.contains("Vitess")) {
                runtime_conn.execute_raw("USE mysql", &[]).await?;
            }

            conn_info.set_version(db_version);

            let sql_conn = SqlConnection::new(runtime_conn, conn_info, self.features);

            Ok(Box::new(sql_conn) as Box<dyn Connection + Send + Sync + 'static>)
        })
        .await
    }

    fn name(&self) -> &'static str {
        "mysql"
    }

    fn should_retry_on_transient_error(&self) -> bool {
        false
    }
}
