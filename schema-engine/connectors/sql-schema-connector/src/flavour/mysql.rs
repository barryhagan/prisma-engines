mod connector;
mod destructive_change_checker;
mod renderer;
mod schema_calculator;
mod schema_differ;

use super::{SqlDialect, UsingExternalShadowDb};
use crate::{error::SystemDatabase, flavour::SqlConnector};
use connector::{Connection, shadow_db};
use destructive_change_checker::MysqlDestructiveChangeCheckerFlavour;
use enumflags2::BitFlags;
use indoc::indoc;
use psl::{ValidatedSchema, datamodel_connector, parser_database::ScalarType};
use quaint::connector::MysqlUrl;
use regex::{Regex, RegexSet};
use renderer::MysqlRenderer;
use schema_calculator::MysqlSchemaCalculatorFlavour;
use schema_connector::{
    BoxFuture, ConnectorError, ConnectorParams, ConnectorResult, Namespaces, SchemaFilter,
    migrations_directory::Migrations,
};
use schema_differ::MysqlSchemaDifferFlavour;
use sql_schema_describer::SqlSchema;
use std::{future, sync::LazyLock};
use url::Url;
use versions::Versioning;

const ADVISORY_LOCK_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);
static QUALIFIED_NAME_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"`[^ ]+`\.`[^ ]+`").unwrap());

type State = super::State<Params, (BitFlags<Circumstances>, Connection)>;

struct Params {
    connector_params: ConnectorParams,
    url: MysqlUrl,
}

impl Params {
    fn new(connector_params: ConnectorParams) -> ConnectorResult<Self> {
        if let Some(shadow_db_url) = &connector_params.shadow_database_connection_string {
            super::validate_connection_infos_do_not_match(&connector_params.connection_string, shadow_db_url)?;
        }

        let url = connector_params
            .connection_string
            .parse()
            .map_err(ConnectorError::url_parse_error)?;
        let url = MysqlUrl::new(url).map_err(ConnectorError::url_parse_error)?;
        Ok(Self { connector_params, url })
    }
}

#[derive(Debug, Default)]
pub struct MysqlDialect {
    circumstances: BitFlags<Circumstances>,
}

impl MysqlDialect {
    fn new(circumstances: BitFlags<Circumstances>) -> Self {
        Self { circumstances }
    }
}

impl SqlDialect for MysqlDialect {
    fn renderer(&self) -> Box<dyn crate::sql_renderer::SqlRenderer> {
        Box::new(MysqlRenderer)
    }

    fn schema_differ(&self) -> Box<dyn crate::sql_schema_differ::SqlSchemaDifferFlavour> {
        Box::new(MysqlSchemaDifferFlavour::new(self.circumstances))
    }

    fn schema_calculator(&self) -> Box<dyn crate::sql_schema_calculator::SqlSchemaCalculatorFlavour> {
        Box::new(MysqlSchemaCalculatorFlavour)
    }

    fn destructive_change_checker(
        &self,
    ) -> Box<dyn crate::sql_destructive_change_checker::DestructiveChangeCheckerFlavour> {
        Box::new(MysqlDestructiveChangeCheckerFlavour)
    }

    fn datamodel_connector(&self) -> &'static dyn datamodel_connector::Connector {
        psl::builtin_connectors::MYSQL
    }

    fn scan_migration_script(&self, script: &str) {
        scan_migration_script_impl(script)
    }

    #[cfg(feature = "mysql-native")]
    fn connect_to_shadow_db(
        &self,
        url: String,
        preview_features: psl::PreviewFeatures,
    ) -> BoxFuture<'_, ConnectorResult<Box<dyn SqlConnector>>> {
        let params = ConnectorParams::new(url, preview_features, None);
        Box::pin(async move { Ok(Box::new(MysqlConnector::new_with_params(params)?) as Box<dyn SqlConnector>) })
    }

    #[cfg(not(feature = "mysql-native"))]
    fn connect_to_shadow_db(
        &self,
        _factory: std::sync::Arc<dyn quaint::connector::ExternalConnectorFactory>,
    ) -> BoxFuture<'_, ConnectorResult<Box<dyn SqlConnector>>> {
        todo!("MySQL WASM shadow database not supported yet")
    }
}

pub(crate) struct MysqlConnector {
    state: State,
}

impl std::fmt::Debug for MysqlConnector {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MysqlConnector").finish()
    }
}

impl MysqlConnector {
    pub fn new_with_params(params: ConnectorParams) -> ConnectorResult<Self> {
        Ok(MysqlConnector {
            state: State::WithParams(Params::new(params)?),
        })
    }

