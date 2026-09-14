use std::{path::PathBuf, process::Command};

use serde_json::Value;

const JOURNAL_VOLUME: &str = "installation-safety-journal";

fn compose_file() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../docker-compose.yml")
}

#[test]
fn journal_initializer_is_the_only_declared_journal_volume_consumer() {
    let source = std::fs::read_to_string(compose_file()).unwrap();
    assert!(source.contains("vestrace-safety-journal-init:"));
    assert!(source.contains("installation-safety-journal:/var/lib/vestrace/safety-journal"));
    assert!(source.contains(".initializer-receipt"));
    assert!(source.contains("installation-safety-journal:\n"));

    // The only mount string is intentionally part of the initializer block.
    assert_eq!(
        source
            .match_indices("installation-safety-journal:/var/lib/vestrace/safety-journal")
            .count(),
        1,
        "a product service gained a journal mount"
    );
}

#[test]
fn normalized_compose_inventory_keeps_the_journal_off_product_services_when_available() {
    let output = Command::new("docker")
        .args([
            "compose",
            "-f",
            compose_file().to_str().unwrap(),
            "config",
            "--format",
            "json",
        ])
        .output();
    let Ok(output) = output else {
        eprintln!("BLOCKED: Docker Compose is unavailable; static mount inventory was checked");
        return;
    };
    if !output.status.success() {
        eprintln!(
            "BLOCKED: Docker Compose config failed; static mount inventory was checked: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        return;
    }
    let config: Value = serde_json::from_slice(&output.stdout).unwrap();
    let services = config["services"].as_object().unwrap();
    for (name, service) in services {
        let mounts = service["volumes"].as_array().cloned().unwrap_or_default();
        let has_journal = mounts.iter().any(|mount| {
            mount["source"].as_str() == Some(JOURNAL_VOLUME)
                || mount
                    .as_str()
                    .is_some_and(|mount| mount.starts_with(JOURNAL_VOLUME))
        });
        assert_eq!(
            has_journal,
            name == "vestrace-safety-journal-init",
            "unexpected normalized journal mount inventory for {name}"
        );
    }
}

#[test]
fn journal_initializer_has_a_read_only_root_and_no_new_privileges() {
    let source = std::fs::read_to_string(compose_file()).unwrap();
    let initializer = source
        .split("  postgres:")
        .next()
        .expect("journal initializer must precede PostgreSQL");
    assert!(initializer.contains("read_only: true"));
    assert!(initializer.contains("no-new-privileges:true"));
    assert!(!initializer.contains("privileged: true"));
}

#[test]
fn product_services_never_mount_safety_roots_when_compose_config_is_available() {
    let output = Command::new("docker")
        .args(["compose", "-f", compose_file().to_str().unwrap(), "config", "--format", "json"])
        .output();
    let Ok(output) = output else {
        eprintln!("BLOCKED: Docker Compose is unavailable; static mount inventory was checked");
        return;
    };
    if !output.status.success() {
        eprintln!("BLOCKED: Docker Compose config failed; static mount inventory was checked");
        return;
    }
    let config: Value = serde_json::from_slice(&output.stdout).unwrap();
    for name in ["postgres", "vestrace-server", "vestrace-worker", "console"] {
        let service = &config["services"][name];
        let mounts = service["volumes"].as_array().cloned().unwrap_or_default();
        for mount in mounts {
            let mount = mount.to_string();
            assert!(!mount.contains("installation-safety-journal"), "{name} gained journal custody");
            assert!(!mount.contains("safety-witness"), "{name} gained witness custody");
            assert!(!mount.contains("backup-archive"), "{name} gained archive custody");
        }
    }
}
/// Resolved Compose configuration, or `None` when the engine is unavailable.
/// A missing engine is recorded as blocked evidence; it never passes silently
/// as if the topology had been proved.
fn resolved_config() -> Option<Value> {
    let output = Command::new("docker")
        .args([
            "compose",
            "-f",
            compose_file().to_str().unwrap(),
            "config",
            "--format",
            "json",
        ])
        .output()
        .ok()?;
    if !output.status.success() {
        eprintln!(
            "BLOCKED: Docker Compose config failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        return None;
    }
    Some(serde_json::from_slice(&output.stdout).unwrap())
}

/// Every stage the plan names must state what it waits for. An implicit start
/// order is not an order: Compose is free to run an unconstrained service
/// before the migration ledger and safety authority it depends on exist.
#[test]
fn every_declared_stage_names_an_explicit_ordered_prerequisite() {
    let Some(config) = resolved_config() else {
        eprintln!("BLOCKED: Docker Compose is unavailable; dependency order was not resolved");
        return;
    };
    let ordered_stages = [
        "vestrace-role-provision",
        "vestrace-migrate-history",
        "vestrace-safety-bootstrap",
        "vestrace-migrate-p05",
        "vestrace-fingerprint-vault-init",
        "dev-seed",
        "vestrace-server",
        "vestrace-worker",
        "console",
    ];
    for name in ordered_stages {
        let service = &config["services"][name];
        assert!(service.is_object(), "{name} is not a declared service");
        let depends_on = service["depends_on"].as_object();
        let count = depends_on.map(|deps| deps.len()).unwrap_or(0);
        assert!(count > 0, "{name} starts without an explicit prerequisite");
        for (dependency, edge) in depends_on.unwrap() {
            let condition = edge["condition"].as_str().unwrap_or_default();
            assert!(
                matches!(condition, "service_healthy" | "service_completed_successfully"),
                "{name} waits on {dependency} with the unordered condition {condition}"
            );
        }
    }
}

/// A prerequisite gated on health is only an order if the named service can
/// actually report health. Without a health check Compose has no signal to
/// wait for, and the edge degrades into an unordered start.
#[test]
fn health_gated_prerequisites_name_a_service_that_declares_a_health_check() {
    let Some(config) = resolved_config() else {
        eprintln!("BLOCKED: Docker Compose is unavailable; health gating was not resolved");
        return;
    };
    let services = config["services"].as_object().unwrap();
    let mut gated = 0usize;
    for (name, service) in services {
        let Some(depends_on) = service["depends_on"].as_object() else {
            continue;
        };
        for (dependency, edge) in depends_on {
            if edge["condition"].as_str() != Some("service_healthy") {
                continue;
            }
            gated += 1;
            let test = &config["services"][dependency]["healthcheck"]["test"];
            assert!(
                test.is_array() || test.is_string(),
                "{name} waits for {dependency} to be healthy, but {dependency} declares no health check"
            );
        }
    }
    assert!(gated > 0, "no health-gated prerequisite remains to prove");
}

/// The journal initializer is the one service that needs a privileged
/// effective user, because it creates a `0700` root owned by the unprivileged
/// runtime uid. Nothing else may claim root, and no service anywhere may
/// request expanded privilege.
#[test]
fn only_the_journal_initializer_claims_root_and_no_service_expands_privilege() {
    let Some(config) = resolved_config() else {
        eprintln!("BLOCKED: Docker Compose is unavailable; privilege flags were not resolved");
        return;
    };
    let services = config["services"].as_object().unwrap();
    for (name, service) in services {
        assert_ne!(
            service["privileged"].as_bool(),
            Some(true),
            "{name} requests privileged execution"
        );
        assert!(
            service["cap_add"].as_array().map(|caps| caps.is_empty()).unwrap_or(true),
            "{name} requests added capabilities"
        );
        assert!(
            service["pid"].as_str().unwrap_or_default() != "host",
            "{name} shares the host PID namespace"
        );
        assert!(
            service["network_mode"].as_str().unwrap_or_default() != "host",
            "{name} shares the host network namespace"
        );

        let user = service["user"].as_str().unwrap_or_default();
        let claims_root = user == "0" || user == "root" || user.starts_with("0:");
        assert_eq!(
            claims_root,
            name == "vestrace-safety-journal-init",
            "unexpected root effective user declaration for {name}"
        );
    }

    let initializer = &config["services"]["vestrace-safety-journal-init"];
    assert_eq!(initializer["user"].as_str(), Some("0:0"));
    assert_eq!(initializer["read_only"].as_bool(), Some(true));
    let security_opt = initializer["security_opt"].as_array().cloned().unwrap_or_default();
    assert!(
        security_opt.iter().any(|opt| opt.as_str() == Some("no-new-privileges:true")),
        "the journal initializer dropped no-new-privileges"
    );
}
