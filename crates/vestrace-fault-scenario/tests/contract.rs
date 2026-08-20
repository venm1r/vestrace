use std::collections::HashMap;
use vestrace_fault_scenario::ScenarioSettings;

fn env_of(pairs: &[(&str, &str)]) -> HashMap<String, String> {
    pairs
        .iter()
        .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
        .collect()
}

fn settings_from(args: &[&str], env: &HashMap<String, String>) -> Result<ScenarioSettings, String> {
    let owned: Vec<String> = args.iter().map(|a| (*a).to_owned()).collect();
    ScenarioSettings::from_env_and_args(owned.into_iter(), &|name| env.get(name).cloned())
}

/// This program kills processes mid-transaction against whatever database it
/// is handed. The isolation claim is the only thing between that and somebody's
/// data, so it is checked rather than assumed.
#[test]
fn any_isolation_but_ephemeral_is_refused() {
    let url_file = std::env::temp_dir().join(format!("vfs-url-{}.txt", std::process::id()));
    std::fs::write(&url_file, "postgres://u:p@127.0.0.1:5432/db").unwrap();
    let env = env_of(&[
        ("VESTRACE_FAULT_POINT", "after_intent_persistence"),
        ("VESTRACE_FAULT_ISOLATION", "designated_non_production"),
    ]);

    let error = settings_from(&["--database-url-file", url_file.to_str().unwrap()], &env)
        .expect_err("a non-ephemeral isolation must be refused");

    assert!(error.contains("ephemeral"), "unhelpful refusal: {error}");
    std::fs::remove_file(&url_file).ok();
}

/// The invoking contract clears the environment, so the URL cannot arrive that
/// way; it must not arrive in argv either, where any process on the host can
/// read it.
#[test]
fn the_database_url_is_read_from_a_file_not_an_argument() {
    let url_file = std::env::temp_dir().join(format!("vfs-url2-{}.txt", std::process::id()));
    std::fs::write(&url_file, "  postgres://u:p@127.0.0.1:5432/db\n").unwrap();
    let env = env_of(&[
        ("VESTRACE_FAULT_POINT", "after_intent_persistence"),
        ("VESTRACE_FAULT_ISOLATION", "ephemeral"),
    ]);

    let settings = settings_from(&["--database-url-file", url_file.to_str().unwrap()], &env)
        .expect("a well-formed invocation must be accepted");

    assert_eq!(settings.database_url(), "postgres://u:p@127.0.0.1:5432/db");
    std::fs::remove_file(&url_file).ok();
}

/// `Debug` is how a panic message or a log line renders this struct, and
/// `expect_err` above uses it whenever the invocation unexpectedly succeeds.
/// None of those call sites should be able to print the password.
#[test]
fn the_debug_rendering_never_prints_the_database_password() {
    let url_file = std::env::temp_dir().join(format!("vfs-url4-{}.txt", std::process::id()));
    let url = "postgres://u:s3cr3t@127.0.0.1:5432/db";
    std::fs::write(&url_file, url).unwrap();
    let env = env_of(&[
        ("VESTRACE_FAULT_POINT", "after_intent_persistence"),
        ("VESTRACE_FAULT_ISOLATION", "ephemeral"),
    ]);

    let settings = settings_from(&["--database-url-file", url_file.to_str().unwrap()], &env)
        .expect("a well-formed invocation must be accepted");

    let rendered = format!("{settings:?}");
    assert!(
        !rendered.contains("s3cr3t"),
        "debug rendering leaked the password: {rendered}"
    );
    assert!(
        !rendered.contains(url),
        "debug rendering leaked the full connection string: {rendered}"
    );
    std::fs::remove_file(&url_file).ok();
}

#[test]
fn a_missing_url_file_is_refused() {
    let env = env_of(&[
        ("VESTRACE_FAULT_POINT", "after_intent_persistence"),
        ("VESTRACE_FAULT_ISOLATION", "ephemeral"),
    ]);

    let error = settings_from(&["--database-url-file", "/nonexistent/vfs"], &env)
        .expect_err("a missing url file must be refused");

    assert!(
        error.contains("database url file"),
        "unhelpful refusal: {error}"
    );
}

#[test]
fn an_unknown_fault_point_is_refused() {
    let url_file = std::env::temp_dir().join(format!("vfs-url3-{}.txt", std::process::id()));
    std::fs::write(&url_file, "postgres://u:p@127.0.0.1:5432/db").unwrap();
    let env = env_of(&[
        ("VESTRACE_FAULT_POINT", "after_lunch"),
        ("VESTRACE_FAULT_ISOLATION", "ephemeral"),
    ]);

    let error = settings_from(&["--database-url-file", url_file.to_str().unwrap()], &env)
        .expect_err("an unknown fault point must be refused");

    assert!(
        error.contains("after_lunch"),
        "the refusal must name what it got: {error}"
    );
    std::fs::remove_file(&url_file).ok();
}

/// The cheapest way for this whole crate to become worthless is for somebody to
/// fill an awkward field with the answer the suite wants. The suite's own
/// `expected` constructor is that answer.
#[test]
fn the_crate_never_constructs_the_expected_observation() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut offenders = Vec::new();
    let mut dirs = vec![root];
    while let Some(dir) = dirs.pop() {
        for entry in std::fs::read_dir(&dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                dirs.push(path);
                continue;
            }
            if path.extension().is_some_and(|e| e == "rs") {
                let text = std::fs::read_to_string(&path).unwrap();
                if text.contains("FaultObservation::expected") {
                    offenders.push(path.display().to_string());
                }
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "an observation must be found, not stated; offenders: {offenders:?}"
    );
}

/// `confirm_reached_point` is unit-tested in `tests/child_points.rs`, but a
/// check nothing calls refuses nothing. The parent is the only caller that
/// matters and there is exactly one place it can go — between the child exiting
/// and the parent reading the database — so this asserts it is still there.
#[test]
fn the_parent_admits_only_a_child_that_reached_its_point() {
    let main = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/main.rs"),
    )
    .unwrap();
    assert!(
        main.contains("confirm_reached_point"),
        "the parent must refuse a child that stopped short of its fault point, or every \
         setup failure becomes an observation of the point it never reached"
    );
}

/// A hook that aborts the process must never reach the shipped image. The image
/// builds one package and copies one path, so this is a guard on that staying
/// true.
#[test]
fn the_shipped_image_does_not_build_this_binary() {
    let dockerfile = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../Dockerfile"),
    )
    .unwrap();
    assert!(
        !dockerfile.contains("vestrace-fault-scenario"),
        "the scenario binary must not enter the image"
    );
}