    pub(crate) fn is_mysql_5_6(&self) -> bool {
        self.circumstances().contains(Circumstances::IsMysql56)
    }

    pub(crate) fn database_name(&self) -> &str {
        self.state
            .params()
            .map(|p| p.url.dbname_or_default())
            .unwrap_or(quaint::connector::DEFAULT_MYSQL_DB)
    }

    fn circumstances(&self) -> BitFlags<Circumstances> {
        match self.state {
            super::State::Initial | super::State::WithParams(_) => Default::default(),
            super::State::Connected(_, (c, _)) => c,
        }
    }
}

impl SqlConnector for MysqlConnector {
    fn dialect(&self) -> Box<dyn SqlDialect> {
        Box::new(MysqlDialect::new(self.circumstances()))
    }

    fn shadow_db_url(&self) -> Option<&str> {
        self.state
            .params()?
            .connector_params
            .shadow_database_connection_string
            .as_deref()
    }

    fn acquire_lock(&mut self) -> BoxFuture<'_, ConnectorResult<()>> {
        with_connection(&mut self.state, |params, _, connection| async move {
            // We do not acquire advisory locks on PlanetScale instances.
            //
            // Advisory locking is supported on vitess (docs:
            // https://vitess.io/docs/12.0/design-docs/query-serving/locking-functions/), but
            // PlanetScale errors if the lock is held for longer than 20 seconds, making it
            // impractical. The recommended planetscale workflow with branching should open
            // fewer chances for race conditions to happen — that's the reasoning.
            if is_planetscale(&params.connector_params.connection_string) {
                return Ok(());
            }

            // https://dev.mysql.com/doc/refman/8.0/en/locking-functions.html
            let query = format!("SELECT GET_LOCK('prisma_migrate', {})", ADVISORY_LOCK_TIMEOUT.as_secs());
            connection.raw_cmd(&query, &params.url).await
        })
    }

    fn connector_type(&self) -> &'static str {
        "mysql"
    }

    fn describe_schema(&mut self, _namespaces: Option<Namespaces>) -> BoxFuture<'_, ConnectorResult<SqlSchema>> {
        with_connection(&mut self.state, |params, circumstances, connection| async move {
            connection.describe_schema(circumstances, params).await
        })
    }

    fn table_names(
        &mut self,
        _namespaces: Option<Namespaces>,
        filters: SchemaFilter,
    ) -> BoxFuture<'_, ConnectorResult<Vec<String>>> {
        Box::pin(async move {
            let select = r#"
                SELECT DISTINCT BINARY table_info.table_name AS table_name
                FROM information_schema.tables AS table_info
                JOIN information_schema.columns AS column_info
                    ON BINARY column_info.table_name = BINARY table_info.table_name
                WHERE
                    table_info.table_schema = ?
                    AND column_info.table_schema = ?
                    -- Exclude views.
                    AND table_info.table_type = 'BASE TABLE'
                ORDER BY BINARY table_info.table_name
            "#;

            let database_name = self.database_name();

            let rows = self
                .query_raw(
                    select,
                    &[database_name.to_string().into(), database_name.to_string().into()],
                )
                .await?;

            let table_names: Vec<String> = rows
                .into_iter()
                .flat_map(|row| row.get("table_name").and_then(|s| s.to_string()))
                .filter(|table_name| {
                    !self
                        .dialect()
                        .schema_differ()
                        .contains_table(&filters.external_tables, None, table_name)
                })
                .collect();

            Ok(table_names)
        })
    }

    fn apply_migration_script<'a>(
        &'a mut self,
        migration_name: &'a str,
        script: &'a str,
    ) -> BoxFuture<'a, ConnectorResult<()>> {
        with_connection(&mut self.state, move |_params, circumstances, connection| async move {
            connection
                .apply_migration_script(migration_name, script, circumstances)
                .await
        })
    }

    fn check_database_version_compatibility(
        &self,
        datamodel: &ValidatedSchema,
    ) -> Option<user_facing_errors::common::DatabaseVersionIncompatibility> {
        if self.is_mysql_5_6() {
            let mut errors = Vec::new();

            check_datamodel_for_mysql_5_6(datamodel, &mut errors);

            if errors.is_empty() {
                return None;
            }

            let mut errors_string = String::with_capacity(errors.iter().map(|err| err.len() + 3).sum());

            for error in &errors {
                errors_string.push_str("- ");
                errors_string.push_str(error);
                errors_string.push('\n');
            }

            Some(user_facing_errors::common::DatabaseVersionIncompatibility {
                errors: errors_string,
                database_version: "MySQL 5.6".into(),
            })
        } else {
            None
        }
    }

    fn create_database(&mut self) -> BoxFuture<'_, ConnectorResult<String>> {
        Box::pin(async {
            let params = self.state.get_unwrapped_params();
            let mut url =
                Url::parse(&params.connector_params.connection_string).map_err(ConnectorError::url_parse_error)?;
            url.set_path("/mysql");

            let mysql_url = MysqlUrl::new(url.clone()).unwrap();
            let mut conn = Connection::new(url).await?;
            let db_name = params.url.dbname_or_default();

            let query = format!("CREATE DATABASE `{db_name}` CHARACTER SET utf8mb4 COLLATE utf8mb4_unicode_ci;");

            conn.raw_cmd(&query, &mysql_url).await?;

            Ok(db_name.to_owned())
        })
    }

    fn create_migrations_table(&mut self) -> BoxFuture<'_, ConnectorResult<()>> {
        let sql = indoc! {r#"
            CREATE TABLE _prisma_migrations (
                id                      VARCHAR(36) PRIMARY KEY NOT NULL,
                checksum                VARCHAR(64) NOT NULL,
                finished_at             DATETIME(3),
                migration_name          VARCHAR(255) NOT NULL,
                logs                    TEXT,
                rolled_back_at          DATETIME(3),
                started_at              DATETIME(3) NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
                applied_steps_count     INTEGER UNSIGNED NOT NULL DEFAULT 0
            ) DEFAULT CHARACTER SET utf8mb4 COLLATE utf8mb4_unicode_ci;
        "#};

        self.raw_cmd(sql)
    }

    fn drop_database(&mut self) -> BoxFuture<'_, ConnectorResult<()>> {
        Box::pin(async {
            let params = self.state.get_unwrapped_params();
            let mut connection = Connection::new(params.url.url().clone()).await?;
            let db_name = params.url.dbname_or_default();

            connection
                .raw_cmd(&format!("DROP DATABASE `{db_name}`"), &params.url)
                .await?;

            Ok(())
        })
    }

    fn drop_migrations_table(&mut self) -> BoxFuture<'_, ConnectorResult<()>> {
        self.raw_cmd("DROP TABLE _prisma_migrations")
    }

    fn ensure_connection_validity(&mut self) -> BoxFuture<'_, ConnectorResult<()>> {
        with_connection(&mut self.state, |_, _, _| future::ready(Ok(())))
    }

    fn query<'a>(
        &'a mut self,
        q: quaint::ast::Query<'a>,
    ) -> BoxFuture<'a, ConnectorResult<quaint::prelude::ResultSet>> {
        with_connection(&mut self.state, |params, _, conn| async move {
            conn.query(q, &params.url).await
        })
    }

    fn query_raw<'a>(
        &'a mut self,
        sql: &'a str,
        params: &'a [quaint::Value<'a>],
    ) -> BoxFuture<'a, ConnectorResult<quaint::prelude::ResultSet>> {
        with_connection(&mut self.state, move |conn_params, _, conn| async move {
            conn.query_raw(sql, params, &conn_params.url).await
        })
    }

    fn raw_cmd<'a>(&'a mut self, sql: &'a str) -> BoxFuture<'a, ConnectorResult<()>> {
        with_connection(&mut self.state, move |params, _, conn| async move {
            conn.raw_cmd(sql, &params.url).await
        })
    }

    fn reset(&mut self, _namespaces: Option<Namespaces>) -> BoxFuture<'_, ConnectorResult<()>> {
        with_connection(&mut self.state, move |params, circumstances, connection| async move {
            if circumstances.contains(Circumstances::IsVitess) {
                return Err(ConnectorError::from_msg(
                    "We do not drop databases on Vitess until it works better.".into(),
                ));
            }

            let db_name = params.url.dbname_or_default();
            connection
                .raw_cmd(&format!("DROP DATABASE `{db_name}`"), &params.url)
                .await?;
            connection
                .raw_cmd(&format!("CREATE DATABASE `{db_name}`"), &params.url)
                .await?;
            connection.raw_cmd(&format!("USE `{db_name}`"), &params.url).await?;

            Ok(())
        })
    }

    #[tracing::instrument(skip(self, migrations))]
    fn sql_schema_from_migration_history<'a>(
        &'a mut self,
        migrations: &'a Migrations,
        namespaces: Option<Namespaces>,
        filter: &'a SchemaFilter,
        external_shadow_db: UsingExternalShadowDb,
    ) -> BoxFuture<'a, ConnectorResult<SqlSchema>> {
        match external_shadow_db {
            UsingExternalShadowDb::Yes => Box::pin(async move {
                self.ensure_connection_validity().await?;
                tracing::info!("Connected to an external shadow database.");

                if self.reset(None).await.is_err() {
                    crate::best_effort_reset(self, namespaces, filter).await?;
                }

                shadow_db::sql_schema_from_migrations_history(migrations, self).await
            }),

            // If we're not using an external shadow database, one must be created manually.
            UsingExternalShadowDb::No => {
                with_connection(&mut self.state, move |params, _circumstances, conn| async move {
                    let shadow_database_name = crate::new_shadow_database_name();

                    let create_database = format!("CREATE DATABASE `{shadow_database_name}`");
                    conn.raw_cmd(&create_database, &params.url)
                        .await
                        .map_err(|err| err.into_shadow_db_creation_error())?;

                    let mut shadow_database_url = params.url.url().clone();
                    shadow_database_url.set_path(&format!("/{shadow_database_name}"));
                    let host = shadow_database_url.host();

                    tracing::debug!("Connecting to shadow database at {:?}/{}", host, shadow_database_name);

                    let preview_features = params.connector_params.preview_features;
                    let connector_params =
                        ConnectorParams::new(shadow_database_url.to_string(), preview_features, None);
                    let mut shadow_database = MysqlConnector::new_with_params(connector_params)?;
                    // We go through the whole process without early return, then clean up
                    // the shadow database, and only then return the result. This avoids
                    // leaving shadow databases behind in case of e.g. faulty migrations.
                    let ret = shadow_db::sql_schema_from_migrations_history(migrations, &mut shadow_database).await;

                    let drop_database = format!("DROP DATABASE IF EXISTS `{shadow_database_name}`");
                    conn.raw_cmd(&drop_database, &params.url).await?;

                    ret
                })
            }
        }
    }

    fn set_preview_features(&mut self, preview_features: enumflags2::BitFlags<psl::PreviewFeature>) {
        match &mut self.state {
            super::State::Initial => {
                if !preview_features.is_empty() {
                    tracing::warn!("set_preview_feature on Initial state has no effect ({preview_features}).");
                }
            }
            super::State::WithParams(params) | super::State::Connected(params, _) => {
                params.connector_params.preview_features = preview_features
            }
        }
    }

    fn preview_features(&self) -> psl::PreviewFeatures {
        self.state
            .params()
            .map(|p| p.connector_params.preview_features)
            .unwrap_or_default()
    }

    fn version(&mut self) -> BoxFuture<'_, ConnectorResult<Option<String>>> {
        with_connection(&mut self.state, |params, _, connection| async {
            connection.version(&params.url).await
        })
    }

    fn search_path(&self) -> &str {
        self.database_name()
    }

    fn default_namespace(&self) -> Option<&str> {
        None // For MySQL we do not support multiple namespaces
    }

    fn describe_query<'a>(
        &'a mut self,
        sql: &'a str,
    ) -> BoxFuture<'a, ConnectorResult<quaint::connector::DescribedQuery>> {
        with_connection(&mut self.state, move |conn_params, circumstances, conn| {
            conn.describe_query(sql, &conn_params.url, circumstances)
        })
    }

    fn dispose(&mut self) -> BoxFuture<'_, ConnectorResult<()>> {
        // Nothing to on dispose, the connection is disposed in Drop
        Box::pin(async move { Ok(()) })
    }
}

