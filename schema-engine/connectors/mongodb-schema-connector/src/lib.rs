//! The MongoDB migration connector.
//!
//! It is intentionally structured after sql-migration-connector and implements the same
//! [MigrationConnector](/trait.MigrationConnector.html) API.

mod client_wrapper;
mod destructive_change_checker;
mod differ;
mod migration;
mod migration_persistence;
mod migration_step_applier;
mod sampler;
mod schema_calculator;

use client_wrapper::{Client, mongo_error_to_connector_error};
use enumflags2::BitFlags;
use migration::MongoDbMigration;
use mongodb_schema_describer::MongoSchema;
use psl::PreviewFeature;
use schema_connector::{migrations_directory::Migrations, *};
use std::{future, sync::Arc};
use tokio::sync::OnceCell;

pub struct MongoDbSchemaDialect;

/// The top-level MongoDB migration connector.
pub struct MongoDbSchemaConnector {
    connection_string: String,
    client: OnceCell<Client>,
    preview_features: BitFlags<PreviewFeature>,
    host: Arc<dyn ConnectorHost>,
}

impl MongoDbSchemaConnector {
    pub fn new(params: ConnectorParams) -> Self {
        Self {
            connection_string: params.connection_string,
            preview_features: params.preview_features,
            client: OnceCell::new(),
            host: Arc::new(EmptyHost),
        }
    }

    async fn client(&self) -> ConnectorResult<&Client> {
        let client: &Client = self
            .client
            .get_or_try_init(move || {
                Box::pin(async move { Client::connect(&self.connection_string, self.preview_features).await })
            })
            .await?;

        Ok(client)
    }
}

impl SchemaDialect for MongoDbSchemaDialect {
    fn diff(&self, from: DatabaseSchema, to: DatabaseSchema, _filter: &SchemaFilter) -> Migration {
        let from: Box<MongoSchema> = from.downcast();
        let to: Box<MongoSchema> = to.downcast();
        Migration::new(differ::diff(from, to))
    }

    fn migration_file_extension(&self) -> &'static str {
        unreachable!("migration_file_extension")
    }

    fn migration_len(&self, migration: &Migration) -> usize {
        migration.downcast_ref::<MongoDbMigration>().steps.len()
    }

    fn migration_summary(&self, migration: &Migration) -> String {
        migration.downcast_ref::<MongoDbMigration>().summary()
    }

    fn render_script(
        &self,
        _migration: &Migration,
        _diagnostics: &DestructiveChangeDiagnostics,
    ) -> ConnectorResult<String> {
        Err(ConnectorError::from_msg(
            "Rendering to a script is not supported on MongoDB.".to_owned(),
        ))
    }

    fn extract_namespaces(&self, _schema: &DatabaseSchema) -> Option<Namespaces> {
        None
    }

    fn empty_database_schema(&self) -> DatabaseSchema {
        DatabaseSchema::new(MongoSchema::default())
    }

    fn schema_from_datamodel(&self, sources: Vec<(String, psl::SourceFile)>) -> ConnectorResult<DatabaseSchema> {
        let validated_schema = psl::parse_schema_multi(&sources).map_err(ConnectorError::new_schema_parser_error)?;
        Ok(DatabaseSchema::new(schema_calculator::calculate(&validated_schema)))
    }

    fn validate_migrations_with_target<'a>(
        &'a mut self,
        _migrations: &'a Migrations,
        _namespaces: Option<Namespaces>,
        _filter: &SchemaFilter,
        _target: ExternalShadowDatabase,
    ) -> BoxFuture<'a, ConnectorResult<()>> {
        Box::pin(future::ready(Ok(())))
    }

    fn schema_from_migrations_with_target<'a>(
        &'a self,
        _migrations: &'a Migrations,
        _namespaces: Option<Namespaces>,
        _filter: &SchemaFilter,
        _target: ExternalShadowDatabase,
    ) -> BoxFuture<'a, ConnectorResult<DatabaseSchema>> {
        Box::pin(async { Err(unsupported_command_error()) })
    }
}

impl SchemaConnector for MongoDbSchemaConnector {
    fn schema_dialect(&self) -> Box<dyn SchemaDialect> {
        Box::new(MongoDbSchemaDialect)
    }

    fn host(&self) -> &Arc<dyn ConnectorHost> {
        &self.host
    }

