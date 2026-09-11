use vestrace_domain::external_effects::EffectFaultPoint;

/// Which lifecycle one invocation crashes.
///
/// The external-effect scenario is the default precisely because it predates
/// the others: every existing invocation omits `--scenario`, and none of them
/// may change meaning.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Scenario {
    ExternalEffect,
    MaterialIntent,
    CredentialIntent,
    EmbeddingDispatch,
    EmbeddingResultPreparation,
    EmbeddingResultFinalization,
    EmbeddingWorkerCompletion,
}

impl Scenario {
    fn parse(value: &str) -> Result<Self, String> {
        match value {
            "external_effect" => Ok(Self::ExternalEffect),
            "material_intent_crash" => Ok(Self::MaterialIntent),
            "credential_intent_crash" => Ok(Self::CredentialIntent),
            "embedding_dispatch_crash" => Ok(Self::EmbeddingDispatch),
            "embedding_result_preparation_crash" => Ok(Self::EmbeddingResultPreparation),
            "embedding_result_finalization_crash" => Ok(Self::EmbeddingResultFinalization),
            "embedding_worker_completion_crash" => Ok(Self::EmbeddingWorkerCompletion),
            other => Err(format!("unknown scenario '{other}'")),
        }
    }
}

/// The boundary this invocation crashes at, in the vocabulary of its scenario.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ScenarioPoint {
    Effect(EffectFaultPoint),
    Intent(EffectFaultPoint),
    Embedding(EffectFaultPoint),
    EmbeddingResultPreparation,
    EmbeddingResultFinalization,
    EmbeddingWorkerCompletion,
}

/// What one invocation was asked to do, and whether it is allowed to.
///
/// The invoking contract clears the environment and passes only the three
/// `VESTRACE_FAULT_*` variables, so everything else has to arrive through
/// arguments — and the database URL cannot, because argv is readable by any
/// process on the host. It arrives as a path to a file instead.
pub struct ScenarioSettings {
    scenario: Scenario,
    point: ScenarioPoint,
    database_url: String,
    is_child: bool,
}

impl std::fmt::Debug for ScenarioSettings {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // The point and child flag are safe to show and are the useful half
        // when diagnosing a refusal; the connection string carries a password
        // and is never rendered.
        formatter
            .debug_struct("ScenarioSettings")
            .field("scenario", &self.scenario)
            .field("point", &self.point)
            .field("database_url", &"[REDACTED]")
            .field("is_child", &self.is_child)
            .finish()
    }
}

impl ScenarioSettings {
    pub fn from_env_and_args(
        args: impl Iterator<Item = String>,
        env: &dyn Fn(&str) -> Option<String>,
    ) -> Result<Self, String> {
        let isolation = env("VESTRACE_FAULT_ISOLATION").unwrap_or_default();
        if isolation != "ephemeral" {
            return Err(format!(
                "refusing to run: this scenario kills processes mid-transaction and \
                 requires an ephemeral isolation, but VESTRACE_FAULT_ISOLATION is \
                 '{isolation}'"
            ));
        }

        let mut url_file = None;
        let mut requested_scenario = None;
        let mut args = args.peekable();
        while let Some(arg) = args.next() {
            if arg == "--database-url-file" {
                url_file = args.next();
            } else if arg == "--scenario" {
                requested_scenario = args.next();
            }
        }

        // An invocation that names no scenario is an external-effect invocation.
        // Every caller written before this argument existed relies on that.
        let scenario = match requested_scenario {
            Some(value) => Scenario::parse(&value)?,
            None => Scenario::ExternalEffect,
        };

        let requested = env("VESTRACE_FAULT_POINT").unwrap_or_default();
        let point = match scenario {
            Scenario::ExternalEffect => {
                ScenarioPoint::Effect(parse_external_effect_point(&requested)?)
            }
            Scenario::MaterialIntent | Scenario::CredentialIntent => {
                ScenarioPoint::Intent(parse_intent_point(&requested)?)
            }
            Scenario::EmbeddingDispatch => {
                ScenarioPoint::Embedding(parse_embedding_dispatch_point(&requested)?)
            }
            Scenario::EmbeddingResultPreparation => {
                if requested != "after_result_prepared_before_return" {
                    return Err(format!("unknown fault point '{requested}'"));
                }
                ScenarioPoint::EmbeddingResultPreparation
            }
            Scenario::EmbeddingResultFinalization => {
                if requested != "finalization_checkpoint_matrix" {
                    return Err(format!("unknown fault point '{requested}'"));
                }
                ScenarioPoint::EmbeddingResultFinalization
            }
            Scenario::EmbeddingWorkerCompletion => {
                if requested != "after_work_claim" {
                    return Err(format!("unknown fault point '{requested}'"));
                }
                ScenarioPoint::EmbeddingWorkerCompletion
            }
        };

        let url_file = url_file
            .ok_or_else(|| "database url file is required: pass --database-url-file".to_owned())?;
        let database_url = std::fs::read_to_string(&url_file)
            .map_err(|error| format!("database url file {url_file} is unreadable: {error}"))?
            .trim()
            .to_owned();
        if database_url.is_empty() {
            return Err(format!("database url file {url_file} is empty"));
        }

        Ok(Self {
            scenario,
            point,
            database_url,
            is_child: env("VESTRACE_FAULT_CHILD").is_some(),
        })
    }

