use async_trait::async_trait;
use vestrace_domain::{
    DomainError,
    id::AgentRunId,
    now,
    run::AgentRun,
};

use crate::{ApplicationError, RequestContext};

use super::{
    commands::CreateRunCommand,
    ports::{RunUseCases, SharedRunRepository},
};

pub struct RunService {
    repository: SharedRunRepository,
}

impl RunService {
    pub fn new(repository: SharedRunRepository) -> Self {
        Self { repository }
    }
}

#[async_trait]
impl RunUseCases for RunService {
    async fn create_run(
        &self,
        context: &RequestContext,
        command: CreateRunCommand,
    ) -> Result<AgentRun, ApplicationError> {
        let title = command.title.trim();
        if title.is_empty() {
            return Err(DomainError::InvalidArgument("run title is required".to_owned()).into());
        }

        let run = AgentRun::new(
            AgentRunId::new(),
            context.workspace_id,
            context.principal_id,
            title,
            now(),
        );
        self.repository.create(context, &run).await?;
        Ok(run)
    }

    async fn list_runs(
        &self,
        context: &RequestContext,
        limit: u32,
    ) -> Result<Vec<AgentRun>, ApplicationError> {
        if !(1..=100).contains(&limit) {
            return Err(DomainError::InvalidArgument(
                "run list limit must be between 1 and 100".to_owned(),
            )
            .into());
        }

        self.repository.list(context, limit).await
    }

    async fn get_run(
        &self,
        context: &RequestContext,
        id: AgentRunId,
    ) -> Result<Option<AgentRun>, ApplicationError> {
        self.repository.find_by_id(context, id).await
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use async_trait::async_trait;
    use vestrace_domain::{
        DomainError,
        PrincipalId,
        WorkspaceId,
        id::AgentRunId,
        now,
        run::{AgentRun, RunStatus},
    };

    use crate::{ApplicationError, RequestContext};

    use super::{CreateRunCommand, RunService};
    use crate::runs::{RunRepository, RunUseCases};

    #[derive(Default)]
    struct InMemoryRunRepository {
        saved: Mutex<Vec<AgentRun>>,
    }

    #[async_trait]
    impl RunRepository for InMemoryRunRepository {
        async fn create(
            &self,
            _context: &RequestContext,
            run: &AgentRun,
        ) -> Result<(), ApplicationError> {
            self.saved.lock().unwrap().push(run.clone());
            Ok(())
        }

        async fn list(
            &self,
            context: &RequestContext,
            limit: u32,
        ) -> Result<Vec<AgentRun>, ApplicationError> {
            Ok(self
                .saved
                .lock()
                .unwrap()
                .iter()
                .filter(|run| run.workspace_id == context.workspace_id)
                .take(limit as usize)
                .cloned()
                .collect())
        }

        async fn find_by_id(
            &self,
            context: &RequestContext,
            id: AgentRunId,
        ) -> Result<Option<AgentRun>, ApplicationError> {
            Ok(self
                .saved
                .lock()
                .unwrap()
                .iter()
                .find(|run| run.workspace_id == context.workspace_id && run.id == id)
                .cloned())
        }
    }

    #[tokio::test]
    async fn create_run_persists_a_workspace_bound_run() {
        let repository = Arc::new(InMemoryRunRepository::default());
        let service = RunService::new(repository.clone());
        let context = RequestContext::new(WorkspaceId::new(), PrincipalId::new());

        let run = service
            .create_run(
                &context,
                CreateRunCommand {
                    title: "  Verify retention policy  ".to_owned(),
                },
            )
            .await
            .unwrap();

        assert_eq!(run.title, "Verify retention policy");
        assert_eq!(run.workspace_id, context.workspace_id);
        assert_eq!(run.principal_id, context.principal_id);
        assert_eq!(run.status, RunStatus::Created);
        assert_eq!(
            repository.saved.lock().unwrap().as_slice(),
            std::slice::from_ref(&run)
        );
    }

    #[tokio::test]
    async fn create_run_rejects_a_blank_title() {
        let service = RunService::new(Arc::new(InMemoryRunRepository::default()));
        let context = RequestContext::new(WorkspaceId::new(), PrincipalId::new());

        let error = service
            .create_run(
                &context,
                CreateRunCommand {
                    title: "   ".to_owned(),
                },
            )
            .await
            .unwrap_err();

        assert!(matches!(
            error,
            ApplicationError::Domain(DomainError::InvalidArgument(message))
                if message == "run title is required"
        ));
    }

    #[tokio::test]
    async fn list_runs_rejects_limits_outside_the_supported_range() {
        let service = RunService::new(Arc::new(InMemoryRunRepository::default()));
        let context = RequestContext::new(WorkspaceId::new(), PrincipalId::new());

        for limit in [0, 101] {
            let error = service.list_runs(&context, limit).await.unwrap_err();
            assert!(matches!(
                error,
                ApplicationError::Domain(DomainError::InvalidArgument(message))
                    if message == "run list limit must be between 1 and 100"
            ));
        }
    }

    #[tokio::test]
    async fn get_run_delegates_workspace_scoped_lookup() {
        let repository = Arc::new(InMemoryRunRepository::default());
        let service = RunService::new(repository.clone());
        let context = RequestContext::new(WorkspaceId::new(), PrincipalId::new());
        let run = AgentRun::new(
            AgentRunId::new(),
            context.workspace_id,
            context.principal_id,
            "Lookup run",
            now(),
        );
        repository.saved.lock().unwrap().push(run.clone());

        assert_eq!(service.get_run(&context, run.id).await.unwrap(), Some(run));
    }
}