    fn apply_migration<'a>(&'a mut self, migration: &'a Migration) -> BoxFuture<'a, ConnectorResult<u32>> {
        Box::pin(self.apply_migration_impl(migration))
    }

    fn apply_script(&mut self, _migration_name: &str, _script: &str) -> BoxFuture<ConnectorResult<()>> {
        Box::pin(future::ready(Err(crate::unsupported_command_error())))
    }

    fn connector_type(&self) -> &'static str {
        "mongodb"
    }

    fn create_database(&mut self) -> BoxFuture<'_, ConnectorResult<String>> {
        Box::pin(async {
            let name = self.client().await?.db_name();
            tracing::warn!("MongoDB database will be created on first use.");
            Ok(name.into())
        })
    }

    fn db_execute(&mut self, _script: String) -> BoxFuture<'_, ConnectorResult<()>> {
        Box::pin(future::ready(Err(ConnectorError::from_msg(
            "dbExecute is not supported on MongoDB".to_owned(),
        ))))
    }

    fn ensure_connection_validity(&mut self) -> BoxFuture<'_, ConnectorResult<()>> {
        Box::pin(future::ready(Ok(())))
    }

    fn version(&mut self) -> BoxFuture<'_, schema_connector::ConnectorResult<String>> {
        Box::pin(future::ready(Ok("4 or 5".to_owned())))
    }

    fn drop_database(&mut self) -> BoxFuture<'_, ConnectorResult<()>> {
        Box::pin(async { self.client().await?.drop_database().await })
    }

    fn reset<'a>(
        &'a mut self,
        _soft: bool,
        _namespaces: Option<Namespaces>,
        _filter: &'a SchemaFilter,
    ) -> BoxFuture<'a, schema_connector::ConnectorResult<()>> {
        Box::pin(async { self.client().await?.drop_database().await })
    }

    fn migration_persistence(&mut self) -> &mut dyn schema_connector::MigrationPersistence {
        self
    }

    fn destructive_change_checker(&mut self) -> &mut dyn schema_connector::DestructiveChangeChecker {
        self
    }

    fn acquire_lock(&mut self) -> BoxFuture<'_, ConnectorResult<()>> {
        Box::pin(future::ready(Ok(())))
    }

    fn introspect<'a>(
        &'a mut self,
        ctx: &'a IntrospectionContext,
    ) -> BoxFuture<'a, ConnectorResult<IntrospectionResult>> {
        Box::pin(async move {
            let client = self.client().await?;
            let schema = client.describe().await?;

            sampler::sample(client.database(), schema, ctx)
                .await
                .map_err(mongo_error_to_connector_error)
        })
    }

    fn set_preview_features(&mut self, preview_features: BitFlags<psl::PreviewFeature>) {
        self.preview_features = preview_features;
    }

    fn set_host(&mut self, host: Arc<dyn schema_connector::ConnectorHost>) {
        self.host = host;
    }

    fn validate_migrations<'a>(
        &'a mut self,
        _migrations: &'a Migrations,
        _namespaces: Option<Namespaces>,
        _filter: &SchemaFilter,
    ) -> BoxFuture<'a, ConnectorResult<()>> {
        Box::pin(future::ready(Ok(())))
    }

    fn introspect_sql(
        &mut self,
        _input: IntrospectSqlQueryInput,
    ) -> BoxFuture<'_, ConnectorResult<IntrospectSqlQueryOutput>> {
        unreachable!()
    }

    fn schema_from_database(
        &mut self,
        _namespaces: Option<Namespaces>,
    ) -> BoxFuture<'_, ConnectorResult<DatabaseSchema>> {
        Box::pin(async { self.client().await?.describe().await.map(DatabaseSchema::new) })
    }

    fn schema_from_migrations<'a>(
        &'a mut self,
        _migrations: &'a Migrations,
        _namespaces: Option<Namespaces>,
        _filter: &SchemaFilter,
    ) -> BoxFuture<'a, ConnectorResult<DatabaseSchema>> {
        Box::pin(async { Err(unsupported_command_error()) })
    }

    fn dispose(&mut self) -> BoxFuture<'_, ConnectorResult<()>> {
        Box::pin(async { Ok(()) })
    }
}

fn unsupported_command_error() -> ConnectorError {
    ConnectorError::from_msg(
"The \"mongodb\" provider is not supported with this command. For more info see https://www.prisma.io/docs/concepts/database-connectors/mongodb".to_owned()

        )
}
