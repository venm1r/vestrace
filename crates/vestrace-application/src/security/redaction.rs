use regex::Regex;
use std::sync::Arc;
use vestrace_domain::Sensitivity;

pub struct RedactionRule {
    pub name: String,
    pub pattern: Regex,
    pub replacement: String,
    pub sensitivity: Sensitivity,
}

pub struct RedactionService {
    rules: Vec<Arc<RedactionRule>>,
}

impl RedactionService {
    pub fn new() -> Self {
        Self { rules: Vec::new() }
    }

    pub fn with_rule(mut self, rule: RedactionRule) -> Self {
        self.rules.push(Arc::new(rule));
        self
    }

    pub fn redact(&self, text: &str, destination: vestrace_domain::DataDestination) -> String {
        let mut result = text.to_owned();
        for rule in &self.rules {
            if should_redact(rule.sensitivity, destination) {
                result = rule
                    .pattern
                    .replace_all(&result, rule.replacement.as_str())
                    .into_owned();
            }
        }
        result
    }

    pub fn redact_json(
        &self,
        value: &serde_json::Value,
        destination: vestrace_domain::DataDestination,
    ) -> serde_json::Value {
        match value {
            serde_json::Value::String(s) => serde_json::Value::String(self.redact(s, destination)),
            serde_json::Value::Object(map) => {
                let mut new_map = serde_json::Map::new();
                for (k, v) in map {
                    new_map.insert(k.clone(), self.redact_json(v, destination));
                }
                serde_json::Value::Object(new_map)
            }
            serde_json::Value::Array(arr) => serde_json::Value::Array(
                arr.iter()
                    .map(|v| self.redact_json(v, destination))
                    .collect(),
            ),
            other => other.clone(),
        }
    }

    pub fn rule_count(&self) -> usize {
        self.rules.len()
    }
}

impl Default for RedactionService {
    fn default() -> Self {
        Self::new()
    }
}

fn should_redact(sensitivity: Sensitivity, destination: vestrace_domain::DataDestination) -> bool {
    use vestrace_domain::DataDestination::*;
    use vestrace_domain::Sensitivity::*;

    match (sensitivity, destination) {
        (Public, _) => false,
        (Internal, LocalModel) => false,
        (Internal, AuditStore) => false,
        (Internal, _) => true,
        (Confidential, AuditStore) => false,
        (Confidential, _) => true,
        (Restricted, _) => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redact_replaces_secrets_in_text() {
        let service = RedactionService::new().with_rule(RedactionRule {
            name: "api_key".to_owned(),
            pattern: Regex::new(r"sk-[a-zA-Z0-9]+").unwrap(),
            replacement: "[REDACTED]".to_owned(),
            sensitivity: Sensitivity::Restricted,
        });

        let result = service.redact(
            "use key sk-abc123 for access",
            vestrace_domain::DataDestination::RemoteProvider,
        );
        assert_eq!(result, "use key [REDACTED] for access");
    }

    #[test]
    fn redact_preserves_public_data() {
        let service = RedactionService::new().with_rule(RedactionRule {
            name: "api_key".to_owned(),
            pattern: Regex::new(r"sk-[a-zA-Z0-9]+").unwrap(),
            replacement: "[REDACTED]".to_owned(),
            sensitivity: Sensitivity::Public,
        });

        let result = service.redact(
            "use key sk-abc123 for access",
            vestrace_domain::DataDestination::RemoteProvider,
        );
        assert_eq!(result, "use key sk-abc123 for access");
    }

    #[test]
    fn redact_json_recurses() {
        let service = RedactionService::new().with_rule(RedactionRule {
            name: "api_key".to_owned(),
            pattern: Regex::new(r"sk-[a-zA-Z0-9]+").unwrap(),
            replacement: "[REDACTED]".to_owned(),
            sensitivity: Sensitivity::Restricted,
        });

        let json = serde_json::json!({
            "key": "sk-secret123",
            "nested": {"inner": "sk-inner456"},
            "number": 42
        });

        let result = service.redact_json(&json, vestrace_domain::DataDestination::RemoteProvider);
        assert_eq!(result["key"], "[REDACTED]");
        assert_eq!(result["nested"]["inner"], "[REDACTED]");
        assert_eq!(result["number"], 42);
    }

    #[test]
    fn no_rules_passes_through() {
        let service = RedactionService::new();
        let result = service.redact(
            "secret data",
            vestrace_domain::DataDestination::RemoteProvider,
        );
        assert_eq!(result, "secret data");
    }
}
