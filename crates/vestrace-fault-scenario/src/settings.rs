use vestrace_domain::external_effects::EffectFaultPoint;

/// What one invocation was asked to do, and whether it is allowed to.
///
/// The invoking contract clears the environment and passes only the three
/// `VESTRACE_FAULT_*` variables, so everything else has to arrive through
/// arguments — and the database URL cannot, because argv is readable by any
/// process on the host. It arrives as a path to a file instead.
#[derive(Debug)]
pub struct ScenarioSettings {
    point: EffectFaultPoint,
    database_url: String,
    is_child: bool,
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

        let requested = env("VESTRACE_FAULT_POINT").unwrap_or_default();
        let point = match requested.as_str() {
            "after_intent_persistence" => EffectFaultPoint::AfterIntentPersistence,
            "after_authorization_before_dispatch" => {
                EffectFaultPoint::AfterAuthorizationBeforeDispatch
            }
            "after_dispatch_before_receipt" => EffectFaultPoint::AfterDispatchBeforeReceipt,
            "after_receipt_before_outcome_confirmation" => {
                EffectFaultPoint::AfterReceiptBeforeOutcomeConfirmation
            }
            "after_outcome_before_run_commit" => EffectFaultPoint::AfterOutcomeBeforeRunCommit,
            other => return Err(format!("unknown fault point '{other}'")),
        };

        let mut url_file = None;
        let mut args = args.peekable();
        while let Some(arg) = args.next() {
            if arg == "--database-url-file" {
                url_file = args.next();
            }
        }
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
            point,
            database_url,
            is_child: env("VESTRACE_FAULT_CHILD").is_some(),
        })
    }

    pub fn point(&self) -> EffectFaultPoint {
        self.point
    }

    pub fn database_url(&self) -> &str {
        &self.database_url
    }

    pub fn is_child(&self) -> bool {
        self.is_child
    }
}