    pub fn scenario(&self) -> Scenario {
        self.scenario
    }

    /// The external-effect boundary.
    ///
    /// Defined only for [`Scenario::ExternalEffect`]. `main` dispatches on the
    /// scenario before any external-effect code runs, so the other arms cannot
    /// reach this; it panics rather than inventing a boundary that the parent
    /// would then report as observed.
    pub fn point(&self) -> EffectFaultPoint {
        match self.point {
            ScenarioPoint::Effect(point) => point,
            ScenarioPoint::Intent(point) => panic!(
                "point() is defined only for the external-effect scenario, but this \
                 invocation is {:?} at {}",
                self.scenario,
                point.as_str()
            ),
            ScenarioPoint::Embedding(point) => panic!(
                "point() is defined only for the external-effect scenario, but this \
                 invocation is {:?} at {}",
                self.scenario,
                point.as_str()
            ),
            ScenarioPoint::EmbeddingResultPreparation
            | ScenarioPoint::EmbeddingResultFinalization
            | ScenarioPoint::EmbeddingWorkerCompletion => panic!(
                "point() is defined only for the external-effect scenario, but this \
                 invocation is result preparation"
            ),
        }
    }

    /// The key-intent boundary, or `None` for an external-effect invocation.
    pub fn intent_point(&self) -> Option<EffectFaultPoint> {
        match self.point {
            ScenarioPoint::Intent(point) => Some(point),
            ScenarioPoint::Effect(_)
            | ScenarioPoint::Embedding(_)
            | ScenarioPoint::EmbeddingResultPreparation
            | ScenarioPoint::EmbeddingResultFinalization
            | ScenarioPoint::EmbeddingWorkerCompletion => None,
        }
    }

    /// The embedding-dispatch boundary.
    pub fn embedding_dispatch_point(&self) -> EffectFaultPoint {
        match self.point {
            ScenarioPoint::Embedding(point) => point,
            _ => panic!("embedding_dispatch_point() is defined only for the embedding scenario"),
        }
    }

    /// The result-preparation scenario has one fixed point: immediately after
    /// its atomic marker transaction commits and before the child returns.
    pub fn embedding_result_preparation_point(&self) {
        if !matches!(self.point, ScenarioPoint::EmbeddingResultPreparation) {
            panic!(
                "embedding_result_preparation_point() is defined only for the result-preparation scenario"
            );
        }
    }

