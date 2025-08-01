use crate::UserFacingError;
use serde::Serialize;
use std::fmt::Display;
use user_facing_error_macros::*;

#[derive(Debug, UserFacingError, Serialize)]
#[user_facing(
    code = "P1000",
    message = "\
Authentication failed against database server, the provided database credentials for `{database_user}` are not valid.

Please make sure to provide valid database credentials for the database server at the configured address."
)]
// **Notes**: Might vary for different data source, For example, SQLite has no concept of user accounts, and instead relies on the file system for all database permissions. This makes enforcing storage quotas difficult and enforcing user permissions impossible.
pub struct IncorrectDatabaseCredentials {
    /// Database user name
    pub database_user: String,
}

#[derive(Debug, UserFacingError, Serialize)]
#[user_facing(
    code = "P1001",
    message = "\
Can't reach database server at `{database_location}`

Please make sure your database server is running at `{database_location}`."
)]
pub struct DatabaseNotReachable {
    pub database_location: String,
}

#[derive(Debug, UserFacingError, Serialize)]
#[user_facing(
    code = "P1002",
    message = "\
The database server was reached but timed out.

Please try again.

Please make sure your database server is running at the configured address.

Context: {context}
"
)]
pub struct DatabaseTimeout {
    /// Extra context
    pub context: String,
}

#[derive(Debug, Serialize)]
pub struct DatabaseDoesNotExist {
    pub database_name: String,
}

impl UserFacingError for DatabaseDoesNotExist {
    const ERROR_CODE: &'static str = "P1003";

    fn message(&self) -> String {
        format!("Database `{}` does not exist", self.database_name)
    }
}

#[derive(Debug, UserFacingError, Serialize)]
#[user_facing(
    code = "P1008",
    message = "Socket timeout (the database failed to respond to a query within the configured timeout{extra_hint})."
)]
pub struct DatabaseOperationTimeout {
    /// Extra hint
    pub extra_hint: String,
}

#[derive(Debug, UserFacingError, Serialize)]
#[user_facing(
    code = "P1009",
    message = "Database `{database_name}` already exists on the database server"
)]
pub struct DatabaseAlreadyExists {
    /// Database name, append `database_schema_name` when applicable
    /// `database_schema_name`: Database schema name (For Postgres for example)
    pub database_name: String,
}

#[derive(Debug, UserFacingError, Serialize)]
#[user_facing(code = "P1010", message = "User was denied access on the database `{database_name}`")]
pub struct DatabaseAccessDenied {
    /// Database name, append `database_schema_name` when applicable
    /// `database_schema_name`: Database schema name (For Postgres for example)
    pub database_name: String,
}

#[derive(Debug, UserFacingError, Serialize)]
#[user_facing(code = "P1011", message = "Error opening a TLS connection: {message}")]
pub struct TlsConnectionError {
    pub message: String,
}

#[derive(Debug, UserFacingError, Serialize)]
#[user_facing(code = "P1012", message = "{full_error}")]
pub struct SchemaParserError {
    pub full_error: String,
}

#[derive(Debug, UserFacingError, Serialize)]
#[user_facing(code = "P1013", message = "The provided database string is invalid. {details}")]
pub struct InvalidConnectionString {
    pub details: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub enum ModelKind {
    Table,
}

impl Display for ModelKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Table => write!(f, "table"),
        }
    }
}

#[derive(Debug, UserFacingError, Serialize)]
#[user_facing(
    code = "P1014",
    message = "The underlying {kind} for model `{model}` does not exist."
)]
pub struct InvalidModel {
    pub model: String,
    pub kind: ModelKind,
}

#[derive(Debug, UserFacingError, Serialize)]
#[user_facing(
    code = "P1015",
    message = "Your Prisma schema is using features that are not supported for the version of the database.\nDatabase version: {database_version}\nErrors:\n{errors}"
)]
pub struct DatabaseVersionIncompatibility {
    pub database_version: String,
    pub errors: String,
}

#[derive(Debug, UserFacingError, Serialize)]
#[user_facing(
    code = "P1016",
    message = "Your raw query had an incorrect number of parameters. Expected: `{expected}`, actual: `{actual}`."
)]
pub struct IncorrectNumberOfParameters {
    pub expected: usize,
    pub actual: usize,
}

#[derive(Debug, SimpleUserFacingError)]
#[user_facing(code = "P1017", message = "Server has closed the connection.")]
pub struct ConnectionClosed;

#[derive(Debug, UserFacingError, Serialize)]
#[user_facing(code = "P1018", message = "{message}")]
pub struct TransactionAlreadyClosed {
    pub message: String,
}

#[derive(Debug, UserFacingError, Serialize)]
#[user_facing(code = "P1019", message = "{message}")]
pub struct UnsupportedFeatureError {
    pub message: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::UserFacingError;

    #[test]
    fn database_does_not_exist_formats_properly() {
        let sqlite_err = DatabaseDoesNotExist {
            database_name: "dev.db".into(),
        };

        assert_eq!(sqlite_err.message(), "Database `dev.db` does not exist");
    }
}
