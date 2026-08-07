use std::sync::Arc;

use async_trait::async_trait;

use crate::{ApplicationError, RequestContext};
use vestrace_domain::diagnostics::{DiagnosticFinding, DiagnosticReport, DiagnosticSeverity};

#[async_trait]
pub trait DiagnosticsRepository: Send + Sync {
    async fn check_migrations(
        &self,
        context: &RequestContext,
    ) -> Result<Vec<DiagnosticFinding>, ApplicationError>;
    async fn check_extensions(
        &self,
        context: &RequestContext,
    ) -> Result<Vec<DiagnosticFinding>, ApplicationError>;
    async fn check_broken_references(
        &self,
        context: &RequestContext,
    ) -> Result<Vec<DiagnosticFinding>, ApplicationError>;
    async fn check_outbox_lag(
        &self,
        context: &RequestContext,
    ) -> Result<Vec<DiagnosticFinding>, ApplicationError>;
    async fn check_missing_search_documents(
        &self,
        context: &RequestContext,
    ) -> Result<Vec<DiagnosticFinding>, ApplicationError>;
    async fn check_stale_model_health(
        &self,
        context: &RequestContext,
    ) -> Result<Vec<DiagnosticFinding>, ApplicationError>;
    async fn check_expired_leases(
        &self,
        context: &RequestContext,
    ) -> Result<Vec<DiagnosticFinding>, ApplicationError>;
    async fn check_dead_letter_jobs(
        &self,
        context: &RequestContext,
    ) -> Result<Vec<DiagnosticFinding>, ApplicationError>;
}

pub type SharedDiagnosticsRepository = Arc<dyn DiagnosticsRepository>;

pub struct DoctorService {
    repository: SharedDiagnosticsRepository,
}

impl DoctorService {
    pub fn new(repository: SharedDiagnosticsRepository) -> Self {
        Self { repository }
    }

    pub async fn run_checks(
        &self,
        context: &RequestContext,
    ) -> Result<DiagnosticReport, ApplicationError> {
        let mut findings = Vec::new();

        let mut migration_findings = self.repository.check_migrations(context).await?;
        findings.append(&mut migration_findings);

        let mut extension_findings = self.repository.check_extensions(context).await?;
        findings.append(&mut extension_findings);

        let mut ref_findings = self.repository.check_broken_references(context).await?;
        findings.append(&mut ref_findings);

        let mut outbox_findings = self.repository.check_outbox_lag(context).await?;
        findings.append(&mut outbox_findings);

        let mut search_findings = self
            .repository
            .check_missing_search_documents(context)
            .await?;
        findings.append(&mut search_findings);

        let mut model_findings = self.repository.check_stale_model_health(context).await?;
        findings.append(&mut model_findings);

        let mut lease_findings = self.repository.check_expired_leases(context).await?;
        findings.append(&mut lease_findings);

        let mut dead_letter_findings = self.repository.check_dead_letter_jobs(context).await?;
        findings.append(&mut dead_letter_findings);

        findings.sort_by_key(|f| match f.severity {
            DiagnosticSeverity::Error => 0,
            DiagnosticSeverity::Warning => 1,
            DiagnosticSeverity::Info => 2,
        });

        Ok(DiagnosticReport::new(findings))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vestrace_domain::PrincipalId;
    use vestrace_domain::id::WorkspaceId;

    struct CleanRepo;
    #[async_trait]
    impl DiagnosticsRepository for CleanRepo {
        async fn check_migrations(
            &self,
            _: &RequestContext,
        ) -> Result<Vec<DiagnosticFinding>, ApplicationError> {
            Ok(vec![])
        }
        async fn check_extensions(
            &self,
            _: &RequestContext,
        ) -> Result<Vec<DiagnosticFinding>, ApplicationError> {
            Ok(vec![])
        }
        async fn check_broken_references(
            &self,
            _: &RequestContext,
        ) -> Result<Vec<DiagnosticFinding>, ApplicationError> {
            Ok(vec![])
        }
        async fn check_outbox_lag(
            &self,
            _: &RequestContext,
        ) -> Result<Vec<DiagnosticFinding>, ApplicationError> {
            Ok(vec![])
        }
        async fn check_missing_search_documents(
            &self,
            _: &RequestContext,
        ) -> Result<Vec<DiagnosticFinding>, ApplicationError> {
            Ok(vec![])
        }
        async fn check_stale_model_health(
            &self,
            _: &RequestContext,
        ) -> Result<Vec<DiagnosticFinding>, ApplicationError> {
            Ok(vec![])
        }
        async fn check_expired_leases(
            &self,
            _: &RequestContext,
        ) -> Result<Vec<DiagnosticFinding>, ApplicationError> {
            Ok(vec![])
        }
        async fn check_dead_letter_jobs(
            &self,
            _: &RequestContext,
        ) -> Result<Vec<DiagnosticFinding>, ApplicationError> {
            Ok(vec![])
        }
    }

    struct DirtyRepo;
    #[async_trait]
    impl DiagnosticsRepository for DirtyRepo {
        async fn check_migrations(
            &self,
            _: &RequestContext,
        ) -> Result<Vec<DiagnosticFinding>, ApplicationError> {
            Ok(vec![DiagnosticFinding::error(
                "MIGRATION_MISMATCH",
                None,
                "missing migration 0018",
                "run vestrace migrate",
            )])
        }
        async fn check_extensions(
            &self,
            _: &RequestContext,
        ) -> Result<Vec<DiagnosticFinding>, ApplicationError> {
            Ok(vec![])
        }
        async fn check_broken_references(
            &self,
            _: &RequestContext,
        ) -> Result<Vec<DiagnosticFinding>, ApplicationError> {
            Ok(vec![])
        }
        async fn check_outbox_lag(
            &self,
            _: &RequestContext,
        ) -> Result<Vec<DiagnosticFinding>, ApplicationError> {
            Ok(vec![DiagnosticFinding::warning(
                "OUTBOX_LAG",
                None,
                "2 pending messages older than 60s",
                "process outbox",
            )])
        }
        async fn check_missing_search_documents(
            &self,
            _: &RequestContext,
        ) -> Result<Vec<DiagnosticFinding>, ApplicationError> {
            Ok(vec![])
        }
        async fn check_stale_model_health(
            &self,
            _: &RequestContext,
        ) -> Result<Vec<DiagnosticFinding>, ApplicationError> {
            Ok(vec![])
        }
        async fn check_expired_leases(
            &self,
            _: &RequestContext,
        ) -> Result<Vec<DiagnosticFinding>, ApplicationError> {
            Ok(vec![])
        }
        async fn check_dead_letter_jobs(
            &self,
            _: &RequestContext,
        ) -> Result<Vec<DiagnosticFinding>, ApplicationError> {
            Ok(vec![])
        }
    }

    fn ctx() -> RequestContext {
        RequestContext::new(WorkspaceId::new(), PrincipalId::new())
    }

    #[tokio::test]
    async fn clean_repo_produces_clean_report() {
        let svc = DoctorService::new(Arc::new(CleanRepo));
        let report = svc.run_checks(&ctx()).await.unwrap();
        assert!(report.is_clean());
        assert!(!report.has_errors());
    }

    #[tokio::test]
    async fn dirty_repo_produces_findings_with_errors_first() {
        let svc = DoctorService::new(Arc::new(DirtyRepo));
        let report = svc.run_checks(&ctx()).await.unwrap();
        assert!(report.has_errors());
        assert_eq!(report.findings.len(), 2);
        assert_eq!(report.findings[0].code, "MIGRATION_MISMATCH");
    }
}
