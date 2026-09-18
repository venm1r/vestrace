-- Runtime processes verify the schema history but never apply DDL. They need
-- this single read privilege after an administrative migrator owns the table.
GRANT SELECT ON TABLE _sqlx_migrations TO vestrace;
