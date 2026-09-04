use std::{
    process::{Command, Output},
    str::FromStr,
};

use sqlx::{
    ConnectOptions, PgPool,
    postgres::{PgConnectOptions, PgPoolOptions},
};

const MCP_SOURCE: &str = include_str!("../src/commands/mcp.rs");
const SERVER_SOURCE: &str = include_str!("../src/commands/server.rs");
const WORKER_SOURCE: &str = include_str!("../src/commands/worker.rs");
const CONFORMANCE_SOURCE: &str = include_str!("../src/commands/conformance.rs");

fn runtime_connect_options(pool: &PgPool) -> PgConnectOptions {
    let runtime_database_url = std::env::var("VESTRACE_RUNTIME_DATABASE_URL")
        .expect("VESTRACE_RUNTIME_DATABASE_URL must authenticate as the restricted runtime role");
    PgConnectOptions::from_str(&runtime_database_url)
        .expect("VESTRACE_RUNTIME_DATABASE_URL must be a PostgreSQL URL");
    let authority = runtime_database_url
        .split_once("://")
        .expect("VESTRACE_RUNTIME_DATABASE_URL must include a scheme")
        .1;
    let credentials = authority
        .rsplit_once('@')
        .expect("VESTRACE_RUNTIME_DATABASE_URL must include runtime credentials")
        .0;
    let (username, password) = credentials
        .split_once(':')
        .expect("VESTRACE_RUNTIME_DATABASE_URL must include a runtime password");
    assert_eq!(username, "vestrace");

    pool.connect_options()
        .as_ref()
        .clone()
        .username(username)
        .password(password)
}

fn runtime_database_url(pool: &PgPool) -> String {
    runtime_connect_options(pool).to_url_lossy().to_string()
}

async fn runtime_pool(pool: &PgPool) -> PgPool {
    PgPoolOptions::new()
        .max_connections(1)
        .connect_with(runtime_connect_options(pool))
        .await
        .expect("the restricted runtime role must connect to the per-test database")
}

fn run_mcp(database_url: &str) -> Output {
    Command::new(env!("CARGO_BIN_EXE_vestrace"))
        .arg("mcp")
        .env("VESTRACE_DATABASE__URL", database_url)
        .env("VESTRACE_POLICY__DATA__MEMORY_LABELS", "internal")
        .env(
            "VESTRACE_POLICY__DATA__RETRIEVAL__ADMISSIBLE_LABELS",
            "internal",
        )
        .env(
            "VESTRACE_POLICY__DATA__RETRIEVAL__ALLOW_UNCLASSIFIED",
            "false",
        )
        .output()
        .expect("vestrace command should start")
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

fn sqlstate(error: &sqlx::Error) -> Option<String> {
    error
        .as_database_error()
        .and_then(|database_error| database_error.code())
        .map(|code| code.into_owned())
}

#[test]
fn runtime_and_qualification_roots_verify_migrations_without_performing_ddl() {
    for (name, source) in [
        ("server", SERVER_SOURCE),
        ("worker", WORKER_SOURCE),
        ("mcp", MCP_SOURCE),
    ] {
        assert!(
            !source.contains(".migrate()"),
            "{name} performs DDL at runtime instead of leaving it to the administrative migrate command"
        );
        assert!(
            source.contains("migrations_are_compatible"),
            "{name} does not verify the installed schema before starting"
        );
    }

    assert!(
        !CONFORMANCE_SOURCE.contains(".migrate()"),
        "conformance performs DDL instead of leaving it to the administrative migrate command"
    );
    assert_eq!(
        CONFORMANCE_SOURCE
            .matches("migrations_are_compatible")
            .count(),
        4,
        "each conformance database entry point must verify migration history"
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn runtime_role_can_read_administratively_applied_migration_history(pool: PgPool) {
    let runtime = runtime_pool(&pool).await;
    let result = sqlx::query_scalar::<_, i64>("SELECT count(*) FROM _sqlx_migrations")
        .fetch_one(&runtime)
        .await;

    match result {
        Ok(_) => {}
        Err(error) => panic!(
            "the restricted runtime role needs SELECT on _sqlx_migrations to verify an \
             administratively migrated schema; got SQLSTATE {}",
            sqlstate(&error).as_deref().unwrap_or("no SQLSTATE")
        ),
    }
}

#[sqlx::test(migrations = "../../migrations")]
async fn runtime_mcp_starts_without_ddl_on_an_already_migrated_database(pool: PgPool) {
    let output = run_mcp(&runtime_database_url(&pool));

    assert!(
        output.status.success(),
        "runtime MCP must start after schema verification without executing DDL: {}",
        stderr(&output)
    );
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("\"tools\""),
        "runtime MCP did not reach its normal protocol output: {}",
        String::from_utf8_lossy(&output.stdout)
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn runtime_mcp_refuses_divergent_migration_history(pool: PgPool) {
    sqlx::query(
        "UPDATE _sqlx_migrations \
         SET checksum = decode('00', 'hex') \
         WHERE version = (SELECT max(version) FROM _sqlx_migrations)",
    )
    .execute(&pool)
    .await
    .expect("the test superuser must be able to make migration history divergent");

    let output = run_mcp(&runtime_database_url(&pool));

    assert!(
        !output.status.success(),
        "runtime MCP unexpectedly started with divergent migration history"
    );
    assert!(
        stderr(&output).contains("database migration history is incompatible"),
        "{}",
        stderr(&output)
    );
}
