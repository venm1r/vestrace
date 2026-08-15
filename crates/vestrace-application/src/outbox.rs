use std::collections::BTreeMap;
use std::sync::Arc;

use crate::{ApplicationError, RequestContext};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use vestrace_domain::{WorkspaceId, id::OutboxId, time::Timestamp};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct OutboxMessage {
    pub id: OutboxId,
    pub workspace_id: WorkspaceId,
    pub topic: String,
    pub payload: serde_json::Value,
    pub created_at: Timestamp,
    /// How many times a handler has refused this message.
    ///
    /// Zero on a message that has never been attempted, which is what
    /// [`OutboxMessage::new`] produces. A producer does not set this.
    #[serde(default)]
    pub attempts: u32,
}

impl OutboxMessage {
    /// A message that has not been attempted.
    pub fn new(
        workspace_id: WorkspaceId,
        topic: impl Into<String>,
        payload: serde_json::Value,
        created_at: Timestamp,
    ) -> Self {
        Self {
            id: OutboxId::new(),
            workspace_id,
            topic: topic.into(),
            payload,
            created_at,
            attempts: 0,
        }
    }
}

/// How many refusals a message is given before it is set aside.
///
/// Five, with the backoff below, is a little over eight minutes — long enough to
/// cover a provider restart and short enough that a permanently broken handler
/// stops occupying a batch slot within one shift.
pub const MAX_DELIVERY_ATTEMPTS: u32 = 5;

/// When a message that has just failed its `attempts`-th delivery may be tried
/// again: 30s, 1m, 2m, 4m, capped at 8m.
///
/// Backoff is returned as a duration and stored on the row rather than kept in
/// the drain, because the drain restarts and the backlog does not.
pub fn retry_delay(attempts: u32) -> std::time::Duration {
    let exponent = attempts.saturating_sub(1).min(4);
    std::time::Duration::from_secs(30u64 << exponent)
}

#[async_trait]
pub trait OutboxRepository: Send + Sync {
    /// Record a message inside the caller's workspace scope.
    ///
    /// The context is not decoration: row-level security on `outbox` was
    /// `ENABLE`d and never `FORCE`d, so its isolation policy was inert against
    /// the owning runtime role. Forcing the policy and giving this a scope are
    /// two halves of one change — forcing it while `save` still ran on a bare
    /// pool would have made writing a memory fail.
    async fn save(
        &self,
        context: &RequestContext,
        message: &OutboxMessage,
    ) -> Result<(), ApplicationError>;

    /// Unprocessed, undelivered, currently-due messages for one workspace,
    /// oldest first.
    ///
    /// A message whose backoff has not elapsed is skipped rather than returned
    /// and refused again, and a dead-lettered message is never returned at all.
    /// Without both, a handful of permanently failing messages at the front of
    /// the queue would fill every batch and block every message written after
    /// them, while the drain reported itself as running.
    ///
    /// # At-least-once, stated plainly
    ///
    /// This used to carry `FOR UPDATE SKIP LOCKED` while running through a bare
    /// pool, so the row lock was released by the statement's own implicit
    /// commit — the clause read as concurrency control and provided none. Two
    /// drains would have claimed the same messages and both delivered them.
    ///
    /// Holding the lock across delivery is not the fix either: delivery makes
    /// network calls, and a transaction open across one is how a connection pool
    /// is exhausted by a slow provider. So the contract is at-least-once and
    /// says so, and a handler must be idempotent. The alternative — marking a
    /// message processed before delivering it — is at-most-once, which loses
    /// messages silently and is the worse of the two.
    async fn claim_pending(
        &self,
        context: &RequestContext,
        limit: u32,
    ) -> Result<Vec<OutboxMessage>, ApplicationError>;

    async fn mark_processed(
        &self,
        context: &RequestContext,
        id: OutboxId,
    ) -> Result<(), ApplicationError>;

