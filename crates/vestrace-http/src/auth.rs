use std::sync::Arc;

use axum::{
    body::Body,
    http::{HeaderMap, HeaderValue, Request, StatusCode},
    middleware::Next,
    response::Response,
};
use vestrace_application::SharedAccessTokenAuthenticator;
use vestrace_domain::identity::hash_presented_token;

const BEARER_PREFIX: &str = "Bearer ";
const WORKSPACE_ID_HEADER: &str = "x-workspace-id";
const PRINCIPAL_ID_HEADER: &str = "x-principal-id";

/// How a request is authenticated.
///
/// # What changed and why
///
/// This used to be a single shared token compared against a value read from the
/// process environment, mapped to one hardcoded `(workspace, principal)` pair.
/// Every authenticated request in the system was therefore the same
/// administrator: audit rows, run events and approvals all recorded an identity
/// that said nothing about who acted. No capability, delegation or governance
/// requirement can be met without attribution, and there was none.
///
/// Now a credential resolves to the principal it was issued to.
#[derive(Clone)]
pub struct Authentication {
    authenticator: SharedAccessTokenAuthenticator,
}

impl Authentication {
    pub fn new(authenticator: SharedAccessTokenAuthenticator) -> Self {
        Self { authenticator }
    }
}

impl std::fmt::Debug for Authentication {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Explicit rather than derived. This no longer holds a token, and the
        // impl exists so that it cannot start holding one silently.
        formatter.debug_struct("Authentication").finish()
    }
}

fn unauthorized() -> Response {
    let mut response = Response::builder()
        .status(StatusCode::UNAUTHORIZED)
        // Says what is required, never what was supplied, and never which of
        // "unknown", "expired" or "revoked" applies — distinguishing those
        // tells a caller whether they guessed a real credential.
        .body(Body::from(
            r#"{"code":"unauthorized","message":"a valid Vestrace access token is required"}"#,
        ))
        .expect("static unauthorized response is valid");
    response
        .headers_mut()
        .insert("content-type", HeaderValue::from_static("application/json"));
    response
}

/// Liveness and readiness probes answer before authentication.
///
/// An orchestrator's probe cannot present a credential, and answering it with
/// 401 would make "the process is wedged" indistinguishable from "the probe is
/// not authorized" — the container would be restarted for the wrong reason.
/// These two routes report only whether the process and its database are
/// reachable, which is what a caller learns anyway by opening the port.
///
/// `/metrics` is deliberately **not** exempt: it reports per-workspace activity.
fn is_public_probe(path: &str) -> bool {
    matches!(path, "/health/live" | "/health/ready")
}

fn bearer_token(headers: &HeaderMap) -> Option<&str> {
    headers
        .get("authorization")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix(BEARER_PREFIX))
}