    /// The worker-completion scenario has one boundary: the instant after a
    /// real work claim commits and before anything is dispatched against it.
    ///
    /// That instant is where the only irreversible step in the cycle has not
    /// yet happened, so it is the one boundary whose survivors say something a
    /// later one cannot: the lease is held by a process that no longer exists,
    /// and the provider was never reached.
    pub fn embedding_worker_completion_point(&self) {
        if !matches!(self.point, ScenarioPoint::EmbeddingWorkerCompletion) {
            panic!(
                "embedding_worker_completion_point() is defined only for the worker-completion scenario"
            );
        }
    }

    pub fn database_url(&self) -> &str {
        &self.database_url
    }

    pub fn is_child(&self) -> bool {
        self.is_child
    }
}

fn parse_external_effect_point(value: &str) -> Result<EffectFaultPoint, String> {
    match value {
        "after_intent_persistence" => Ok(EffectFaultPoint::AfterIntentPersistence),
        "after_authorization_before_dispatch" => {
            Ok(EffectFaultPoint::AfterAuthorizationBeforeDispatch)
        }
        "after_dispatch_before_receipt" => Ok(EffectFaultPoint::AfterDispatchBeforeReceipt),
        "after_receipt_before_outcome_confirmation" => {
            Ok(EffectFaultPoint::AfterReceiptBeforeOutcomeConfirmation)
        }
        "after_outcome_before_run_commit" => Ok(EffectFaultPoint::AfterOutcomeBeforeRunCommit),
        "after_reserved"
        | "after_vault_create_before_receipt"
        | "after_receipt_before_prepared"
        | "after_prepared_before_bound"
        | "after_bound_before_promotion"
        | "after_abort_before_witnessed_erase"
        | "after_erase_receipt_before_terminal_append" => {
            Err(format!("unknown fault point '{value}'"))
        }
        other => Err(format!("unknown fault point '{other}'")),
    }
}

fn parse_intent_point(value: &str) -> Result<EffectFaultPoint, String> {
    EffectFaultPoint::intent_points()
        .into_iter()
        .find(|point| point.as_str() == value)
        .ok_or_else(|| format!("unknown fault point '{value}'"))
}