    /// Record that delivery was refused, and set the message aside once it has
    /// been refused `MAX_DELIVERY_ATTEMPTS` times.
    ///
    /// The error text is stored on the row. A dead-lettered message whose cause
    /// existed only as a log line is a message nobody can act on, because the
    /// log has rotated by the time anyone reads the table.
    ///
    /// A dead-lettered message is never deleted: the row is the evidence that
    /// something the system undertook to do was not done.
    async fn record_failure(
        &self,
        context: &RequestContext,
        id: OutboxId,
        error: &str,
        retry_after: std::time::Duration,
        dead_letter: bool,
    ) -> Result<(), ApplicationError>;
}

pub type SharedOutboxRepository = Arc<dyn OutboxRepository>;

/// Something that acts on one topic of outbox message.
#[async_trait]
pub trait OutboxHandler: Send + Sync {
    /// The topic this handles, matched exactly.
    fn topic(&self) -> &str;

    /// Act on the message. Must be idempotent: the delivery contract is
    /// at-least-once, so a handler will see the same message again after a
    /// crash between delivery and acknowledgement.
    async fn handle(
        &self,
        context: &RequestContext,
        message: &OutboxMessage,
    ) -> Result<(), ApplicationError>;
}

pub type SharedOutboxHandler = Arc<dyn OutboxHandler>;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DrainReport {
    pub delivered: usize,
    /// Messages whose topic no handler claims. They are **not** marked
    /// processed: a topic nobody handles is a gap to notice, not a message to
    /// discard.
    pub unhandled: usize,
    pub failed: usize,
    /// Messages that exhausted their attempts on this pass. Counted separately
    /// from `failed` because a retry is normal operation and a dead letter is
    /// an undelivered promise.
    pub dead_lettered: usize,
}

impl DrainReport {
    pub fn did_work(&self) -> bool {
        self.delivered > 0
    }
}

/// Delivers outbox messages to the handlers that claim their topic.
///
/// # Why this did not exist
///
/// `OutboxRepository::claim_pending` and `mark_processed` were declared, and
/// implemented, and called by nothing. The outbox accumulated for as long as the
/// system had been writing to it, and the doctor's `outbox.backlog_within_budget`
/// invariant fired on every deployment with a remediation — "process the outbox
/// queue" — that named no component capable of doing it.
pub struct OutboxDispatcher {
    repository: SharedOutboxRepository,
    handlers: BTreeMap<String, SharedOutboxHandler>,
}

impl OutboxDispatcher {
    pub fn new(repository: SharedOutboxRepository) -> Self {
        Self {
            repository,
            handlers: BTreeMap::new(),
        }
    }

    pub fn with_handler(mut self, handler: SharedOutboxHandler) -> Self {
        self.handlers.insert(handler.topic().to_string(), handler);
        self
    }

    pub fn topics(&self) -> impl Iterator<Item = &str> {
        self.handlers.keys().map(String::as_str)
    }