#[enumflags2::bitflags]
#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(u8)]
pub(crate) enum Circumstances {
    LowerCasesTableNames,
    IsMysql56,
    IsMysql57,
    IsMariadb,
    IsVitess,
    CheckConstraints,
}

fn check_datamodel_for_mysql_5_6(datamodel: &ValidatedSchema, errors: &mut Vec<String>) {
    datamodel
        .db
        .walk_models()
        .flat_map(|model| model.scalar_fields())
        .for_each(|field| {
            if field
                .scalar_type()
                .map(|t| matches!(t, ScalarType::Json))
                .unwrap_or(false)
            {
                errors.push(format!(
                    "The `Json` data type used in {}.{} is not supported on MySQL 5.6.",
                    field.model().name(),
                    field.name()
                ))
            }
        });
}

fn with_connection<'a, O, F, C>(state: &'a mut State, f: C) -> BoxFuture<'a, ConnectorResult<O>>
where
    O: 'a,
    F: future::Future<Output = ConnectorResult<O>> + Send + 'a,
    C: (FnOnce(&'a mut Params, BitFlags<Circumstances>, &'a mut Connection) -> F) + Send + 'a,
{
    static MYSQL_SYSTEM_DATABASES: LazyLock<regex::RegexSet> = LazyLock::new(|| {
        RegexSet::new([
            "(?i)^mysql$",
            "(?i)^information_schema$",
            "(?i)^performance_schema$",
            "(?i)^sys$",
        ])
        .unwrap()
    });

    match state {
        super::State::Initial => panic!("logic error: Initial"),
        super::State::Connected(p, (circumstances, conn)) => Box::pin(f(p, *circumstances, conn)),
        state @ super::State::WithParams(_) => Box::pin(async move {
            state
                .try_connect(|params| {
                    Box::pin(async move {
                        let db_name = params.url.dbname_or_default();
                        let mut connection = Connection::new(params.url.url().clone()).await?;

                        if MYSQL_SYSTEM_DATABASES.is_match(db_name) {
                            return Err(SystemDatabase(db_name.to_owned()).into());
                        }

                        let versions = connection
                            .query_raw("SELECT @@version, @@GLOBAL.version", &[], &params.url)
                            .await?
                            .into_iter()
                            .next()
                            .and_then(|row| {
                                let mut columns = row.into_iter();
                                Some((columns.next()?.into_string()?, columns.next()?.into_string()?))
                            });

                        let mut circumstances = BitFlags::<Circumstances>::default();

                        if let Some((version, global_version)) = versions {
                            let semver = Versioning::new(&global_version).unwrap_or_default();
                            let min_check_constraints_semver = Versioning::new("8.0.16").unwrap();

                            if version.contains("vitess") || version.contains("Vitess") {
                                circumstances |= Circumstances::IsVitess;
                            }

                            if global_version.starts_with("5.6") {
                                circumstances |= Circumstances::IsMysql56;
                            }

                            if global_version.starts_with("5.7") {
                                circumstances |= Circumstances::IsMysql57;
                            }

                            if global_version.contains("MariaDB") {
                                circumstances |= Circumstances::IsMariadb;
                            }

                            if semver >= min_check_constraints_semver {
                                circumstances |= Circumstances::CheckConstraints;
                            }
                        }

                        let result_set = connection
                            .query_raw("SELECT @@lower_case_table_names", &[], &params.url)
                            .await?;

                        if let Some(1) = result_set.into_single().ok().and_then(|row| {
                            row.at(0).and_then(|row| {
                                row.to_string()
                                    .and_then(|s| s.parse().ok())
                                    .or_else(|| row.as_integer())
                            })
                        }) {
                            // https://dev.mysql.com/doc/refman/8.0/en/identifier-case-sensitivity.html
                            circumstances |= Circumstances::LowerCasesTableNames;
                        }

                        Ok((circumstances, connection))
                    })
                })
                .await?;
            with_connection(state, f).await
        }),
    }
}