/// Authenticate the caller and take identity out of the caller's hands.
///
/// On success the client-supplied identity headers are **replaced** with the
/// ones the credential resolved to, so a caller cannot assert who it is.
/// Downstream handlers keep reading the same headers and cannot be lied to.
pub async fn authenticate(
    authentication: Authentication,
    mut request: Request<Body>,
    next: Next,
) -> Response {
    if is_public_probe(request.uri().path()) {
        return next.run(request).await;
    }

    let Some(presented) = bearer_token(request.headers()) else {
        return unauthorized();
    };

    // Hashed before it goes anywhere near the store, so the plaintext never
    // reaches an adapter, a query log or an error path. A malformed credential
    // fails here and is reported exactly like an unknown one.
    let Ok(token_hash) = hash_presented_token(presented) else {
        return unauthorized();
    };

    let resolved = match authentication.authenticator.resolve(&token_hash).await {
        Ok(Some(resolved)) => resolved,
        Ok(None) => return unauthorized(),
        Err(error) => {
            // A storage failure is not an authentication failure, and must not
            // be reported as one: 401 would send an operator hunting for a bad
            // credential when the database is down.
            tracing::error!(error = %error, "access token could not be resolved");
            return Response::builder()
                .status(StatusCode::SERVICE_UNAVAILABLE)
                .header("content-type", "application/json")
                .body(Body::from(
                    r#"{"code":"unavailable","message":"authentication is temporarily unavailable"}"#,
                ))
                .expect("static unavailable response is valid");
        }
    };

    let headers = request.headers_mut();
    let (Ok(workspace), Ok(principal)) = (
        HeaderValue::from_str(&resolved.workspace_id.to_string()),
        HeaderValue::from_str(&resolved.principal_id.to_string()),
    ) else {
        return unauthorized();
    };
    headers.insert(WORKSPACE_ID_HEADER, workspace);
    headers.insert(PRINCIPAL_ID_HEADER, principal);

    // Recorded after the identity is settled and before the handler runs, so a
    // long request still shows the credential as used. A failure here is logged
    // and ignored: refusing a valid credential because a usage timestamp could
    // not be written would be a self-inflicted outage.
    let authenticator = Arc::clone(&authentication.authenticator);
    let token_id = resolved.token_id;
    if let Err(error) = authenticator.record_use(token_id).await {
        tracing::warn!(error = %error, "access token use could not be recorded");
    }

    next.run(request).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use std::sync::Mutex;
    use vestrace_application::{
        AccessTokenAuthenticator, ApplicationError, AuthenticatedPrincipal,
    };
    use vestrace_domain::{AccessTokenId, PrincipalId, WorkspaceId};

    #[derive(Default)]
    struct RecordingAuthenticator {
        resolved: Option<AuthenticatedPrincipal>,
        fail: bool,
        seen_hashes: Mutex<Vec<String>>,
        uses: Mutex<Vec<AccessTokenId>>,
    }

    #[async_trait]
    impl AccessTokenAuthenticator for RecordingAuthenticator {
        async fn resolve(
            &self,
            token_hash: &str,
        ) -> Result<Option<AuthenticatedPrincipal>, ApplicationError> {
            self.seen_hashes
                .lock()
                .unwrap()
                .push(token_hash.to_string());
            if self.fail {
                return Err(ApplicationError::Storage("database is down".into()));
            }
            Ok(self.resolved)
        }

        async fn record_use(&self, token_id: AccessTokenId) -> Result<(), ApplicationError> {
            self.uses.lock().unwrap().push(token_id);
            Ok(())
        }
    }

    #[test]
    fn liveness_and_readiness_answer_without_a_credential() {
        assert!(is_public_probe("/health/live"));
        assert!(is_public_probe("/health/ready"));
    }

    #[test]
    fn metrics_is_not_a_public_probe() {
        // It reports per-workspace activity, unlike the probes.
        assert!(!is_public_probe("/metrics"));
    }

    #[test]
    fn the_exemption_does_not_extend_beyond_the_two_probes() {
        // An exact match, so a path that merely begins with or traverses out of
        // the probe prefix stays authenticated.
        assert!(!is_public_probe("/health"));
        assert!(!is_public_probe("/health/live/../../v1/runs"));
        assert!(!is_public_probe("/health/livez"));
        assert!(!is_public_probe("/v1/runs"));
    }

    #[tokio::test]
    async fn the_plaintext_credential_never_reaches_the_authenticator() {
        // The whole point of hashing in the middleware: an adapter, a query log
        // and an error path all see the hash and never the token.
        let token = mint_token();
        let authenticator = Arc::new(RecordingAuthenticator {
            resolved: Some(principal()),
            ..Default::default()
        });
        let authentication = Authentication::new(authenticator.clone());

        let response = run_request(authentication, Some(&token)).await;
        assert_eq!(response.status(), StatusCode::OK);

        let seen = authenticator.seen_hashes.lock().unwrap().clone();
        assert_eq!(seen.len(), 1);
        assert!(!seen[0].contains(&token));
        assert_eq!(seen[0].len(), 64);
    }

    #[tokio::test]
    async fn an_unresolvable_credential_is_refused() {
        let authenticator = Arc::new(RecordingAuthenticator::default());
        let response = run_request(Authentication::new(authenticator), Some(&mint_token())).await;
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn a_malformed_credential_is_refused_without_reaching_the_store() {
        // A provider key sent by mistake must not become a lookup.
        let authenticator = Arc::new(RecordingAuthenticator {
            resolved: Some(principal()),
            ..Default::default()
        });
        let response = run_request(
            Authentication::new(authenticator.clone()),
            Some("sk-a-provider-key"),
        )
        .await;
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        assert!(authenticator.seen_hashes.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn a_storage_failure_is_not_reported_as_a_bad_credential() {
        // 401 here would send an operator hunting for a wrong token while the
        // database is down.
        let authenticator = Arc::new(RecordingAuthenticator {
            fail: true,
            ..Default::default()
        });
        let response = run_request(Authentication::new(authenticator), Some(&mint_token())).await;
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    }

    #[tokio::test]
    async fn the_caller_cannot_assert_its_own_identity() {
        // Identity headers supplied by the client are replaced, not merged.
        let resolved = principal();
        let authenticator = Arc::new(RecordingAuthenticator {
            resolved: Some(resolved),
            ..Default::default()
        });

        let seen = Arc::new(Mutex::new(None));
        let captured = seen.clone();
        let app = axum::Router::new()
            .route(
                "/v1/runs",
                axum::routing::get(move |headers: HeaderMap| {
                    let captured = captured.clone();
                    async move {
                        *captured.lock().unwrap() = Some((
                            headers
                                .get(WORKSPACE_ID_HEADER)
                                .and_then(|v| v.to_str().ok())
                                .map(str::to_owned),
                            headers
                                .get(PRINCIPAL_ID_HEADER)
                                .and_then(|v| v.to_str().ok())
                                .map(str::to_owned),
                        ));
                        StatusCode::OK
                    }
                }),
            )
            .layer(axum::middleware::from_fn(move |request, next| {
                let authentication = Authentication::new(authenticator.clone());
                async move { authenticate(authentication, request, next).await }
            }));

        let request = Request::builder()
            .uri("/v1/runs")
            .header("authorization", format!("Bearer {}", mint_token()))
            // A caller trying to be somebody else.
            .header(WORKSPACE_ID_HEADER, uuid::Uuid::nil().to_string())
            .header(PRINCIPAL_ID_HEADER, uuid::Uuid::nil().to_string())
            .body(Body::empty())
            .unwrap();

        use tower::ServiceExt;
        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let (workspace, principal_header) = seen.lock().unwrap().clone().unwrap();
        assert_eq!(workspace, Some(resolved.workspace_id.to_string()));
        assert_eq!(principal_header, Some(resolved.principal_id.to_string()));
        assert_ne!(workspace, Some(uuid::Uuid::nil().to_string()));
    }

    #[tokio::test]
    async fn a_successful_authentication_records_the_credential_as_used() {
        let resolved = principal();
        let authenticator = Arc::new(RecordingAuthenticator {
            resolved: Some(resolved),
            ..Default::default()
        });
        let response = run_request(
            Authentication::new(authenticator.clone()),
            Some(&mint_token()),
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            authenticator.uses.lock().unwrap().clone(),
            vec![resolved.token_id]
        );
    }

    fn principal() -> AuthenticatedPrincipal {
        AuthenticatedPrincipal {
            token_id: AccessTokenId::new(),
            workspace_id: WorkspaceId::new(),
            principal_id: PrincipalId::new(),
        }
    }

    fn mint_token() -> String {
        use vestrace_domain::identity::{AccessToken, TOKEN_PREFIX};
        let _ = TOKEN_PREFIX;
        AccessToken::issue(
            AccessTokenId::new(),
            WorkspaceId::new(),
            PrincipalId::new(),
            "test",
            &[3u8; 32],
            None,
            vestrace_domain::time::now(),
        )
        .expect("a well-formed token")
        .into_token()
    }

    async fn run_request(authentication: Authentication, token: Option<&str>) -> Response {
        use tower::ServiceExt;

        let app = axum::Router::new()
            .route("/v1/runs", axum::routing::get(|| async { StatusCode::OK }))
            .layer(axum::middleware::from_fn(move |request, next| {
                let authentication = authentication.clone();
                async move { authenticate(authentication, request, next).await }
            }));

        let mut builder = Request::builder().uri("/v1/runs");
        if let Some(token) = token {
            builder = builder.header("authorization", format!("Bearer {token}"));
        }
        app.oneshot(builder.body(Body::empty()).unwrap())
            .await
            .unwrap()
    }
}