    /// Deliver up to `batch` pending messages for this workspace.
    pub async fn drain_once(
        &self,
        context: &RequestContext,
        batch: u32,
    ) -> Result<DrainReport, ApplicationError> {
        let pending = self.repository.claim_pending(context, batch).await?;
        let mut report = DrainReport::default();

        for message in pending {
            let Some(handler) = self.handlers.get(&message.topic) else {
                report.unhandled += 1;
                continue;
            };

            match handler.handle(context, &message).await {
                Ok(()) => {
                    // Acknowledged only after the handler returned. A message
                    // marked processed before delivery is a message lost when
                    // delivery fails.
                    self.repository.mark_processed(context, message.id).await?;
                    report.delivered += 1;
                }
                Err(error) => {
                    // Left unacknowledged deliberately, so it is retried. A
                    // failure that discarded the message would show up as
                    // nothing at all.
                    let attempts = message.attempts.saturating_add(1);
                    let dead_letter = attempts >= MAX_DELIVERY_ATTEMPTS;
                    self.repository
                        .record_failure(
                            context,
                            message.id,
                            &error.to_string(),
                            retry_delay(attempts),
                            dead_letter,
                        )
                        .await?;

                    if dead_letter {
                        // Escalated to an error, and counted apart from a
                        // retry: giving up on a message is not the same event
                        // as failing to deliver it this time.
                        tracing::error!(
                            topic = %message.topic,
                            message_id = %message.id,
                            attempts,
                            error = %error,
                            "outbox message dead-lettered after exhausting its attempts"
                        );
                        report.dead_lettered += 1;
                    } else {
                        tracing::warn!(
                            topic = %message.topic,
                            message_id = %message.id,
                            attempts,
                            error = %error,
                            "outbox delivery failed and will be retried"
                        );
                    }
                    report.failed += 1;
                }
            }
        }

        Ok(report)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    use std::time::Duration;
    use vestrace_domain::id::PrincipalId;

    /// A stored message and the delivery state the table keeps beside it.
    #[derive(Clone)]
    struct Row {
        message: OutboxMessage,
        processed: bool,
        dead_lettered: bool,
        last_error: Option<String>,
        retry_after: Option<Duration>,
        /// Set when a failure defers the message; a claim skips it, which is
        /// what the adapter's `next_attempt_at <= NOW()` does.
        deferred: bool,
    }

    #[derive(Default)]
    struct FakeOutbox {
        rows: Mutex<Vec<Row>>,
    }

    impl FakeOutbox {
        fn insert(&self, message: OutboxMessage) {
            self.rows.lock().unwrap().push(Row {
                message,
                processed: false,
                dead_lettered: false,
                last_error: None,
                retry_after: None,
                deferred: false,
            });
        }

        fn row(&self, id: OutboxId) -> Row {
            self.rows
                .lock()
                .unwrap()
                .iter()
                .find(|row| row.message.id == id)
                .expect("row is present")
                .clone()
        }
    }

    #[async_trait]
    impl OutboxRepository for FakeOutbox {
        async fn save(
            &self,
            _context: &RequestContext,
            message: &OutboxMessage,
        ) -> Result<(), ApplicationError> {
            self.insert(message.clone());
            Ok(())
        }

        async fn claim_pending(
            &self,
            _context: &RequestContext,
            limit: u32,
        ) -> Result<Vec<OutboxMessage>, ApplicationError> {
            Ok(self
                .rows
                .lock()
                .unwrap()
                .iter()
                .filter(|row| !row.processed && !row.dead_lettered && !row.deferred)
                .take(limit as usize)
                .map(|row| row.message.clone())
                .collect())
        }

        async fn mark_processed(
            &self,
            _context: &RequestContext,
            id: OutboxId,
        ) -> Result<(), ApplicationError> {
            for row in self.rows.lock().unwrap().iter_mut() {
                if row.message.id == id {
                    row.processed = true;
                }
            }
            Ok(())
        }

        async fn record_failure(
            &self,
            _context: &RequestContext,
            id: OutboxId,
            error: &str,
            retry_after: Duration,
            dead_letter: bool,
        ) -> Result<(), ApplicationError> {
            for row in self.rows.lock().unwrap().iter_mut() {
                if row.message.id == id {
                    row.message.attempts += 1;
                    row.last_error = Some(error.to_string());
                    row.retry_after = Some(retry_after);
                    row.deferred = true;
                    row.dead_lettered = row.dead_lettered || dead_letter;
                }
            }
            Ok(())
        }
    }

    struct Recording {
        topic: String,
        seen: Mutex<Vec<OutboxId>>,
        fails: bool,
    }

    impl Recording {
        fn new(topic: &str, fails: bool) -> Self {
            Self {
                topic: topic.to_string(),
                seen: Mutex::new(Vec::new()),
                fails,
            }
        }
    }

    #[async_trait]
    impl OutboxHandler for Recording {
        fn topic(&self) -> &str {
            &self.topic
        }

        async fn handle(
            &self,
            _context: &RequestContext,
            message: &OutboxMessage,
        ) -> Result<(), ApplicationError> {
            self.seen.lock().unwrap().push(message.id);
            if self.fails {
                return Err(ApplicationError::Unavailable("the model is down".into()));
            }
            Ok(())
        }
    }

    fn context() -> RequestContext {
        RequestContext::new(WorkspaceId::new(), PrincipalId::new())
    }

    fn message(topic: &str) -> OutboxMessage {
        OutboxMessage::new(
            WorkspaceId::new(),
            topic,
            serde_json::json!({}),
            chrono::Utc::now(),
        )
    }

    #[tokio::test]
    async fn a_delivered_message_is_acknowledged() {
        let repository = Arc::new(FakeOutbox::default());
        let handler = Arc::new(Recording::new("memory.created", false));
        let delivered = message("memory.created");
        repository.insert(delivered.clone());

        let report = OutboxDispatcher::new(repository.clone())
            .with_handler(handler.clone())
            .drain_once(&context(), 10)
            .await
            .unwrap();

        assert_eq!(report.delivered, 1);
        assert_eq!(handler.seen.lock().unwrap().as_slice(), [delivered.id]);
        assert!(repository.row(delivered.id).processed);
    }

    /// A failed delivery that acknowledged the message would lose it silently.
    /// It stays pending so the next pass retries it, and so the backlog the
    /// health invariant watches keeps telling the truth.
    #[tokio::test]
    async fn a_failed_delivery_is_not_acknowledged() {
        let repository = Arc::new(FakeOutbox::default());
        let failing = message("memory.created");
        repository.insert(failing.clone());

        let report = OutboxDispatcher::new(repository.clone())
            .with_handler(Arc::new(Recording::new("memory.created", true)))
            .drain_once(&context(), 10)
            .await
            .unwrap();

        assert_eq!(report.failed, 1);
        assert_eq!(report.delivered, 0);
        assert_eq!(report.dead_lettered, 0);

        let row = repository.row(failing.id);
        assert!(!row.processed);
        assert!(!row.dead_lettered);
        assert_eq!(row.message.attempts, 1);
    }

    /// The reason is stored on the row. A dead letter whose cause existed only
    /// as a log line is one nobody can act on: the log has rotated by the time
    /// anyone reads the table.
    #[tokio::test]
    async fn a_failure_records_why() {
        let repository = Arc::new(FakeOutbox::default());
        let failing = message("memory.created");
        repository.insert(failing.clone());

        OutboxDispatcher::new(repository.clone())
            .with_handler(Arc::new(Recording::new("memory.created", true)))
            .drain_once(&context(), 10)
            .await
            .unwrap();

        let row = repository.row(failing.id);
        assert!(
            row.last_error
                .as_deref()
                .unwrap()
                .contains("the model is down"),
            "the handler's reason was not recorded: {:?}",
            row.last_error
        );
        assert_eq!(row.retry_after, Some(retry_delay(1)));
    }

    /// The whole point of the deferral: a permanently failing message must not
    /// be re-offered on the very next pass, or it fills every batch and the
    /// messages behind it are never reached.
    #[tokio::test]
    async fn a_failed_message_is_not_immediately_reclaimed() {
        let repository = Arc::new(FakeOutbox::default());
        repository.insert(message("memory.created"));
        let behind = message("memory.revised");
        repository.insert(behind.clone());

        let dispatcher = OutboxDispatcher::new(repository.clone())
            .with_handler(Arc::new(Recording::new("memory.created", true)))
            .with_handler(Arc::new(Recording::new("memory.revised", false)));

        dispatcher.drain_once(&context(), 1).await.unwrap();
        // Batch of one: the first pass saw only the failing message. If it were
        // still claimable, the second pass would see it again and the message
        // behind it would never be delivered.
        let report = dispatcher.drain_once(&context(), 1).await.unwrap();

        assert_eq!(report.delivered, 1);
        assert!(repository.row(behind.id).processed);
    }

    /// Retrying forever is not resilience: the message occupies a batch slot
    /// and the failure never becomes a thing anyone is told about.
    #[tokio::test]
    async fn a_message_is_dead_lettered_once_its_attempts_are_spent() {
        let repository = Arc::new(FakeOutbox::default());
        let mut doomed = message("memory.created");
        doomed.attempts = MAX_DELIVERY_ATTEMPTS - 1;
        repository.insert(doomed.clone());

        let report = OutboxDispatcher::new(repository.clone())
            .with_handler(Arc::new(Recording::new("memory.created", true)))
            .drain_once(&context(), 10)
            .await
            .unwrap();

        assert_eq!(report.dead_lettered, 1);
        assert_eq!(report.failed, 1);

        let row = repository.row(doomed.id);
        assert!(row.dead_lettered);
        // Never acknowledged: a dead letter is an undelivered promise, and a
        // row claiming it was processed would erase the evidence.
        assert!(!row.processed);
    }

    /// A dead-lettered message is set aside, not retried and not deleted.
    #[tokio::test]
    async fn a_dead_lettered_message_is_not_claimed_again() {
        let repository = Arc::new(FakeOutbox::default());
        let mut doomed = message("memory.created");
        doomed.attempts = MAX_DELIVERY_ATTEMPTS - 1;
        repository.insert(doomed.clone());

        let handler = Arc::new(Recording::new("memory.created", true));
        let dispatcher = OutboxDispatcher::new(repository.clone()).with_handler(handler.clone());

        dispatcher.drain_once(&context(), 10).await.unwrap();
        let report = dispatcher.drain_once(&context(), 10).await.unwrap();

        assert_eq!(report, DrainReport::default());
        assert_eq!(handler.seen.lock().unwrap().len(), 1);
        assert_eq!(repository.rows.lock().unwrap().len(), 1);
    }

    /// A topic nobody handles is a gap to notice, not a message to discard.
    #[tokio::test]
    async fn an_unhandled_topic_is_left_pending_and_counted() {
        let repository = Arc::new(FakeOutbox::default());
        let orphan = message("event.recorded");
        repository.insert(orphan.clone());

        let report = OutboxDispatcher::new(repository.clone())
            .with_handler(Arc::new(Recording::new("memory.created", false)))
            .drain_once(&context(), 10)
            .await
            .unwrap();

        assert_eq!(report.unhandled, 1);
        assert_eq!(report.delivered, 0);
        assert!(!report.did_work());
        let row = repository.row(orphan.id);
        assert!(!row.processed);
        // Not counted as an attempt either: nothing tried to deliver it, so
        // giving it a failure budget it can exhaust would dead-letter messages
        // for the absence of a handler.
        assert_eq!(row.message.attempts, 0);
    }

    /// Handlers are matched by exact topic, and one failing topic must not stop
    /// the messages behind it from being delivered.
    #[tokio::test]
    async fn one_failing_topic_does_not_block_another() {
        let repository = Arc::new(FakeOutbox::default());
        repository.insert(message("memory.created"));
        let revised = message("memory.revised");
        repository.insert(revised.clone());

        let report = OutboxDispatcher::new(repository.clone())
            .with_handler(Arc::new(Recording::new("memory.created", true)))
            .with_handler(Arc::new(Recording::new("memory.revised", false)))
            .drain_once(&context(), 10)
            .await
            .unwrap();

        assert_eq!(report.failed, 1);
        assert_eq!(report.delivered, 1);
        assert!(repository.row(revised.id).processed);
    }

    /// Backoff grows and then stops growing: unbounded doubling would put a
    /// message hours out for a provider that came back in a minute.
    #[test]
    fn backoff_grows_to_a_cap() {
        assert_eq!(retry_delay(1), Duration::from_secs(30));
        assert_eq!(retry_delay(2), Duration::from_secs(60));
        assert_eq!(retry_delay(3), Duration::from_secs(120));
        assert_eq!(retry_delay(4), Duration::from_secs(240));
        assert_eq!(retry_delay(5), Duration::from_secs(480));
        assert_eq!(retry_delay(50), Duration::from_secs(480));
        // Attempt zero is not a real call site — a failure always increments
        // first — but it must not underflow into an enormous shift.
        assert_eq!(retry_delay(0), Duration::from_secs(30));
    }
}
