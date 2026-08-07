use std::process::Command;

const COMPOSE_FILE: &str = "docker-compose.yml";

fn compose(args: &[&str]) -> std::process::Output {
    Command::new("docker")
        .arg("compose")
        .arg("-f")
        .arg(COMPOSE_FILE)
        .args(args)
        .output()
        .expect("docker compose should be available")
}

fn compose_up() -> std::process::Output {
    compose(&["up", "--build", "-d"])
}

fn compose_down() -> std::process::Output {
    compose(&["down", "-v"])
}

fn curl(url: &str) -> (bool, String) {
    let output = Command::new("curl")
        .args(["-sf", url])
        .output()
        .expect("curl should be available");

    let body = String::from_utf8_lossy(&output.stdout).into_owned();
    (output.status.success(), body)
}

fn curl_allow_fail(url: &str) -> (bool, String) {
    let output = Command::new("curl")
        .args(["-s", url])
        .output()
        .expect("curl should be available");

    let body = String::from_utf8_lossy(&output.stdout).into_owned();
    (output.status.success(), body)
}

fn wait_for_ready(timeout_secs: u32) -> bool {
    for _ in 0..timeout_secs {
        let (ok, _) = curl("http://127.0.0.1:8080/health/ready");
        if ok {
            return true;
        }
        std::thread::sleep(std::time::Duration::from_secs(1));
    }
    false
}

#[test]
#[ignore = "requires docker compose and available ports"]
fn compose_smoke_health_ready() {
    let _ = compose_down();

    let up = compose_up();
    assert!(
        up.status.success(),
        "docker compose up failed: {}",
        String::from_utf8_lossy(&up.stderr)
    );

    let ready = wait_for_ready(60);
    assert!(ready, "server did not become ready within 60 seconds");

    let (ok, body) = curl("http://127.0.0.1:8080/health/ready");
    assert!(ok, "health/ready returned non-2xx: {body}");
    assert!(
        body.contains("ok") || body.contains("healthy"),
        "unexpected health body: {body}"
    );

    let _ = compose_down();
}

#[test]
#[ignore = "requires docker compose and available ports"]
fn compose_smoke_metrics_endpoint() {
    let _ = compose_down();

    let up = compose_up();
    assert!(
        up.status.success(),
        "docker compose up failed: {}",
        String::from_utf8_lossy(&up.stderr)
    );

    let ready = wait_for_ready(60);
    assert!(ready, "server did not become ready within 60 seconds");

    let (ok, body) = curl("http://127.0.0.1:8080/metrics");
    assert!(ok, "/metrics returned non-2xx: {body}");
    assert!(
        body.contains("vestrace_http_requests_total"),
        "/metrics should contain vestrace metric names, got: {body}"
    );

    let _ = compose_down();
}

#[test]
#[ignore = "requires docker compose and available ports"]
fn compose_smoke_doctor_in_container() {
    let _ = compose_down();

    let up = compose_up();
    assert!(
        up.status.success(),
        "docker compose up failed: {}",
        String::from_utf8_lossy(&up.stderr)
    );

    let ready = wait_for_ready(60);
    assert!(ready, "server did not become ready within 60 seconds");

    let output = compose(&["exec", "vestrace-server", "vestrace", "doctor"]);
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();

    assert!(
        output.status.success() || stderr.contains("warning"),
        "doctor should pass or only warn, stdout: {stdout}, stderr: {stderr}"
    );

    let _ = compose_down();
}

#[test]
#[ignore = "requires docker compose and available ports"]
fn compose_smoke_no_sensitive_labels_in_metrics() {
    let _ = compose_down();

    let up = compose_up();
    assert!(
        up.status.success(),
        "docker compose up failed: {}",
        String::from_utf8_lossy(&up.stderr)
    );

    let ready = wait_for_ready(60);
    assert!(ready, "server did not become ready within 60 seconds");

    let (_, body) = curl_allow_fail("http://127.0.0.1:8080/metrics");

    assert!(
        !body.contains("query=") && !body.contains("memory_id=") && !body.contains("content="),
        "metrics should not contain sensitive labels, got: {body}"
    );

    let _ = compose_down();
}