fn parse_embedding_dispatch_point(value: &str) -> Result<EffectFaultPoint, String> {
    parse_external_effect_point(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn url_file() -> std::path::PathBuf {
        let path = std::env::temp_dir().join(format!("vestrace-fault-url-{}", std::process::id()));
        std::fs::write(&path, "postgres://localhost/ephemeral").unwrap();
        path
    }

    fn settings_for(scenario: Option<&str>, point: &str) -> Result<ScenarioSettings, String> {
        let path = url_file();
        let mut args = vec!["--database-url-file".to_owned(), path.display().to_string()];
        if let Some(scenario) = scenario {
            args.push("--scenario".to_owned());
            args.push(scenario.to_owned());
        }
        ScenarioSettings::from_env_and_args(args.into_iter(), &|name| match name {
            "VESTRACE_FAULT_ISOLATION" => Some("ephemeral".to_owned()),
            "VESTRACE_FAULT_POINT" => Some(point.to_owned()),
            _ => None,
        })
    }

    #[test]
    fn an_invocation_without_a_scenario_is_still_an_external_effect_invocation() {
        let settings = settings_for(None, "after_dispatch_before_receipt").unwrap();
        assert_eq!(settings.scenario(), Scenario::ExternalEffect);
        assert_eq!(
            settings.point(),
            EffectFaultPoint::AfterDispatchBeforeReceipt
        );
        assert_eq!(settings.intent_point(), None);
    }

    #[test]
    fn an_intent_scenario_parses_its_own_boundary_vocabulary() {
        let settings = settings_for(
            Some("material_intent_crash"),
            "after_vault_create_before_receipt",
        )
        .unwrap();
        assert_eq!(settings.scenario(), Scenario::MaterialIntent);
        assert_eq!(
            settings.intent_point(),
            Some(EffectFaultPoint::AfterVaultCreateBeforeReceipt)
        );
    }

    #[test]
    fn an_effect_boundary_is_not_accepted_by_an_intent_scenario() {
        let error = settings_for(
            Some("material_intent_crash"),
            "after_dispatch_before_receipt",
        )
        .expect_err("an effect boundary must not name an intent boundary");
        assert!(error.contains("unknown fault point"), "{error}");
    }

    #[test]
    fn an_intent_boundary_is_not_accepted_by_the_effect_scenario() {
        let error = settings_for(None, "after_reserved")
            .expect_err("an intent boundary must not name an effect boundary");
        assert!(error.contains("unknown fault point"), "{error}");
    }

    #[test]
    fn result_preparation_has_only_its_post_commit_abort_boundary() {
        let settings = settings_for(
            Some("embedding_result_preparation_crash"),
            "after_result_prepared_before_return",
        )
        .unwrap();
        assert_eq!(settings.scenario(), Scenario::EmbeddingResultPreparation);
        settings.embedding_result_preparation_point();
        let error = settings_for(
            Some("embedding_result_preparation_crash"),
            "after_dispatch_before_receipt",
        )
        .expect_err("the result scenario must not borrow dispatch fault points");
        assert!(error.contains("unknown fault point"), "{error}");
    }

    #[test]
    fn result_finalization_has_only_its_checkpoint_matrix() {
        let settings = settings_for(
            Some("embedding_result_finalization_crash"),
            "finalization_checkpoint_matrix",
        )
        .unwrap();
        assert_eq!(settings.scenario(), Scenario::EmbeddingResultFinalization);
        assert!(
            settings_for(
                Some("embedding_result_finalization_crash"),
                "after_result_prepared_before_return"
            )
            .is_err()
        );
    }

    #[test]
    fn worker_completion_has_only_its_post_claim_boundary() {
        let settings = settings_for(
            Some("embedding_worker_completion_crash"),
            "after_work_claim",
        )
        .unwrap();
        assert_eq!(settings.scenario(), Scenario::EmbeddingWorkerCompletion);
        settings.embedding_worker_completion_point();
        assert_eq!(settings.intent_point(), None);
        // It borrows no other scenario's vocabulary, in either direction.
        for borrowed in [
            "after_dispatch_before_receipt",
            "after_result_prepared_before_return",
            "finalization_checkpoint_matrix",
        ] {
            assert!(
                settings_for(Some("embedding_worker_completion_crash"), borrowed).is_err(),
                "{borrowed} must not name a worker-completion boundary"
            );
        }
        assert!(settings_for(None, "after_work_claim").is_err());
    }

    #[test]
    fn the_ephemeral_isolation_guard_still_refuses_every_scenario() {
        let path = url_file();
        for scenario in ["material_intent_crash", "credential_intent_crash"] {
            let args = vec![
                "--database-url-file".to_owned(),
                path.display().to_string(),
                "--scenario".to_owned(),
                scenario.to_owned(),
            ];
            let error = ScenarioSettings::from_env_and_args(args.into_iter(), &|name| match name {
                "VESTRACE_FAULT_ISOLATION" => Some("shared".to_owned()),
                "VESTRACE_FAULT_POINT" => Some("after_reserved".to_owned()),
                _ => None,
            })
            .expect_err("a non-ephemeral isolation must be refused for every scenario");
            assert!(error.contains("refusing to run"), "{error}");
        }
    }

    #[test]
    fn a_database_url_file_is_required_by_every_scenario() {
        let error = ScenarioSettings::from_env_and_args(
            [
                "--scenario".to_owned(),
                "credential_intent_crash".to_owned(),
            ]
            .into_iter(),
            &|name| match name {
                "VESTRACE_FAULT_ISOLATION" => Some("ephemeral".to_owned()),
                "VESTRACE_FAULT_POINT" => Some("after_reserved".to_owned()),
                _ => None,
            },
        )
        .expect_err("an intent scenario must still require --database-url-file");
        assert!(error.contains("database url file is required"), "{error}");
    }
}
