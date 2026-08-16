use std::process::Command;

const COMPOSE_FILE: &str = "docker-compose.yml";
const DEFAULT_ADMIN_TOKEN: &str =
    "vst_21d7341d3a4009860168e3cccae642b55123de308fe3c714098b133f75caf863";

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

fn admin_token() -> String {
    match std::env::var("VESTRACE_ADMIN_TOKEN") {
        Ok(token) => token,
        Err(std::env::VarError::NotPresent) => DEFAULT_ADMIN_TOKEN.to_owned(),
        Err(std::env::VarError::NotUnicode(_)) => {
            panic!("VESTRACE_ADMIN_TOKEN must contain valid Unicode")
        }
    }
}

fn curl_authenticated(url: &str) -> (bool, String) {
    let token = admin_token();
    let output = Command::new("curl")
        .args(["-sf", "--header"])
        .arg(format!("authorization: Bearer {token}"))
        .arg(url)
        .output()
        .expect("curl should be available");

    let body = String::from_utf8_lossy(&output.stdout).into_owned();
    (output.status.success(), body)
}

fn base_url_from_port(port_override: Option<&str>) -> Result<String, String> {
    let raw_port = port_override.unwrap_or("8080");
    let port = raw_port.parse::<u16>().map_err(|_| {
        format!("VESTRACE_HTTP_PORT must be a numeric TCP port in 1..=65535, got {raw_port:?}")
    })?;
    if port == 0 {
        return Err("VESTRACE_HTTP_PORT must be a numeric TCP port in 1..=65535, got 0".into());
    }
    Ok(format!("http://127.0.0.1:{port}"))
}

fn base_url() -> String {
    let configured_port = match std::env::var("VESTRACE_HTTP_PORT") {
        Ok(value) => Some(value),
        Err(std::env::VarError::NotPresent) => None,
        Err(std::env::VarError::NotUnicode(_)) => {
            panic!("VESTRACE_HTTP_PORT must contain a numeric TCP port in valid Unicode")
        }
    };
    base_url_from_port(configured_port.as_deref()).unwrap_or_else(|error| panic!("{error}"))
}

fn wait_for_ready(base_url: &str, timeout_secs: u32) -> bool {
    for _ in 0..timeout_secs {
        let (ok, _) = curl(&format!("{base_url}/health/ready"));
        if ok {
            return true;
        }
        std::thread::sleep(std::time::Duration::from_secs(1));
    }
    false
}

#[test]
fn compose_base_url_defaults_to_port_8080() {
    assert_eq!(
        base_url_from_port(None),
        Ok("http://127.0.0.1:8080".to_string())
    );
}

#[test]
fn compose_base_url_uses_an_explicit_valid_port() {
    assert_eq!(
        base_url_from_port(Some("18080")),
        Ok("http://127.0.0.1:18080".to_string())
    );
}

#[test]
fn compose_base_url_rejects_an_invalid_port() {
    let error = base_url_from_port(Some("not-a-port")).expect_err("invalid ports must fail");
    assert!(
        error.contains("VESTRACE_HTTP_PORT") && error.contains("numeric TCP port"),
        "unexpected validation error: {error}"
    );
}

#[test]
#[ignore = "requires docker compose and available ports"]
fn compose_smoke_health_ready() {
    let base_url = base_url();
    let _ = compose_down();

    let up = compose_up();
    assert!(
        up.status.success(),
        "docker compose up failed: {}",
        String::from_utf8_lossy(&up.stderr)
    );

    let ready = wait_for_ready(&base_url, 60);
    assert!(ready, "server did not become ready within 60 seconds");

    let (ok, body) = curl(&format!("{base_url}/health/ready"));
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
    let base_url = base_url();
    let _ = compose_down();

    let up = compose_up();
    assert!(
        up.status.success(),
        "docker compose up failed: {}",
        String::from_utf8_lossy(&up.stderr)
    );

    let ready = wait_for_ready(&base_url, 60);
    assert!(ready, "server did not become ready within 60 seconds");

    let (ok, body) = curl_authenticated(&format!("{base_url}/metrics"));
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
    let base_url = base_url();
    let _ = compose_down();

    let up = compose_up();
    assert!(
        up.status.success(),
        "docker compose up failed: {}",
        String::from_utf8_lossy(&up.stderr)
    );

    let ready = wait_for_ready(&base_url, 60);
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
    let base_url = base_url();
    let _ = compose_down();

    let up = compose_up();
    assert!(
        up.status.success(),
        "docker compose up failed: {}",
        String::from_utf8_lossy(&up.stderr)
    );

    let ready = wait_for_ready(&base_url, 60);
    assert!(ready, "server did not become ready within 60 seconds");

    let (ok, body) = curl_authenticated(&format!("{base_url}/metrics"));
    assert!(ok, "/metrics returned non-2xx");

    assert!(
        !body.contains("query=") && !body.contains("memory_id=") && !body.contains("content="),
        "metrics should not contain sensitive labels, got: {body}"
    );

    let _ = compose_down();
}
