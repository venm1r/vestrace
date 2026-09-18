use anyhow::anyhow;
use tracing::info;
use vestrace_infrastructure::{AppConfig, PgStore};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum MigrationMode {
    Through(i64),
    OnlyAfterPrefix(i64),
    Full,
}

impl MigrationMode {
    fn from_flags(through_version: Option<i64>, only_version: Option<i64>) -> Self {
        match (through_version, only_version) {
            (Some(version), None) => Self::Through(version),
            (None, Some(version)) => Self::OnlyAfterPrefix(version),
            (None, None) => Self::Full,
            (Some(_), Some(_)) => unreachable!("clap rejects conflicting migration options"),
        }
    }
}

pub async fn run(
    config: &AppConfig,
    through_version: Option<i64>,
    only_version: Option<i64>,
) -> anyhow::Result<()> {
    let store = PgStore::connect(&config.database)
        .await
        .map_err(|_| anyhow!("database is unavailable"))?;

    let mode = MigrationMode::from_flags(through_version, only_version);

    let compatible = match mode {
        MigrationMode::Through(version) => {
            store
                .migrate_through_version(version)
                .await
                .map_err(|_| anyhow!("database migrations are unavailable"))?;
            store.migrations_are_compatible_through(version).await
        }
        MigrationMode::OnlyAfterPrefix(version) => {
            store
                .migrate_only_version_after(version, version.saturating_sub(1))
                .await
                .map_err(|_| anyhow!("database migrations are unavailable"))?;
            store.migrations_are_compatible_through(version).await
        }
        MigrationMode::Full => {
            store
                .migrate()
                .await
                .map_err(|_| anyhow!("database migrations are unavailable"))?;
            store.migrations_are_compatible().await
        }
    };

    match compatible {
        Ok(true) => {
            info!("database migrations applied and verified");
            Ok(())
        }
        Ok(false) => anyhow::bail!("database migration history is incompatible"),
        Err(_) => Err(anyhow!("database migration verification is unavailable")),
    }
}

#[cfg(test)]
mod tests {
    use super::{MigrationMode, MigrationMode::*};

    #[test]
    fn bounded_modes_verify_their_exact_requested_prefix() {
        assert_eq!(MigrationMode::from_flags(Some(208), None), Through(208));
        assert_eq!(
            MigrationMode::from_flags(None, Some(209)),
            OnlyAfterPrefix(209)
        );
        assert_eq!(
            MigrationMode::from_flags(None, Some(210)),
            OnlyAfterPrefix(210)
        );
        assert_eq!(
            MigrationMode::from_flags(None, Some(211)),
            OnlyAfterPrefix(211)
        );
        assert_eq!(MigrationMode::from_flags(None, None), Full);
    }
}