fn scan_migration_script_impl(script: &str) {
    for capture in QUALIFIED_NAME_RE
        .captures_iter(script)
        .filter_map(|captures| captures.get(0))
    {
        tracing::warn!(
            location = ?capture.range(),
            name = capture.as_str(),
            "Your migration appears to contain a qualified name. Qualified names like `mydb`.`mytable` interact badly with the shadow database on MySQL. Please change these to unqualified names (just `mytable` in the previous example)."
        );
    }
}

/// This bit of logic was given to us by a PlanetScale engineer.
fn is_planetscale(connection_string: &str) -> bool {
    connection_string.contains(".psdb.cloud")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_impl_does_not_leak_connection_info() {
        let url = "mysql://myname:mypassword@myserver:8765/mydbname";

        let connector_params = ConnectorParams::new(url.to_owned(), Default::default(), None);
        let connector = MysqlConnector::new_with_params(connector_params).unwrap();
        let debugged = format!("{connector:?}");

        let words = &["myname", "mypassword", "myserver", "8765", "mydbname"];

        for word in words {
            assert!(!debugged.contains(word));
        }
    }

    #[test]
    fn qualified_name_re_matches_as_expected() {
        let should_match = r#"ALTER TABLE `mydb`.`cat` DROP PRIMARY KEY"#;
        let should_not_match = r#"ALTER TABLE `cat` ADD FOREIGN KEY (`ab`, cd`) REFERENCES `dog`(`id`)"#;

        assert!(
            QUALIFIED_NAME_RE.is_match_at(should_match, 12),
            "captures: {:?}",
            QUALIFIED_NAME_RE.captures(should_match)
        );
        assert!(!QUALIFIED_NAME_RE.is_match(should_not_match));
    }
}
