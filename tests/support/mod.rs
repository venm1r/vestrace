use sqlx::postgres::PgQueryResult;

pub fn assert_sqlstate(result: Result<PgQueryResult, sqlx::Error>, expected: &str) {
    let error = result.expect_err("statement unexpectedly succeeded");
    let database_error = error
        .as_database_error()
        .expect("statement failed without a PostgreSQL error");

    assert_eq!(database_error.code().as_deref(), Some(expected));
}
