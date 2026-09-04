use std::net::{IpAddr, SocketAddr, ToSocketAddrs};
use std::sync::Arc;
use std::time::Duration;

#[cfg(debug_assertions)]
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use async_trait::async_trait;
use reqwest::header::{HeaderName, HeaderValue};
use reqwest::{Client, Method, RequestBuilder, Response, StatusCode, Url};
use serde::de::DeserializeOwned;
use vestrace_application::{
    ChatCompletionsMode, ChatCompletionsOutput, ChatCompletionsRequest, ChatCompletionsResponse,
    ChatSseEvent, ConnectionAuth, ConnectionKind, EffectiveChatCompletionsRequest,
    EffectiveChatEvidence, EffectiveChatFinishReason, EffectiveChatResult,
    EffectiveEmbeddingsRequest, EffectiveModelRequest, EffectiveModelResponse, EmbeddingsRequest,
    EmbeddingsResponse, GenerationRequest, GenerationResponse, MAX_EFFECTIVE_MATERIAL_BYTES,
    ModelsListRequest, ModelsListResponse, ProviderEgress, ProviderError, ProviderResponseError,
    ProviderUsage, Q1ChatMessage, Q1ChatProbeRequest, Q1ChatProbeResult, Q1EmbeddingUsage,
    Q1EmbeddingsProbeRequest, Q1EmbeddingsProbeResult, Q1ModelsListProbeResult, Q1ProbeFailure,
    Q1ProbeRequest, Q1ProbeResponse, Q1ResponseFormat, Q1ToolChoice, Q1ToolSet,
    TextGenerationProvider, run::GovernedModelAdapter,
};

use vestrace_domain::DataDestination;
use zeroize::{Zeroize, Zeroizing};

/// How long to wait for a provider before treating it as unavailable.
///
/// Generous, because a large completion legitimately takes a while, but finite:
/// without a deadline a wedged provider holds a worker slot forever, and the
/// run it is executing never reaches a terminal state.
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(120);
const DEFAULT_CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const DEFAULT_READ_TIMEOUT: Duration = Duration::from_secs(120);
const MAX_RESPONSE_BYTES: usize = 1024 * 1024;
const MAX_SSE_RESPONSE_BYTES: usize = 2 * 1024 * 1024;
const MAX_SSE_EVENTS: usize = 4096;
const SSE_IDLE_TIMEOUT: Duration = Duration::from_secs(10);
const SSE_TOTAL_TIMEOUT: Duration = Duration::from_secs(30);
const MAX_ERROR_FIELD_CHARS: usize = 96;
const MAX_CORRELATION_ID_CHARS: usize = 128;

#[derive(serde::Serialize)]
struct EffectiveChatWireRequest<'a> {
    model: &'a str,
    messages: Vec<EffectiveChatWireMessage<'a>>,
    temperature: f64,
    top_p: f64,
    max_tokens: u32,
    stream: bool,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    tools: Vec<EffectiveChatWireTool<'a>>,
}

#[derive(serde::Serialize)]
struct EffectiveChatWireMessage<'a> {
    role: &'static str,
    content: &'a str,
}

#[derive(serde::Serialize)]
struct EffectiveChatWireTool<'a> {
    #[serde(rename = "type")]
    kind: &'static str,
    function: EffectiveChatWireFunction<'a>,
}

#[derive(serde::Serialize)]
struct EffectiveChatWireFunction<'a> {
    name: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    description: Option<&'a str>,
    parameters: &'a serde_json::Value,
}

#[derive(serde::Serialize)]
struct EffectiveEmbeddingsWireRequest<'a> {
    model: &'a str,
    input: Vec<&'a str>,
    encoding_format: &'static str,
}

#[derive(serde::Serialize)]
struct Q1ChatWireRequest<'a> {
    model: &'a str,
    messages: Vec<Q1ChatWireMessage<'a>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f64>,
    stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    stream_options: Option<Q1StreamOptions>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    tools: Vec<Q1WireTool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_choice: Option<Q1WireToolChoice>,
    #[serde(skip_serializing_if = "Option::is_none")]
    parallel_tool_calls: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    response_format: Option<Q1WireResponseFormat>,
}

#[derive(serde::Serialize)]
#[serde(untagged)]
enum Q1ChatWireMessage<'a> {
    Text {
        role: &'static str,
        content: &'a str,
    },
    AssistantToolCall {
        role: &'static str,
        tool_calls: [Q1WireToolCall<'a>; 1],
    },
    ToolResult {
        role: &'static str,
        content: &'a str,
        tool_call_id: &'a str,
    },
    Image {
        role: &'static str,
        content: [Q1ImagePart<'a>; 2],
    },
}

#[derive(serde::Serialize)]
struct Q1WireToolCall<'a> {
    id: &'a str,
    #[serde(rename = "type")]
    kind: &'static str,
    function: Q1WireToolCallFunction<'a>,
}

#[derive(serde::Serialize)]
struct Q1WireToolCallFunction<'a> {
    name: &'a str,
    arguments: String,
}

#[derive(serde::Serialize)]
#[serde(tag = "type")]
enum Q1ImagePart<'a> {
    #[serde(rename = "text")]
    Text { text: &'a str },
    #[serde(rename = "image_url")]
    ImageUrl { image_url: Q1ImageUrl<'a> },
}

#[derive(serde::Serialize)]
struct Q1ImageUrl<'a> {
    url: &'a str,
}

#[derive(serde::Serialize)]
struct Q1StreamOptions {
    include_usage: bool,
}

#[derive(serde::Serialize)]
struct Q1WireTool {
    #[serde(rename = "type")]
    kind: &'static str,
    function: Q1WireToolFunction,
}

#[derive(serde::Serialize)]
struct Q1WireToolFunction {
    name: &'static str,
    parameters: serde_json::Value,
}

#[derive(serde::Serialize)]
#[serde(untagged)]
enum Q1WireToolChoice {
    Required(&'static str),
    Named {
        #[serde(rename = "type")]
        kind: &'static str,
        function: Q1NamedFunction,
    },
}

#[derive(serde::Serialize)]
struct Q1NamedFunction {
    name: &'static str,
}

#[derive(serde::Serialize)]
struct Q1WireResponseFormat {
    #[serde(rename = "type")]
    kind: &'static str,
    json_schema: Q1StrictSchema,
}

#[derive(serde::Serialize)]
struct Q1StrictSchema {
    name: &'static str,
    strict: bool,
    schema: serde_json::Value,
}

#[derive(serde::Serialize)]
struct Q1EmbeddingsWireRequest<'a> {
    model: &'a str,
    input: Vec<&'a str>,
    encoding_format: &'static str,
}

#[cfg(debug_assertions)]
#[doc(hidden)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(usize)]
pub enum DebugGovernedBufferKind {
    JsonBody = 0,
    SseBody = 1,
    ParserScratch = 2,
    Content = 3,
    AuthHeader = 4,
}

#[cfg(not(debug_assertions))]
#[derive(Clone, Copy)]
#[repr(usize)]
enum DebugGovernedBufferKind {
    JsonBody = 0,
    SseBody = 1,
    ParserScratch = 2,
    Content = 3,
    AuthHeader = 4,
}

#[cfg(debug_assertions)]
#[doc(hidden)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DebugGovernedBufferMetadata {
    pub pointer: usize,
    pub initialized_len: usize,
    pub capacity: usize,
}

#[cfg(debug_assertions)]
struct DebugGovernedBufferSlot {
    pointer: AtomicUsize,
    initialized_len: AtomicUsize,
    capacity: AtomicUsize,
}

#[cfg(debug_assertions)]
impl DebugGovernedBufferSlot {
    const fn new() -> Self {
        Self {
            pointer: AtomicUsize::new(0),
            initialized_len: AtomicUsize::new(0),
            capacity: AtomicUsize::new(0),
        }
    }
}

#[cfg(debug_assertions)]
#[doc(hidden)]
pub struct DebugGovernedBufferMetadataSink {
    enabled: AtomicBool,
    slots: [DebugGovernedBufferSlot; 5],
}

#[cfg(debug_assertions)]
impl DebugGovernedBufferMetadataSink {
    pub const fn new() -> Self {
        Self {
            enabled: AtomicBool::new(false),
            slots: [
                DebugGovernedBufferSlot::new(),
                DebugGovernedBufferSlot::new(),
                DebugGovernedBufferSlot::new(),
                DebugGovernedBufferSlot::new(),
                DebugGovernedBufferSlot::new(),
            ],
        }
    }

    pub fn begin(&self) {
        for slot in &self.slots {
            slot.pointer.store(0, Ordering::SeqCst);
            slot.initialized_len.store(0, Ordering::SeqCst);
            slot.capacity.store(0, Ordering::SeqCst);
        }
        self.enabled.store(true, Ordering::SeqCst);
    }

    pub fn end(&self) {
        self.enabled.store(false, Ordering::SeqCst);
    }

    pub fn metadata(&self, kind: DebugGovernedBufferKind) -> DebugGovernedBufferMetadata {
        let slot = &self.slots[kind as usize];
        DebugGovernedBufferMetadata {
            pointer: slot.pointer.load(Ordering::SeqCst),
            initialized_len: slot.initialized_len.load(Ordering::SeqCst),
            capacity: slot.capacity.load(Ordering::SeqCst),
        }
    }

    pub fn retire_if_matches(&self, kind: DebugGovernedBufferKind, pointer: usize) {
        let slot = &self.slots[kind as usize];
        if slot
            .pointer
            .compare_exchange(pointer, 0, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
        {
            slot.initialized_len.store(0, Ordering::SeqCst);
            slot.capacity.store(0, Ordering::SeqCst);
        }
    }

    fn register(
        &self,
        kind: DebugGovernedBufferKind,
        pointer: *const u8,
        initialized_len: usize,
        capacity: usize,
    ) {
        if !self.enabled.load(Ordering::SeqCst) {
            return;
        }
        let slot = &self.slots[kind as usize];
        let address = pointer as usize;
        if slot.pointer.swap(address, Ordering::SeqCst) == address {
            slot.initialized_len
                .fetch_max(initialized_len, Ordering::SeqCst);
        } else {
            slot.initialized_len
                .store(initialized_len, Ordering::SeqCst);
        }
        slot.capacity.store(capacity, Ordering::SeqCst);
    }
}

#[cfg(debug_assertions)]
impl Default for DebugGovernedBufferMetadataSink {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(not(debug_assertions))]
struct DebugGovernedBufferMetadataSink;

#[cfg(not(debug_assertions))]
impl DebugGovernedBufferMetadataSink {
    const fn new() -> Self {
        Self
    }

    fn register(
        &self,
        _kind: DebugGovernedBufferKind,
        _pointer: *const u8,
        _initialized_len: usize,
        _capacity: usize,
    ) {
    }
}

static NOOP_GOVERNED_BUFFER_METADATA: DebugGovernedBufferMetadataSink =
    DebugGovernedBufferMetadataSink::new();

#[derive(Clone, Copy, Debug)]
struct ChatTransportBounds {
    nonstream_bytes: usize,
    sse_bytes: usize,
    sse_events: usize,
    sse_idle_timeout: Duration,
    sse_total_timeout: Duration,
}

impl ChatTransportBounds {
    const Q1: Self = Self {
        nonstream_bytes: MAX_RESPONSE_BYTES,
        sse_bytes: MAX_SSE_RESPONSE_BYTES,
        sse_events: MAX_SSE_EVENTS,
        sse_idle_timeout: SSE_IDLE_TIMEOUT,
        sse_total_timeout: SSE_TOTAL_TIMEOUT,
    };
}

trait ResponsePeerObserver: Send + Sync {
    fn validate(
        &self,
        accepted_peers: &[SocketAddr],
        actual_peer: Option<SocketAddr>,
    ) -> Result<(), ProviderError>;
}

struct PinnedResponsePeerObserver;

impl ResponsePeerObserver for PinnedResponsePeerObserver {
    fn validate(
        &self,
        accepted_peers: &[SocketAddr],
        actual_peer: Option<SocketAddr>,
    ) -> Result<(), ProviderError> {
        validate_response_peer(accepted_peers, actual_peer)
    }
}

pub struct OpenAiCompatibleClient {
    client: Client,
    base_url: String,
    egress: ProviderEgress,
    auth_header: Option<(HeaderName, HeaderValue)>,
    accepted_peers: Vec<SocketAddr>,
    chat_bounds: ChatTransportBounds,
    peer_observer: Arc<dyn ResponsePeerObserver>,
    buffer_metadata: &'static DebugGovernedBufferMetadataSink,
}

impl std::fmt::Debug for OpenAiCompatibleClient {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Explicit rather than absent, so that adding `#[derive(Debug)]` to a
        // struct holding this client cannot start printing the key.
        formatter
            .debug_struct("OpenAiCompatibleClient")
            .field("egress", &self.egress)
            .field("auth", &self.auth_header.as_ref().map(|_| "[REDACTED]"))
            .field("accepted_peer_count", &self.accepted_peers.len())
            .field("chat_bounds", &self.chat_bounds)
            .field("peer_observer", &"pinned")
            .finish()
    }
}

impl OpenAiCompatibleClient {
    /// Construct a client, or fail if no HTTP client can be built.
    ///
    /// Previously this fell back to `Client::default()` when the builder
    /// failed, which silently produced a client with **no timeout** — the one
    /// property the builder was there to set.
    pub fn new(
        base_url: impl Into<String>,
        api_key: Option<String>,
    ) -> Result<Self, ProviderError> {
        Self::with_timeout(base_url, api_key, DEFAULT_TIMEOUT)
    }

    pub fn with_timeout(
        base_url: impl Into<String>,
        api_key: Option<String>,
        timeout: Duration,
    ) -> Result<Self, ProviderError> {
        let auth = match api_key {
            Some(api_key) if api_key.is_empty() => {
                return Err(ProviderError::Unavailable(
                    "provider credential must not be empty".into(),
                ));
            }
            Some(api_key) => ConnectionAuth::Bearer(api_key.into()),
            None => ConnectionAuth::None,
        };
        let base_url = base_url.into();
        let normalized = Url::parse(base_url.trim_end_matches('/'))
            .map_err(|_| ProviderError::Unavailable("provider base URL is invalid".into()))?;
        let kind = match normalized.scheme() {
            "https" => ConnectionKind::OpenAiChatCompletionsV1,
            "http" => ConnectionKind::LMStudioLocal,
            _ => {
                return Err(ProviderError::Unavailable(
                    "provider base URL uses an unsupported scheme".into(),
                ));
            }
        };
        Self::for_connection_with_timeout(kind, base_url, auth, timeout)
    }

    pub fn for_connection(
        kind: ConnectionKind,
        base_url: impl Into<String>,
        auth: ConnectionAuth,
    ) -> Result<Self, ProviderError> {
        Self::for_connection_with_timeout(kind, base_url, auth, DEFAULT_TIMEOUT)
    }

    /// Builds the reusable transport used by a governed provider attempt.
    ///
    /// It deliberately has no credential argument.  A governed dispatch owns
    /// its `ConnectionAuth` and gives it to one of the `*_once` methods below,
    /// which consumes it while building that one outbound request.  Retaining
    /// a `HeaderValue` or credential on this client would let a later request
    /// accidentally reuse authority from an earlier effect.
    pub fn for_governed_connection(
        kind: ConnectionKind,
        base_url: impl Into<String>,
    ) -> Result<Self, ProviderError> {
        Self::for_connection_with_timeout(kind, base_url, ConnectionAuth::None, DEFAULT_TIMEOUT)
    }

    #[cfg(debug_assertions)]
    #[doc(hidden)]
    pub fn for_governed_connection_with_debug_buffer_metadata(
        kind: ConnectionKind,
        base_url: impl Into<String>,
        buffer_metadata: &'static DebugGovernedBufferMetadataSink,
    ) -> Result<Self, ProviderError> {
        let mut client = Self::for_governed_connection(kind, base_url)?;
        client.buffer_metadata = buffer_metadata;
        Ok(client)
    }

    #[cfg(debug_assertions)]
    #[doc(hidden)]
    pub fn for_connection_with_debug_buffer_metadata(
        kind: ConnectionKind,
        base_url: impl Into<String>,
        auth: ConnectionAuth,
        buffer_metadata: &'static DebugGovernedBufferMetadataSink,
    ) -> Result<Self, ProviderError> {
        let mut client = Self::for_connection(kind, base_url, auth)?;
        client.buffer_metadata = buffer_metadata;
        Ok(client)
    }

    #[cfg(debug_assertions)]
    #[doc(hidden)]
    pub fn for_connection_with_debug_buffer_metadata_and_timeouts(
        kind: ConnectionKind,
        base_url: impl Into<String>,
        auth: ConnectionAuth,
        timeouts: (Duration, Duration, Duration, Duration),
        buffer_metadata: &'static DebugGovernedBufferMetadataSink,
    ) -> Result<Self, ProviderError> {
        let (request_total_timeout, read_idle_timeout, sse_idle_timeout, sse_total_timeout) =
            timeouts;
        if sse_idle_timeout.is_zero() || sse_total_timeout.is_zero() {
            return Err(ProviderError::Unavailable(
                "provider stream timeouts must be greater than zero".into(),
            ));
        }
        let mut client = Self::for_connection_with_components(
            kind,
            base_url,
            auth,
            (request_total_timeout, read_idle_timeout),
            ChatTransportBounds::Q1,
            resolve_all_peers,
            Arc::new(PinnedResponsePeerObserver),
        )?;
        client.chat_bounds.sse_idle_timeout = sse_idle_timeout;
        client.chat_bounds.sse_total_timeout = sse_total_timeout;
        client.buffer_metadata = buffer_metadata;
        Ok(client)
    }

    fn for_connection_with_timeout(
        kind: ConnectionKind,
        base_url: impl Into<String>,
        auth: ConnectionAuth,
        timeout: Duration,
    ) -> Result<Self, ProviderError> {
        Self::for_connection_with_components(
            kind,
            base_url,
            auth,
            (timeout, DEFAULT_READ_TIMEOUT.min(timeout)),
            ChatTransportBounds::Q1,
            resolve_all_peers,
            Arc::new(PinnedResponsePeerObserver),
        )
    }

    fn for_connection_with_components<R>(
        kind: ConnectionKind,
        base_url: impl Into<String>,
        auth: ConnectionAuth,
        transport_timeouts: (Duration, Duration),
        chat_bounds: ChatTransportBounds,
        resolver: R,
        peer_observer: Arc<dyn ResponsePeerObserver>,
    ) -> Result<Self, ProviderError>
    where
        R: FnOnce(&Url, Duration) -> Result<Vec<SocketAddr>, ProviderError>,
    {
        let (timeout, read_timeout) = transport_timeouts;
        if timeout.is_zero() || read_timeout.is_zero() {
            return Err(ProviderError::Unavailable(
                "provider total timeout must be greater than zero".into(),
            ));
        }
        let base_url = base_url.into().trim_end_matches('/').to_string();
        let parsed = parse_base_url(kind, &base_url)?;
        let accepted_peers = resolver(&parsed, DEFAULT_CONNECT_TIMEOUT.min(timeout))?;
        let destination = classify_and_validate_peers(kind, &parsed, &accepted_peers)?;
        let auth_header = canonical_auth_header(auth, &NOOP_GOVERNED_BUFFER_METADATA)?;

        let redirects_disabled = true;
        let proxy_disabled = true;
        let mut builder = Client::builder()
            .timeout(timeout)
            .connect_timeout(DEFAULT_CONNECT_TIMEOUT.min(timeout))
            .read_timeout(read_timeout)
            .redirect(reqwest::redirect::Policy::none())
            .no_proxy()
            .tls_built_in_root_certs(true);
        if kind == ConnectionKind::OpenAiChatCompletionsV1 {
            builder = builder.https_only(true);
        }
        if parsed
            .host_str()
            .is_some_and(|host| host.parse::<IpAddr>().is_err())
        {
            builder = builder.resolve_to_addrs(
                parsed.host_str().expect("validated URL has a host"),
                &accepted_peers,
            );
        }
        let client = builder.build().map_err(|_| {
            ProviderError::Unavailable("provider HTTP client could not be built".into())
        })?;

        Ok(Self {
            client,
            base_url: base_url.clone(),
            egress: ProviderEgress::new(
                format!("{base_url}/chat/completions"),
                destination,
                redirects_disabled,
                proxy_disabled,
            ),
            auth_header,
            accepted_peers,
            chat_bounds,
            peer_observer,
            buffer_metadata: &NOOP_GOVERNED_BUFFER_METADATA,
        })
    }

    pub fn egress(&self) -> &ProviderEgress {
        &self.egress
    }

    pub async fn models_list(
        &self,
        _request: ModelsListRequest,
    ) -> Result<ModelsListResponse, ProviderError> {
        let request = self.request(Method::GET, "models")?;
        self.execute_json(request).await
    }

    pub async fn chat_completions(
        &self,
        request: ChatCompletionsRequest,
    ) -> Result<ChatCompletionsResponse, ProviderError> {
        let mode = request.mode();
        let http_request = self
            .request(Method::POST, "chat/completions")?
            .json(request.payload());
        self.execute_chat_request(http_request, mode).await
    }

    /// Executes one compatibility-shaped chat request with one owned governed
    /// credential.  Unlike [`Self::chat_completions`], this method does not
    /// read the legacy client-bound credential cache; the supplied auth is
    /// consumed while constructing this request and then dropped on every
    /// success, refusal, and transport-error path.
    pub async fn chat_completions_once(
        &self,
        auth: ConnectionAuth,
        request: ChatCompletionsRequest,
    ) -> Result<ChatCompletionsResponse, ProviderError> {
        let mode = request.mode();
        let http_request = self
            .request_with_consumed_auth(auth, Method::POST, "chat/completions")?
            .json(request.payload());
        self.execute_chat_request(http_request, mode).await
    }

    async fn execute_chat_request(
        &self,
        http_request: RequestBuilder,
        mode: ChatCompletionsMode,
    ) -> Result<ChatCompletionsResponse, ProviderError> {
        match mode {
            ChatCompletionsMode::Json => {
                let body = self
                    .execute_json_value(http_request, self.chat_bounds.nonstream_bytes)
                    .await?;
                let content = body["choices"][0]["message"]["content"]
                    .as_str()
                    .unwrap_or_default()
                    .to_owned();
                let usage = usage_from_json(&body);
                Ok(ChatCompletionsResponse {
                    content,
                    usage,
                    output: ChatCompletionsOutput::Json(body),
                })
            }
            ChatCompletionsMode::Sse => tokio::time::timeout(
                self.chat_bounds.sse_total_timeout,
                self.execute_sse(http_request),
            )
            .await
            .map_err(|_| ProviderError::Timeout)?,
        }
    }

    pub async fn embeddings(
        &self,
        request: EmbeddingsRequest,
    ) -> Result<EmbeddingsResponse, ProviderError> {
        let http_request = self.request(Method::POST, "embeddings")?.json(&request);
        self.execute_json(http_request).await
    }

    /// Consumes the one reconstructed governed request and renders its private
    /// wire form exactly once. It does not pass through the legacy compatibility
    /// request types or a generic request JSON value.
    pub async fn execute_effective(
        &self,
        request: EffectiveModelRequest,
    ) -> Result<EffectiveModelResponse, ProviderError> {
        match request {
            EffectiveModelRequest::ModelsList => {
                let http_request = self.request(Method::GET, "models")?;
                self.execute_json(http_request)
                    .await
                    .map(EffectiveModelResponse::ModelsList)
            }
            EffectiveModelRequest::ChatCompletions(request) => self
                .execute_effective_chat(request)
                .await
                .map(EffectiveModelResponse::ChatCompletions),
            EffectiveModelRequest::Embeddings(request) => self
                .execute_effective_embeddings(request)
                .await
                .map(EffectiveModelResponse::Embeddings),
        }
    }

    /// Executes one reconstructed governed request with the one owned
    /// credential returned by provider dispatch.  The credential is consumed
    /// while the request builder is created and never enters this reusable
    /// transport's state.
    pub async fn execute_effective_once(
        &self,
        auth: ConnectionAuth,
        request: EffectiveModelRequest,
    ) -> Result<EffectiveModelResponse, ProviderError> {
        match request {
            EffectiveModelRequest::ModelsList => {
                let http_request = self.request_with_consumed_auth(auth, Method::GET, "models")?;
                self.execute_json(http_request)
                    .await
                    .map(EffectiveModelResponse::ModelsList)
            }
            EffectiveModelRequest::ChatCompletions(request) => self
                .execute_effective_chat_with_request(
                    request,
                    self.request_with_consumed_auth(auth, Method::POST, "chat/completions")?,
                )
                .await
                .map(EffectiveModelResponse::ChatCompletions),
            EffectiveModelRequest::Embeddings(request) => self
                .execute_effective_embeddings_with_request(
                    request,
                    self.request_with_consumed_auth(auth, Method::POST, "embeddings")?,
                )
                .await
                .map(EffectiveModelResponse::Embeddings),
        }
    }

    /// Executes a closed q1 request through this same production adapter.
    /// Unlike the legacy compatibility API, this method never accepts a
    /// caller-supplied JSON body.
    pub async fn execute_q1(
        &self,
        request: Q1ProbeRequest,
    ) -> Result<Q1ProbeResponse, ProviderError> {
        self.execute_q1_for_qualification(request)
            .await
            .map_err(q1_failure_as_provider_error)
    }

    /// Qualification keeps the exact numeric HTTP refusal separate from a
    /// structural or oracle failure.  This is the entrypoint a q1 runner uses;
    /// the backwards-compatible method above deliberately retains its legacy
    /// `ProviderError` surface for pre-existing callers.
    pub async fn execute_q1_for_qualification(
        &self,
        request: Q1ProbeRequest,
    ) -> Result<Q1ProbeResponse, Q1ProbeFailure> {
        match request {
            Q1ProbeRequest::ModelsList => self
                .execute_q1_models()
                .await
                .map(Q1ProbeResponse::ModelsList),
            Q1ProbeRequest::Chat(request) => self
                .execute_q1_chat(&request)
                .await
                .map(Q1ProbeResponse::Chat),
            Q1ProbeRequest::Embeddings(request) => self
                .execute_q1_embeddings(&request)
                .await
                .map(Q1ProbeResponse::Embeddings),
        }
    }

    /// Executes one q1 probe with the credential leased for that exact shared
    /// dispatch.  The reusable transport receives no credential; this method
    /// consumes it while building the sole probe request.
    pub async fn execute_q1_for_qualification_once(
        &self,
        auth: ConnectionAuth,
        request: Q1ProbeRequest,
    ) -> Result<Q1ProbeResponse, Q1ProbeFailure> {
        match request {
            Q1ProbeRequest::ModelsList => self
                .execute_q1_models_from_request(
                    self.request_with_consumed_auth(auth, Method::GET, "models")
                        .map_err(q1_transport_failure)?,
                )
                .await
                .map(Q1ProbeResponse::ModelsList),
            Q1ProbeRequest::Chat(request) => self
                .execute_q1_chat_from_request(
                    &request,
                    self.request_with_consumed_auth(auth, Method::POST, "chat/completions")
                        .map_err(q1_transport_failure)?,
                )
                .await
                .map(Q1ProbeResponse::Chat),
            Q1ProbeRequest::Embeddings(request) => self
                .execute_q1_embeddings_from_request(
                    &request,
                    self.request_with_consumed_auth(auth, Method::POST, "embeddings")
                        .map_err(q1_transport_failure)?,
                )
                .await
                .map(Q1ProbeResponse::Embeddings),
        }
    }

    async fn execute_q1_models(&self) -> Result<Q1ModelsListProbeResult, Q1ProbeFailure> {
        self.execute_q1_models_from_request(
            self.request(Method::GET, "models")
                .map_err(q1_transport_failure)?,
        )
        .await
    }

    async fn execute_q1_models_from_request(
        &self,
        request: RequestBuilder,
    ) -> Result<Q1ModelsListProbeResult, Q1ProbeFailure> {
        let response = request
            .send()
            .await
            .map_err(map_request_error)
            .map_err(q1_transport_failure)?;
        self.peer_observer
            .validate(&self.accepted_peers, response.remote_addr())
            .map_err(q1_transport_failure)?;
        let status = response.status();
        let body = read_zeroizing_bounded_body(
            response,
            self.chat_bounds.nonstream_bytes,
            self.buffer_metadata,
            DebugGovernedBufferKind::JsonBody,
        )
        .await
        .map_err(q1_transport_failure)?;
        if !status.is_success() {
            return Err(Q1ProbeFailure::HttpStatus {
                status: status.as_u16(),
            });
        }
        parse_q1_models(body.as_slice()).map_err(|_| Q1ProbeFailure::StructuralFailure)
    }

    async fn execute_q1_chat(
        &self,
        request: &Q1ChatProbeRequest,
    ) -> Result<Q1ChatProbeResult, Q1ProbeFailure> {
        self.execute_q1_chat_from_request(
            request,
            self.request(Method::POST, "chat/completions")
                .map_err(q1_transport_failure)?,
        )
        .await
    }

    async fn execute_q1_chat_from_request(
        &self,
        request: &Q1ChatProbeRequest,
        http_request: RequestBuilder,
    ) -> Result<Q1ChatProbeResult, Q1ProbeFailure> {
        let messages = request
            .messages()
            .iter()
            .map(q1_wire_message)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| Q1ProbeFailure::StructuralFailure)?;
        let (tools, tool_choice) =
            q1_tools(request.tools(), request.tool_choice(), request.nonce());
        let response_format = (request.response_format()
            == Q1ResponseFormat::StrictNonceJsonSchema)
            .then(|| q1_strict_response_format(request.nonce()));
        let wire = Q1ChatWireRequest {
            model: request.model(),
            messages,
            max_tokens: request.max_tokens(),
            temperature: request.temperature(),
            stream: request.stream(),
            stream_options: request.stream_include_usage().then_some(Q1StreamOptions {
                include_usage: true,
            }),
            tools,
            tool_choice,
            parallel_tool_calls: request.parallel_tool_calls().then_some(true),
            response_format,
        };
        let http_request = http_request.json(&wire);
        if request.stream() {
            tokio::time::timeout(
                self.chat_bounds.sse_total_timeout,
                self.execute_q1_chat_sse(http_request, request),
            )
            .await
            .map_err(|_| Q1ProbeFailure::TransportFailure)?
        } else {
            self.execute_q1_chat_json(http_request, request).await
        }
    }

    async fn execute_q1_embeddings(
        &self,
        request: &Q1EmbeddingsProbeRequest,
    ) -> Result<Q1EmbeddingsProbeResult, Q1ProbeFailure> {
        self.execute_q1_embeddings_from_request(
            request,
            self.request(Method::POST, "embeddings")
                .map_err(q1_transport_failure)?,
        )
        .await
    }

    async fn execute_q1_embeddings_from_request(
        &self,
        request: &Q1EmbeddingsProbeRequest,
        http_request: RequestBuilder,
    ) -> Result<Q1EmbeddingsProbeResult, Q1ProbeFailure> {
        let wire = Q1EmbeddingsWireRequest {
            model: request.model(),
            input: request.inputs().collect(),
            encoding_format: "float",
        };
        let response = http_request
            .json(&wire)
            .send()
            .await
            .map_err(map_request_error)
            .map_err(q1_transport_failure)?;
        self.peer_observer
            .validate(&self.accepted_peers, response.remote_addr())
            .map_err(q1_transport_failure)?;
        let status = response.status();
        let body = read_zeroizing_bounded_body(
            response,
            self.chat_bounds.nonstream_bytes,
            self.buffer_metadata,
            DebugGovernedBufferKind::JsonBody,
        )
        .await
        .map_err(q1_transport_failure)?;
        if !status.is_success() {
            return Err(Q1ProbeFailure::HttpStatus {
                status: status.as_u16(),
            });
        }
        parse_q1_embeddings(body.as_slice(), request.model())
            .map_err(|_| Q1ProbeFailure::StructuralFailure)
    }

    async fn execute_q1_chat_json(
        &self,
        request: RequestBuilder,
        q1: &Q1ChatProbeRequest,
    ) -> Result<Q1ChatProbeResult, Q1ProbeFailure> {
        let response = request
            .send()
            .await
            .map_err(map_request_error)
            .map_err(q1_transport_failure)?;
        self.peer_observer
            .validate(&self.accepted_peers, response.remote_addr())
            .map_err(q1_transport_failure)?;
        let status = response.status();
        let body = read_zeroizing_bounded_body(
            response,
            self.chat_bounds.nonstream_bytes,
            self.buffer_metadata,
            DebugGovernedBufferKind::JsonBody,
        )
        .await
        .map_err(q1_transport_failure)?;
        if !status.is_success() {
            return Err(Q1ProbeFailure::HttpStatus {
                status: status.as_u16(),
            });
        }
        parse_q1_chat_json(body.as_slice(), q1, self.buffer_metadata)
            .map_err(|_| Q1ProbeFailure::OracleViolation)
    }

    async fn execute_q1_chat_sse(
        &self,
        request: RequestBuilder,
        q1: &Q1ChatProbeRequest,
    ) -> Result<Q1ChatProbeResult, Q1ProbeFailure> {
        let mut response = request
            .send()
            .await
            .map_err(map_request_error)
            .map_err(q1_transport_failure)?;
        self.peer_observer
            .validate(&self.accepted_peers, response.remote_addr())
            .map_err(q1_transport_failure)?;
        let status = response.status();
        if !status.is_success() {
            let _body = read_zeroizing_bounded_body(
                response,
                self.chat_bounds.nonstream_bytes,
                self.buffer_metadata,
                DebugGovernedBufferKind::SseBody,
            )
            .await
            .map_err(q1_transport_failure)?;
            return Err(Q1ProbeFailure::HttpStatus {
                status: status.as_u16(),
            });
        }
        let is_sse = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .is_some_and(|value| {
                value
                    .split(';')
                    .next()
                    .is_some_and(|media| media.trim().eq_ignore_ascii_case("text/event-stream"))
            });
        if !is_sse {
            return Err(Q1ProbeFailure::StructuralFailure);
        }
        let mut body = Zeroizing::new(Vec::with_capacity(self.chat_bounds.sse_bytes));
        self.buffer_metadata.register(
            DebugGovernedBufferKind::SseBody,
            body.as_ptr(),
            body.len(),
            body.capacity(),
        );
        loop {
            let chunk = tokio::time::timeout(self.chat_bounds.sse_idle_timeout, response.chunk())
                .await
                .map_err(|_| Q1ProbeFailure::TransportFailure)?
                .map_err(map_request_error)
                .map_err(q1_transport_failure)?;
            let Some(chunk) = chunk else {
                break;
            };
            if body.len().saturating_add(chunk.len()) > self.chat_bounds.sse_bytes {
                return Err(Q1ProbeFailure::StructuralFailure);
            }
            body.extend_from_slice(&chunk);
            self.buffer_metadata.register(
                DebugGovernedBufferKind::SseBody,
                body.as_ptr(),
                body.len(),
                body.capacity(),
            );
        }
        parse_q1_chat_sse(
            body.as_slice(),
            q1,
            self.chat_bounds.sse_events,
            self.buffer_metadata,
        )
        .map_err(|_| Q1ProbeFailure::OracleViolation)
    }

    async fn execute_effective_chat(
        &self,
        request: EffectiveChatCompletionsRequest,
    ) -> Result<EffectiveChatResult, ProviderError> {
        let http_request = self.request(Method::POST, "chat/completions")?;
        self.execute_effective_chat_with_request(request, http_request)
            .await
    }

    async fn execute_effective_chat_with_request(
        &self,
        request: EffectiveChatCompletionsRequest,
        http_request: RequestBuilder,
    ) -> Result<EffectiveChatResult, ProviderError> {
        let mode = if request.stream() {
            ChatCompletionsMode::Sse
        } else {
            ChatCompletionsMode::Json
        };
        let sampling = request.sampling();
        let limits = request.limits();
        let wire = EffectiveChatWireRequest {
            model: request.model(),
            messages: request
                .messages()
                .iter()
                .map(|message| EffectiveChatWireMessage {
                    role: message.role().as_str(),
                    content: message.content(),
                })
                .collect(),
            temperature: sampling.temperature(),
            top_p: sampling.top_p(),
            max_tokens: limits.max_output_tokens(),
            stream: request.stream(),
            tools: request
                .tools()
                .iter()
                .map(|tool| EffectiveChatWireTool {
                    kind: "function",
                    function: EffectiveChatWireFunction {
                        name: tool.name(),
                        description: tool.description(),
                        parameters: tool.parameters(),
                    },
                })
                .collect(),
        };
        let http_request = http_request.json(&wire);
        match mode {
            ChatCompletionsMode::Json => self.execute_governed_chat_json(http_request).await,
            ChatCompletionsMode::Sse => tokio::time::timeout(
                self.chat_bounds.sse_total_timeout,
                self.execute_governed_chat_sse(http_request),
            )
            .await
            .map_err(|_| ProviderError::Timeout)?,
        }
    }

    async fn execute_governed_chat_json(
        &self,
        request: RequestBuilder,
    ) -> Result<EffectiveChatResult, ProviderError> {
        let response = request.send().await.map_err(map_request_error)?;
        self.peer_observer
            .validate(&self.accepted_peers, response.remote_addr())?;
        let status = response.status();
        let correlation_id = allowlisted_correlation_id(&response);
        let body = read_zeroizing_bounded_body(
            response,
            self.chat_bounds.nonstream_bytes,
            self.buffer_metadata,
            DebugGovernedBufferKind::JsonBody,
        )
        .await?;
        if !status.is_success() {
            let safe = ProviderResponseError {
                status: status.as_u16(),
                code: None,
                error_type: None,
                correlation_id,
            };
            return Err(map_safe_status(status, &safe));
        }
        parse_governed_json(body.as_slice(), self.buffer_metadata)
    }

    async fn execute_governed_chat_sse(
        &self,
        request: RequestBuilder,
    ) -> Result<EffectiveChatResult, ProviderError> {
        let mut response = request.send().await.map_err(map_request_error)?;
        self.peer_observer
            .validate(&self.accepted_peers, response.remote_addr())?;
        let status = response.status();
        let correlation_id = allowlisted_correlation_id(&response);
        if !status.is_success() {
            let _body = read_zeroizing_bounded_body(
                response,
                self.chat_bounds.nonstream_bytes,
                self.buffer_metadata,
                DebugGovernedBufferKind::SseBody,
            )
            .await?;
            let safe = ProviderResponseError {
                status: status.as_u16(),
                code: None,
                error_type: None,
                correlation_id,
            };
            return Err(map_safe_status(status, &safe));
        }
        let is_sse = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .is_some_and(|value| {
                value
                    .split(';')
                    .next()
                    .is_some_and(|media| media.trim().eq_ignore_ascii_case("text/event-stream"))
            });
        if !is_sse {
            return Err(ProviderError::InvalidResponse(
                "streaming provider response is not text/event-stream".into(),
            ));
        }

        let mut body = Zeroizing::new(Vec::with_capacity(self.chat_bounds.sse_bytes));
        self.buffer_metadata.register(
            DebugGovernedBufferKind::SseBody,
            body.as_ptr(),
            body.len(),
            body.capacity(),
        );
        loop {
            let chunk = tokio::time::timeout(self.chat_bounds.sse_idle_timeout, response.chunk())
                .await
                .map_err(|_| ProviderError::Timeout)?
                .map_err(map_request_error)?;
            let Some(chunk) = chunk else {
                break;
            };
            if body.len().saturating_add(chunk.len()) > self.chat_bounds.sse_bytes {
                return Err(ProviderError::InvalidResponse(
                    "provider SSE response exceeded the configured byte limit".into(),
                ));
            }
            body.extend_from_slice(&chunk);
            self.buffer_metadata.register(
                DebugGovernedBufferKind::SseBody,
                body.as_ptr(),
                body.len(),
                body.capacity(),
            );
        }
        parse_governed_sse(
            body.as_slice(),
            self.chat_bounds.sse_events,
            self.buffer_metadata,
        )
    }

    async fn execute_effective_embeddings(
        &self,
        request: EffectiveEmbeddingsRequest,
    ) -> Result<EmbeddingsResponse, ProviderError> {
        let http_request = self.request(Method::POST, "embeddings")?;
        self.execute_effective_embeddings_with_request(request, http_request)
            .await
    }

    async fn execute_effective_embeddings_with_request(
        &self,
        request: EffectiveEmbeddingsRequest,
        http_request: RequestBuilder,
    ) -> Result<EmbeddingsResponse, ProviderError> {
        let wire = EffectiveEmbeddingsWireRequest {
            model: request.model(),
            input: request.inputs().collect(),
            encoding_format: "float",
        };
        let http_request = http_request.json(&wire);
        self.execute_json(http_request).await
    }

    pub fn safe_error_for_status(&self, status: u16) -> ProviderResponseError {
        ProviderResponseError {
            status,
            code: None,
            error_type: None,
            correlation_id: None,
        }
    }

    fn request(&self, method: Method, suffix: &str) -> Result<RequestBuilder, ProviderError> {
        let mut request = self.request_without_auth(method, suffix)?;
        if let Some((name, value)) = &self.auth_header {
            request = request.header(name, value);
        }
        Ok(request)
    }

    fn request_with_consumed_auth(
        &self,
        auth: ConnectionAuth,
        method: Method,
        suffix: &str,
    ) -> Result<RequestBuilder, ProviderError> {
        let mut request = self.request_without_auth(method, suffix)?;
        if let Some((name, value)) = canonical_auth_header(auth, self.buffer_metadata)? {
            request = request.header(name, value);
        }
        Ok(request)
    }

    fn request_without_auth(
        &self,
        method: Method,
        suffix: &str,
    ) -> Result<RequestBuilder, ProviderError> {
        let endpoint = format!("{}/{suffix}", self.base_url);
        Ok(self.client.request(method, endpoint))
    }

    async fn execute_json<T: DeserializeOwned>(
        &self,
        request: RequestBuilder,
    ) -> Result<T, ProviderError> {
        let body = self
            .execute_json_value(request, self.chat_bounds.nonstream_bytes)
            .await?;
        serde_json::from_value(body)
            .map_err(|_| ProviderError::InvalidResponse("provider returned invalid JSON".into()))
    }

    async fn execute_json_value(
        &self,
        request: RequestBuilder,
        byte_limit: usize,
    ) -> Result<serde_json::Value, ProviderError> {
        let response = request.send().await.map_err(map_request_error)?;
        self.peer_observer
            .validate(&self.accepted_peers, response.remote_addr())?;
        let status = response.status();
        let correlation_id = allowlisted_correlation_id(&response);
        let body = read_bounded_body(response, byte_limit).await?;
        if !status.is_success() {
            let safe = safe_response_error(status, correlation_id, &body);
            return Err(map_safe_status(status, &safe));
        }
        serde_json::from_slice(&body)
            .map_err(|_| ProviderError::InvalidResponse("provider returned invalid JSON".into()))
    }

    async fn execute_sse(
        &self,
        request: RequestBuilder,
    ) -> Result<ChatCompletionsResponse, ProviderError> {
        let mut response = request.send().await.map_err(map_request_error)?;
        self.peer_observer
            .validate(&self.accepted_peers, response.remote_addr())?;
        let status = response.status();
        let correlation_id = allowlisted_correlation_id(&response);
        if !status.is_success() {
            let body = read_bounded_body(response, self.chat_bounds.nonstream_bytes).await?;
            let safe = safe_response_error(status, correlation_id, &body);
            return Err(map_safe_status(status, &safe));
        }
        let is_sse = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .is_some_and(|value| {
                value
                    .split(';')
                    .next()
                    .is_some_and(|media| media.trim().eq_ignore_ascii_case("text/event-stream"))
            });
        if !is_sse {
            return Err(ProviderError::InvalidResponse(
                "streaming provider response is not text/event-stream".into(),
            ));
        }

        let mut body = Vec::new();
        loop {
            let chunk = tokio::time::timeout(self.chat_bounds.sse_idle_timeout, response.chunk())
                .await
                .map_err(|_| ProviderError::Timeout)?
                .map_err(map_request_error)?;
            let Some(chunk) = chunk else {
                break;
            };
            if body.len().saturating_add(chunk.len()) > self.chat_bounds.sse_bytes {
                return Err(ProviderError::InvalidResponse(
                    "provider SSE response exceeded the configured byte limit".into(),
                ));
            }
            body.extend_from_slice(&chunk);
        }
        let events = parse_sse_events(&body, self.chat_bounds.sse_events)?;
        let content = events
            .iter()
            .filter_map(|event| match event {
                ChatSseEvent::Json(value) => value["choices"][0]["delta"]["content"].as_str(),
                ChatSseEvent::Done => None,
            })
            .collect::<String>();
        let usage = events
            .iter()
            .filter_map(|event| match event {
                ChatSseEvent::Json(value) => Some(usage_from_json(value)),
                ChatSseEvent::Done => None,
            })
            .find(|usage| !matches!(usage, vestrace_application::ProviderUsage::Unknown))
            .unwrap_or(vestrace_application::ProviderUsage::Unknown);
        Ok(ChatCompletionsResponse {
            content,
            usage,
            output: ChatCompletionsOutput::Sse(events),
        })
    }

    #[cfg(test)]
    fn map_status(status: reqwest::StatusCode) -> ProviderError {
        // Distinguished because they call for different responses: a rate limit
        // is worth retrying, a 4xx is not.
        match status.as_u16() {
            429 => ProviderError::RateLimited,
            408 | 504 => ProviderError::Timeout,
            // Both, because providers disagree on which one a bad key gets:
            // NVIDIA answers 403, OpenAI 401. Neither is a malformed reply, and
            // reporting one as such told an operator with an expired key to go
            // and read the adapter.
            401 | 403 => ProviderError::CredentialRejected(format!("provider returned {status}")),
            500..=599 => ProviderError::Unavailable(format!("provider returned {status}")),
            // The body is not included: it echoes back request content, which
            // may be the very data a caller is careful about.
            _ => ProviderError::InvalidResponse(format!("provider returned {status}")),
        }
    }
}

fn parse_base_url(kind: ConnectionKind, base_url: &str) -> Result<Url, ProviderError> {
    let parsed = Url::parse(base_url)
        .map_err(|_| ProviderError::Unavailable("provider base URL is invalid".into()))?;
    if parsed.username() != ""
        || parsed.password().is_some()
        || parsed.query().is_some()
        || parsed.fragment().is_some()
        || parsed.host_str().is_none()
    {
        return Err(ProviderError::Unavailable(
            "provider base URL must not contain userinfo, query, or fragment".into(),
        ));
    }
    match (kind, parsed.scheme()) {
        (ConnectionKind::LMStudioLocal, "http" | "https")
        | (ConnectionKind::OpenAiChatCompletionsV1, "https") => Ok(parsed),
        (ConnectionKind::OpenAiChatCompletionsV1, _) => Err(ProviderError::Unavailable(
            "remote provider base URL must use HTTPS".into(),
        )),
        _ => Err(ProviderError::Unavailable(
            "provider base URL uses an unsupported scheme".into(),
        )),
    }
}

fn resolve_all_peers(
    endpoint: &Url,
    resolution_timeout: Duration,
) -> Result<Vec<SocketAddr>, ProviderError> {
    let host = endpoint
        .host_str()
        .ok_or_else(|| ProviderError::Unavailable("provider base URL has no host".into()))?;
    let host = host
        .strip_prefix('[')
        .and_then(|host| host.strip_suffix(']'))
        .unwrap_or(host);
    let port = endpoint.port_or_known_default().ok_or_else(|| {
        ProviderError::Unavailable("provider base URL has no effective port".into())
    })?;
    let mut peers: Vec<_> = match host.parse::<IpAddr>() {
        Ok(address) => vec![SocketAddr::new(address, port)],
        Err(_) => {
            let host = host.to_owned();
            let (sender, receiver) = std::sync::mpsc::sync_channel(1);
            std::thread::spawn(move || {
                let resolved = (host.as_str(), port)
                    .to_socket_addrs()
                    .map(|addresses| addresses.collect::<Vec<_>>());
                let _ = sender.send(resolved);
            });
            receiver
                .recv_timeout(resolution_timeout)
                .map_err(|_| {
                    ProviderError::Unavailable(
                        "provider host resolution exceeded its deadline".into(),
                    )
                })?
                .map_err(|_| {
                    ProviderError::Unavailable("provider host could not be resolved".into())
                })?
        }
    };
    peers.sort_unstable();
    peers.dedup();
    if peers.is_empty() {
        return Err(ProviderError::Unavailable(
            "provider host resolved to no peers".into(),
        ));
    }
    Ok(peers)
}

fn classify_and_validate_peers(
    kind: ConnectionKind,
    endpoint: &Url,
    peers: &[SocketAddr],
) -> Result<DataDestination, ProviderError> {
    match kind {
        ConnectionKind::OpenAiChatCompletionsV1 => {
            validate_remote_peers(peers)?;
            Ok(DataDestination::RemoteProvider)
        }
        ConnectionKind::LMStudioLocal => {
            let loopback_count = peers.iter().filter(|peer| peer.ip().is_loopback()).count();
            if loopback_count != 0 && loopback_count != peers.len() {
                return Err(ProviderError::Unavailable(
                    "LM Studio host resolved to mixed loopback and non-loopback peers".into(),
                ));
            }
            if exact_loopback_host(endpoint) {
                if loopback_count == peers.len() {
                    Ok(DataDestination::LocalModel)
                } else {
                    Err(ProviderError::Unavailable(
                        "loopback LM Studio host resolved to a non-loopback peer".into(),
                    ))
                }
            } else {
                Ok(DataDestination::RemoteProvider)
            }
        }
    }
}

/// Classify a pinned connection revision's base URL at policy time, without
/// resolving its host.
///
/// This is the static half of [`classify_and_validate_peers`]: it decides from
/// the connection kind and the literal host exactly as that function does
/// before it consults resolved peers, and shares [`exact_loopback_host`] with
/// it so the two cannot drift.
///
/// It is deliberately never more permissive than the dispatch-time check. The
/// only inputs on which they differ are ones where `classify_and_validate_peers`
/// refuses outright - a loopback host that resolves off-loopback - so a
/// `LocalModel` decided here can never let a remote disclosure through.
///
/// `None` means the pinned base URL is not a URL at all. The caller must refuse
/// to route rather than assume a disclosure class.
pub fn pinned_destination(kind: ConnectionKind, base_url: &str) -> Option<DataDestination> {
    let endpoint = Url::parse(base_url).ok()?;
    Some(match kind {
        ConnectionKind::OpenAiChatCompletionsV1 => DataDestination::RemoteProvider,
        ConnectionKind::LMStudioLocal if exact_loopback_host(&endpoint) => {
            DataDestination::LocalModel
        }
        ConnectionKind::LMStudioLocal => DataDestination::RemoteProvider,
    })
}

fn exact_loopback_host(endpoint: &Url) -> bool {
    endpoint.host_str().is_some_and(|host| {
        let host = host
            .strip_prefix('[')
            .and_then(|host| host.strip_suffix(']'))
            .unwrap_or(host);
        host.eq_ignore_ascii_case("localhost")
            || host.eq_ignore_ascii_case("localhost.")
            || host
                .parse::<IpAddr>()
                .is_ok_and(|address| address.is_loopback())
    })
}

fn validate_remote_peers(peers: &[SocketAddr]) -> Result<(), ProviderError> {
    if peers.iter().any(|peer| is_forbidden_remote_ip(peer.ip())) {
        return Err(ProviderError::Unavailable(
            "remote provider resolved to a forbidden network peer".into(),
        ));
    }
    Ok(())
}

fn is_forbidden_remote_ip(address: IpAddr) -> bool {
    match address {
        IpAddr::V4(address) => {
            address.is_loopback()
                || address.is_private()
                || address.is_link_local()
                || address.is_multicast()
                || address.is_unspecified()
                || address.is_broadcast()
                || address.octets() == [169, 254, 169, 254]
                || (address.octets()[0] == 100 && (64..=127).contains(&address.octets()[1]))
        }
        IpAddr::V6(address) => {
            let octets = address.octets();
            address.is_loopback()
                || address.is_unspecified()
                || address.is_multicast()
                || (octets[0] & 0xfe) == 0xfc
                || (octets[0] == 0xfe && (octets[1] & 0xc0) == 0x80)
                || address
                    .to_ipv4_mapped()
                    .is_some_and(|address| is_forbidden_remote_ip(IpAddr::V4(address)))
        }
    }
}

fn validate_response_peer(
    accepted_peers: &[SocketAddr],
    actual_peer: Option<SocketAddr>,
) -> Result<(), ProviderError> {
    if actual_peer.is_some_and(|peer| accepted_peers.contains(&peer)) {
        Ok(())
    } else {
        Err(ProviderError::Unavailable(
            "provider response peer did not match the pinned set".into(),
        ))
    }
}

fn canonical_auth_header(
    auth: ConnectionAuth,
    buffer_metadata: &'static DebugGovernedBufferMetadataSink,
) -> Result<Option<(HeaderName, HeaderValue)>, ProviderError> {
    let (name, value) = match auth {
        ConnectionAuth::None => return Ok(None),
        ConnectionAuth::Bearer(credential) => {
            if credential.expose().is_empty() {
                return Err(ProviderError::Unavailable(
                    "provider credential must not be empty".into(),
                ));
            }
            let mut value = Zeroizing::new(Vec::with_capacity(7 + credential.expose().len()));
            value.extend_from_slice(b"Bearer ");
            value.extend_from_slice(credential.expose().as_bytes());
            (reqwest::header::AUTHORIZATION, value)
        }
        ConnectionAuth::ApiKey(credential) => (
            HeaderName::from_static("api-key"),
            Zeroizing::new(credential.expose().as_bytes().to_vec()),
        ),
        ConnectionAuth::XApiKey(credential) => (
            HeaderName::from_static("x-api-key"),
            Zeroizing::new(credential.expose().as_bytes().to_vec()),
        ),
    };
    if value.is_empty() {
        return Err(ProviderError::Unavailable(
            "provider credential must not be empty".into(),
        ));
    }
    buffer_metadata.register(
        DebugGovernedBufferKind::AuthHeader,
        value.as_ptr(),
        value.len(),
        value.capacity(),
    );
    let mut value = HeaderValue::from_bytes(value.as_slice()).map_err(|_| {
        ProviderError::Unavailable("provider credential is not a valid header value".into())
    })?;
    value.set_sensitive(true);
    Ok(Some((name, value)))
}

fn map_request_error(error: reqwest::Error) -> ProviderError {
    if error.is_timeout() {
        ProviderError::Timeout
    } else {
        ProviderError::Unavailable("provider request failed".into())
    }
}

fn usage_from_json(body: &serde_json::Value) -> vestrace_application::ProviderUsage {
    let prompt_tokens = body["usage"]["prompt_tokens"]
        .as_u64()
        .and_then(|value| u32::try_from(value).ok());
    let completion_tokens = body["usage"]["completion_tokens"]
        .as_u64()
        .and_then(|value| u32::try_from(value).ok());
    match (prompt_tokens, completion_tokens) {
        (Some(prompt_tokens), Some(completion_tokens)) => {
            vestrace_application::ProviderUsage::Known {
                prompt_tokens,
                completion_tokens,
            }
        }
        _ => vestrace_application::ProviderUsage::Unknown,
    }
}

fn parse_sse_events(body: &[u8], event_limit: usize) -> Result<Vec<ChatSseEvent>, ProviderError> {
    let text = std::str::from_utf8(body)
        .map_err(|_| ProviderError::InvalidResponse("provider SSE was not UTF-8".into()))?;
    let normalized = text.replace("\r\n", "\n");
    let mut events = Vec::new();
    let mut saw_terminal_choice = false;
    let mut saw_done = false;

    for block in normalized.split("\n\n") {
        let data = block
            .lines()
            .filter_map(|line| line.strip_prefix("data:"))
            .map(str::trim_start)
            .collect::<Vec<_>>()
            .join("\n");
        if data.is_empty() {
            continue;
        }
        if saw_done {
            return Err(ProviderError::InvalidResponse(
                "provider SSE contained data after [DONE]".into(),
            ));
        }
        if events.len() >= event_limit {
            return Err(ProviderError::InvalidResponse(
                "provider SSE exceeded the configured event limit".into(),
            ));
        }
        if data == "[DONE]" {
            if !saw_terminal_choice {
                return Err(ProviderError::InvalidResponse(
                    "provider SSE emitted [DONE] before a terminal choice".into(),
                ));
            }
            events.push(ChatSseEvent::Done);
            saw_done = true;
            continue;
        }
        let value: serde_json::Value = serde_json::from_str(&data).map_err(|_| {
            ProviderError::InvalidResponse("provider SSE data was invalid JSON".into())
        })?;
        let choices = value["choices"].as_array();
        let usage_only = choices.is_some_and(Vec::is_empty)
            && value.get("usage").is_some_and(|usage| !usage.is_null());
        if saw_terminal_choice && !usage_only {
            return Err(ProviderError::InvalidResponse(
                "provider SSE contained data after the terminal choice".into(),
            ));
        }
        if choices.is_some_and(|choices| {
            choices
                .iter()
                .any(|choice| !choice["finish_reason"].is_null())
        }) {
            saw_terminal_choice = true;
        }
        events.push(ChatSseEvent::Json(value));
    }
    if !saw_terminal_choice || !saw_done {
        return Err(ProviderError::InvalidResponse(
            "provider SSE ended before terminal choice and [DONE]".into(),
        ));
    }
    Ok(events)
}

fn q1_wire_message(message: &Q1ChatMessage) -> Result<Q1ChatWireMessage<'_>, ProviderError> {
    match message {
        Q1ChatMessage::UserText(content) => Ok(Q1ChatWireMessage::Text {
            role: "user",
            content: content.as_str(),
        }),
        Q1ChatMessage::AssistantToolCall {
            call_id,
            function_name,
            argument_value,
        } => Ok(Q1ChatWireMessage::AssistantToolCall {
            role: "assistant",
            tool_calls: [Q1WireToolCall {
                id: call_id,
                kind: "function",
                function: Q1WireToolCallFunction {
                    name: function_name,
                    arguments: format!(r#"{{"value":"{}"}}"#, argument_value.as_str()),
                },
            }],
        }),
        Q1ChatMessage::ToolResult { call_id, content } => Ok(Q1ChatWireMessage::ToolResult {
            role: "tool",
            content: content.as_str(),
            tool_call_id: call_id,
        }),
        Q1ChatMessage::UserImage { text, data_url } => Ok(Q1ChatWireMessage::Image {
            role: "user",
            content: [
                Q1ImagePart::Text {
                    text: text.as_str(),
                },
                Q1ImagePart::ImageUrl {
                    image_url: Q1ImageUrl {
                        url: data_url.as_str(),
                    },
                },
            ],
        }),
    }
}

fn q1_tools(
    set: Q1ToolSet,
    choice: Q1ToolChoice,
    nonce: &str,
) -> (Vec<Q1WireTool>, Option<Q1WireToolChoice>) {
    let single_schema = serde_json::json!({
        "type":"object", "additionalProperties":false,
        "properties":{"value":{"const":nonce}}, "required":["value"]
    });
    match (set, choice) {
        (Q1ToolSet::None, Q1ToolChoice::None) => (Vec::new(), None),
        (Q1ToolSet::SingleProbe, Q1ToolChoice::NamedProbe) => (
            vec![Q1WireTool {
                kind: "function",
                function: Q1WireToolFunction {
                    name: "vestrace_probe",
                    parameters: single_schema,
                },
            }],
            Some(Q1WireToolChoice::Named {
                kind: "function",
                function: Q1NamedFunction {
                    name: "vestrace_probe",
                },
            }),
        ),
        (Q1ToolSet::ParallelProbes, Q1ToolChoice::Required) => (
            vec![
                Q1WireTool {
                    kind: "function",
                    function: Q1WireToolFunction {
                        name: "vestrace_probe_a",
                        parameters: serde_json::json!({
                            "type":"object", "additionalProperties":false,
                            "properties":{"value":{"const":format!("{nonce}-a")}}, "required":["value"]
                        }),
                    },
                },
                Q1WireTool {
                    kind: "function",
                    function: Q1WireToolFunction {
                        name: "vestrace_probe_b",
                        parameters: serde_json::json!({
                            "type":"object", "additionalProperties":false,
                            "properties":{"value":{"const":format!("{nonce}-b")}}, "required":["value"]
                        }),
                    },
                },
            ],
            Some(Q1WireToolChoice::Required("required")),
        ),
        _ => unreachable!("Q1ChatProbeRequest validates the frozen tool matrix"),
    }
}

fn q1_strict_response_format(nonce: &str) -> Q1WireResponseFormat {
    Q1WireResponseFormat {
        kind: "json_schema",
        json_schema: Q1StrictSchema {
            name: "vestrace_q1_structured_output",
            strict: true,
            schema: serde_json::json!({
                "type":"object", "additionalProperties":false,
                "properties":{"nonce":{"const":nonce},"ok":{"const":true}},
                "required":["nonce","ok"]
            }),
        },
    }
}

async fn read_bounded_body(
    mut response: Response,
    byte_limit: usize,
) -> Result<Vec<u8>, ProviderError> {
    if response
        .content_length()
        .is_some_and(|length| length > byte_limit as u64)
    {
        return Err(ProviderError::InvalidResponse(
            "provider response exceeded the configured byte limit".into(),
        ));
    }
    let mut body = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(map_request_error)? {
        if body.len().saturating_add(chunk.len()) > byte_limit {
            return Err(ProviderError::InvalidResponse(
                "provider response exceeded the configured byte limit".into(),
            ));
        }
        body.extend_from_slice(&chunk);
    }
    Ok(body)
}

async fn read_zeroizing_bounded_body(
    mut response: Response,
    byte_limit: usize,
    buffer_metadata: &'static DebugGovernedBufferMetadataSink,
    buffer_kind: DebugGovernedBufferKind,
) -> Result<Zeroizing<Vec<u8>>, ProviderError> {
    if response
        .content_length()
        .is_some_and(|length| length > byte_limit as u64)
    {
        return Err(ProviderError::InvalidResponse(
            "provider response exceeded the configured byte limit".into(),
        ));
    }
    let mut body = Zeroizing::new(Vec::with_capacity(byte_limit));
    buffer_metadata.register(buffer_kind, body.as_ptr(), body.len(), body.capacity());
    while let Some(chunk) = response.chunk().await.map_err(map_request_error)? {
        if body.len().saturating_add(chunk.len()) > byte_limit {
            return Err(ProviderError::InvalidResponse(
                "provider response exceeded the configured byte limit".into(),
            ));
        }
        body.extend_from_slice(&chunk);
        buffer_metadata.register(buffer_kind, body.as_ptr(), body.len(), body.capacity());
    }
    Ok(body)
}

fn invalid_governed_json() -> ProviderError {
    ProviderError::InvalidResponse("provider returned invalid governed JSON".into())
}

const Q1_MAX_MODEL_ID_BYTES: usize = 256;
const Q1_MAX_MODELS: usize = 4096;
const Q1_MAX_EMBEDDING_DIMENSIONS: usize = 65_536;
const Q1_MAX_TOOL_ARGUMENT_BYTES: usize = 8192;

struct Q1ToolCall {
    id: String,
    name: String,
    arguments: Zeroizing<String>,
}

struct Q1JsonChoice {
    content: Option<Zeroizing<String>>,
    finish_reason: EffectiveChatFinishReason,
    tool_calls: Vec<Q1ToolCall>,
}

fn q1_string(
    cursor: &mut GovernedJsonCursor<'_>,
    maximum: usize,
) -> Result<Zeroizing<String>, ProviderError> {
    let mut value = Zeroizing::new(String::new());
    cursor.string_into(&mut value, maximum)?;
    Ok(value)
}

fn q1_exact_model(
    cursor: &mut GovernedJsonCursor<'_>,
    expected: &str,
) -> Result<(), ProviderError> {
    let model = q1_string(cursor, Q1_MAX_MODEL_ID_BYTES)?;
    (model.as_str() == expected)
        .then_some(())
        .ok_or_else(invalid_governed_json)
}

fn parse_q1_models(body: &[u8]) -> Result<Q1ModelsListProbeResult, ProviderError> {
    let mut cursor = GovernedJsonCursor::new(body);
    let mut saw_data = false;
    let mut model_ids = std::collections::BTreeSet::new();
    parse_object(&mut cursor, |key, cursor| match key {
        b"data" if !saw_data => {
            saw_data = true;
            cursor.expect(b'[')?;
            if cursor.consume(b']') {
                return Err(invalid_governed_json());
            }
            loop {
                let mut id = None;
                parse_object(cursor, |field, cursor| match field {
                    b"id" if id.is_none() => {
                        id = Some(q1_string(cursor, Q1_MAX_MODEL_ID_BYTES)?);
                        Ok(())
                    }
                    _ => cursor.skip_value(0),
                })?;
                let id = id.ok_or_else(invalid_governed_json)?;
                if id.is_empty()
                    || id.len() > Q1_MAX_MODEL_ID_BYTES
                    || !model_ids.insert(id.to_string())
                    || model_ids.len() > Q1_MAX_MODELS
                {
                    return Err(invalid_governed_json());
                }
                if cursor.consume(b']') {
                    break;
                }
                cursor.expect(b',')?;
            }
            Ok(())
        }
        _ => cursor.skip_value(0),
    })?;
    cursor.finish()?;
    if !saw_data {
        return Err(invalid_governed_json());
    }
    Q1ModelsListProbeResult::new(model_ids.len())
}

fn parse_q1_tool_call(cursor: &mut GovernedJsonCursor<'_>) -> Result<Q1ToolCall, ProviderError> {
    let mut id = None;
    let mut call_type = false;
    let mut name = None;
    let mut arguments = None;
    parse_object(cursor, |key, cursor| match key {
        b"id" if id.is_none() => {
            id = Some(q1_string(cursor, 256)?.to_string());
            Ok(())
        }
        b"type" if !call_type => {
            let value = q1_string(cursor, 32)?;
            call_type = value.as_str() == "function";
            call_type.then_some(()).ok_or_else(invalid_governed_json)
        }
        b"function" if name.is_none() && arguments.is_none() => {
            parse_object(cursor, |field, cursor| match field {
                b"name" if name.is_none() => {
                    name = Some(q1_string(cursor, 128)?.to_string());
                    Ok(())
                }
                b"arguments" if arguments.is_none() => {
                    arguments = Some(q1_string(cursor, Q1_MAX_TOOL_ARGUMENT_BYTES)?);
                    Ok(())
                }
                _ => cursor.skip_value(0),
            })
        }
        _ => cursor.skip_value(0),
    })?;
    let id = id.ok_or_else(invalid_governed_json)?;
    if id.trim().is_empty() || !call_type {
        return Err(invalid_governed_json());
    }
    Ok(Q1ToolCall {
        id,
        name: name.ok_or_else(invalid_governed_json)?,
        arguments: arguments.ok_or_else(invalid_governed_json)?,
    })
}

fn parse_q1_message(
    cursor: &mut GovernedJsonCursor<'_>,
) -> Result<(Option<Zeroizing<String>>, Vec<Q1ToolCall>), ProviderError> {
    let mut role = None;
    let mut content = None;
    let mut tool_calls = None;
    parse_object(cursor, |key, cursor| match key {
        b"role" if role.is_none() => {
            role = Some(q1_string(cursor, 32)?);
            Ok(())
        }
        b"content" if content.is_none() => {
            cursor.skip_ws();
            if cursor.bytes.get(cursor.offset..cursor.offset + 4) == Some(b"null") {
                cursor.offset += 4;
                content = Some(Zeroizing::new(String::new()));
                Ok(())
            } else {
                content = Some(q1_string(cursor, MAX_EFFECTIVE_MATERIAL_BYTES)?);
                Ok(())
            }
        }
        b"tool_calls" if tool_calls.is_none() => {
            let mut calls = Vec::new();
            cursor.expect(b'[')?;
            if !cursor.consume(b']') {
                loop {
                    calls.push(parse_q1_tool_call(cursor)?);
                    if cursor.consume(b']') {
                        break;
                    }
                    cursor.expect(b',')?;
                }
            }
            tool_calls = Some(calls);
            Ok(())
        }
        _ => cursor.skip_value(0),
    })?;
    let role = role.ok_or_else(invalid_governed_json)?;
    if role.as_str() != "assistant" {
        return Err(invalid_governed_json());
    }
    Ok((content, tool_calls.unwrap_or_default()))
}

fn parse_q1_json_choice(
    cursor: &mut GovernedJsonCursor<'_>,
) -> Result<Q1JsonChoice, ProviderError> {
    let mut message = None;
    let mut finish_reason = None;
    parse_object(cursor, |key, cursor| match key {
        b"message" if message.is_none() => {
            message = Some(parse_q1_message(cursor)?);
            Ok(())
        }
        b"finish_reason" if finish_reason.is_none() => {
            finish_reason = Some(cursor.closed_string()?);
            Ok(())
        }
        _ => cursor.skip_value(0),
    })?;
    let (content, tool_calls) = message.ok_or_else(invalid_governed_json)?;
    Ok(Q1JsonChoice {
        content,
        finish_reason: finish_reason.ok_or_else(invalid_governed_json)?,
        tool_calls,
    })
}

fn parse_q1_chat_usage(
    cursor: &mut GovernedJsonCursor<'_>,
    require_total: bool,
) -> Result<ProviderUsage, ProviderError> {
    let mut prompt = None;
    let mut completion = None;
    let mut total = None;
    parse_object(cursor, |key, cursor| match key {
        b"prompt_tokens" if prompt.is_none() => {
            prompt = Some(cursor.u32()?);
            Ok(())
        }
        b"completion_tokens" if completion.is_none() => {
            completion = Some(cursor.u32()?);
            Ok(())
        }
        b"total_tokens" if total.is_none() => {
            total = Some(cursor.u32()?);
            Ok(())
        }
        _ => cursor.skip_value(0),
    })?;
    let prompt = prompt.ok_or_else(invalid_governed_json)?;
    let completion = completion.ok_or_else(invalid_governed_json)?;
    if (require_total || total.is_some()) && total != prompt.checked_add(completion) {
        return Err(invalid_governed_json());
    }
    Ok(ProviderUsage::Known {
        prompt_tokens: prompt,
        completion_tokens: completion,
    })
}

fn validate_q1_argument(arguments: &str, expected: &str) -> Result<(), ProviderError> {
    let mut cursor = GovernedJsonCursor::new(arguments.as_bytes());
    let mut value = None;
    parse_object(&mut cursor, |key, cursor| match key {
        b"value" if value.is_none() => {
            value = Some(q1_string(cursor, 64)?);
            Ok(())
        }
        _ => Err(invalid_governed_json()),
    })?;
    cursor.finish()?;
    (value
        .as_ref()
        .is_some_and(|value| value.as_str() == expected))
    .then_some(())
    .ok_or_else(invalid_governed_json)
}

fn validate_q1_structured_content(content: &str, nonce: &str) -> Result<(), ProviderError> {
    let mut cursor = GovernedJsonCursor::new(content.as_bytes());
    let mut saw_nonce = false;
    let mut saw_ok = false;
    parse_object(&mut cursor, |key, cursor| match key {
        b"nonce" if !saw_nonce => {
            saw_nonce = true;
            let value = q1_string(cursor, 64)?;
            (value.as_str() == nonce)
                .then_some(())
                .ok_or_else(invalid_governed_json)
        }
        b"ok" if !saw_ok => {
            saw_ok = true;
            cursor.literal(b"true")
        }
        _ => Err(invalid_governed_json()),
    })?;
    cursor.finish()?;
    (saw_nonce && saw_ok)
        .then_some(())
        .ok_or_else(invalid_governed_json)
}

fn validate_q1_json_choice(
    choice: Q1JsonChoice,
    request: &Q1ChatProbeRequest,
) -> Result<(), ProviderError> {
    match request.ordinal() {
        "40" => {
            if choice.finish_reason != EffectiveChatFinishReason::ToolCalls
                || choice.tool_calls.len() != 1
            {
                return Err(invalid_governed_json());
            }
            let call = &choice.tool_calls[0];
            if call.name != "vestrace_probe" {
                return Err(invalid_governed_json());
            }
            validate_q1_argument(call.arguments.as_str(), request.nonce())
        }
        "60" => {
            if choice.finish_reason != EffectiveChatFinishReason::ToolCalls
                || choice.tool_calls.len() != 2
                || choice.tool_calls[0].id == choice.tool_calls[1].id
            {
                return Err(invalid_governed_json());
            }
            for call in &choice.tool_calls {
                let expected = match call.name.as_str() {
                    "vestrace_probe_a" => format!("{}-a", request.nonce()),
                    "vestrace_probe_b" => format!("{}-b", request.nonce()),
                    _ => return Err(invalid_governed_json()),
                };
                validate_q1_argument(call.arguments.as_str(), &expected)?;
            }
            Ok(())
        }
        "20" | "50" | "70" | "80" => {
            if choice.finish_reason != EffectiveChatFinishReason::Stop
                || !choice.tool_calls.is_empty()
            {
                return Err(invalid_governed_json());
            }
            let content = choice.content.ok_or_else(invalid_governed_json)?;
            match request.ordinal() {
                "20" => (content.trim() == format!("VESTRACE_Q1_TEXT_{}", request.nonce()))
                    .then_some(())
                    .ok_or_else(invalid_governed_json),
                "50" => (content.trim() == format!("VESTRACE_Q1_TOOL_DONE_{}", request.nonce()))
                    .then_some(())
                    .ok_or_else(invalid_governed_json),
                "70" => validate_q1_structured_content(content.as_str(), request.nonce()),
                "80" => (content.trim() == "VESTRACE_Q1_IMAGE")
                    .then_some(())
                    .ok_or_else(invalid_governed_json),
                _ => unreachable!(),
            }
        }
        _ => Err(invalid_governed_json()),
    }
}

fn parse_q1_chat_json(
    body: &[u8],
    request: &Q1ChatProbeRequest,
    _buffer_metadata: &'static DebugGovernedBufferMetadataSink,
) -> Result<Q1ChatProbeResult, ProviderError> {
    let mut cursor = GovernedJsonCursor::new(body);
    let mut saw_model = false;
    let mut choice = None;
    let mut usage = None;
    parse_object(&mut cursor, |key, cursor| match key {
        b"model" if !saw_model => {
            saw_model = true;
            q1_exact_model(cursor, request.model())
        }
        b"choices" if choice.is_none() => {
            cursor.expect(b'[')?;
            if cursor.consume(b']') {
                return Err(invalid_governed_json());
            }
            let parsed = parse_q1_json_choice(cursor)?;
            if !cursor.consume(b']') {
                return Err(invalid_governed_json());
            }
            choice = Some(parsed);
            Ok(())
        }
        b"usage" if usage.is_none() => {
            usage = Some(parse_q1_chat_usage(cursor, false)?);
            Ok(())
        }
        _ => cursor.skip_value(0),
    })?;
    cursor.finish()?;
    validate_q1_json_choice(choice.ok_or_else(invalid_governed_json)?, request)?;
    Ok(Q1ChatProbeResult::new(
        usage.unwrap_or(ProviderUsage::Unknown),
    ))
}

fn parse_q1_embeddings(
    body: &[u8],
    expected_model: &str,
) -> Result<Q1EmbeddingsProbeResult, ProviderError> {
    let mut cursor = GovernedJsonCursor::new(body);
    let mut saw_model = false;
    let mut saw_data = false;
    let mut usage = None;
    parse_object(&mut cursor, |key, cursor| match key {
        b"model" if !saw_model => {
            saw_model = true;
            q1_exact_model(cursor, expected_model)
        }
        b"data" if !saw_data => {
            saw_data = true;
            let mut indices = std::collections::BTreeSet::new();
            let mut dimension = None;
            cursor.expect(b'[')?;
            if cursor.consume(b']') {
                return Err(invalid_governed_json());
            }
            loop {
                let mut index = None;
                let mut vector_dimension = None;
                parse_object(cursor, |field, cursor| match field {
                    b"index" if index.is_none() => {
                        index = Some(cursor.u32()?);
                        Ok(())
                    }
                    b"embedding" if vector_dimension.is_none() => {
                        cursor.expect(b'[')?;
                        let mut count = 0_usize;
                        if !cursor.consume(b']') {
                            loop {
                                cursor.finite_number()?;
                                count = count.saturating_add(1);
                                if count > Q1_MAX_EMBEDDING_DIMENSIONS {
                                    return Err(invalid_governed_json());
                                }
                                if cursor.consume(b']') {
                                    break;
                                }
                                cursor.expect(b',')?;
                            }
                        }
                        if count == 0 {
                            return Err(invalid_governed_json());
                        }
                        vector_dimension = Some(count);
                        Ok(())
                    }
                    _ => cursor.skip_value(0),
                })?;
                let index = index.ok_or_else(invalid_governed_json)?;
                let vector_dimension = vector_dimension.ok_or_else(invalid_governed_json)?;
                if !indices.insert(index)
                    || dimension
                        .replace(vector_dimension)
                        .is_some_and(|prior| prior != vector_dimension)
                {
                    return Err(invalid_governed_json());
                }
                if cursor.consume(b']') {
                    break;
                }
                cursor.expect(b',')?;
            }
            (indices.len() == 2 && indices.contains(&0) && indices.contains(&1))
                .then_some(())
                .ok_or_else(invalid_governed_json)
        }
        b"usage" if usage.is_none() => {
            let mut prompt = None;
            let mut total = None;
            parse_object(cursor, |field, cursor| match field {
                b"prompt_tokens" if prompt.is_none() => {
                    prompt = Some(cursor.u32()?);
                    Ok(())
                }
                b"total_tokens" if total.is_none() => {
                    total = Some(cursor.u32()?);
                    Ok(())
                }
                _ => cursor.skip_value(0),
            })?;
            usage = Some(Q1EmbeddingUsage::new(
                prompt.ok_or_else(invalid_governed_json)?,
                total.ok_or_else(invalid_governed_json)?,
            )?);
            Ok(())
        }
        _ => cursor.skip_value(0),
    })?;
    cursor.finish()?;
    if !saw_data {
        return Err(invalid_governed_json());
    }
    Ok(Q1EmbeddingsProbeResult::new(usage))
}

struct Q1SseFacts {
    content: Zeroizing<String>,
    saw_terminal: bool,
    usage: Option<ProviderUsage>,
}

fn parse_q1_sse_choice(
    cursor: &mut GovernedJsonCursor<'_>,
    facts: &mut Q1SseFacts,
) -> Result<(), ProviderError> {
    let mut saw_delta = false;
    let mut saw_finish = false;
    parse_object(cursor, |key, cursor| match key {
        b"delta" if !saw_delta => {
            saw_delta = true;
            parse_object(cursor, |field, cursor| match field {
                b"content" => {
                    if facts.saw_terminal {
                        return Err(invalid_governed_json());
                    }
                    cursor.string_into(&mut facts.content, MAX_EFFECTIVE_MATERIAL_BYTES)
                }
                _ => cursor.skip_value(0),
            })
        }
        b"finish_reason" if !saw_finish => {
            saw_finish = true;
            cursor.skip_ws();
            if cursor.bytes.get(cursor.offset..cursor.offset + 4) == Some(b"null") {
                cursor.offset += 4;
                Ok(())
            } else {
                let finish = cursor.closed_string()?;
                if facts.saw_terminal || finish != EffectiveChatFinishReason::Stop {
                    return Err(invalid_governed_json());
                }
                facts.saw_terminal = true;
                Ok(())
            }
        }
        _ => cursor.skip_value(0),
    })
}

fn parse_q1_sse_event(
    data: &[u8],
    request: &Q1ChatProbeRequest,
    facts: &mut Q1SseFacts,
) -> Result<(), ProviderError> {
    if facts.saw_terminal {
        return Err(invalid_governed_json());
    }
    let mut cursor = GovernedJsonCursor::new(data);
    let mut saw_model = false;
    let mut saw_choices = false;
    parse_object(&mut cursor, |key, cursor| match key {
        b"model" if !saw_model => {
            saw_model = true;
            q1_exact_model(cursor, request.model())
        }
        b"choices" if !saw_choices => {
            saw_choices = true;
            cursor.expect(b'[')?;
            if cursor.consume(b']') {
                return Ok(());
            }
            parse_q1_sse_choice(cursor, facts)?;
            if !cursor.consume(b']') {
                return Err(invalid_governed_json());
            }
            Ok(())
        }
        b"usage" if facts.usage.is_none() => {
            facts.usage = Some(parse_q1_chat_usage(cursor, request.ordinal() == "35")?);
            Ok(())
        }
        _ => cursor.skip_value(0),
    })?;
    cursor.finish()
}

fn parse_q1_chat_sse(
    body: &[u8],
    request: &Q1ChatProbeRequest,
    event_limit: usize,
    buffer_metadata: &'static DebugGovernedBufferMetadataSink,
) -> Result<Q1ChatProbeResult, ProviderError> {
    let text = std::str::from_utf8(body)
        .map_err(|_| ProviderError::InvalidResponse("provider SSE was not UTF-8".into()))?;
    let mut facts = Q1SseFacts {
        content: Zeroizing::new(String::with_capacity(
            body.len().min(MAX_EFFECTIVE_MATERIAL_BYTES),
        )),
        saw_terminal: false,
        usage: None,
    };
    buffer_metadata.register(
        DebugGovernedBufferKind::Content,
        facts.content.as_ptr(),
        facts.content.len(),
        facts.content.capacity(),
    );
    let mut event_data = Zeroizing::new(Vec::<u8>::with_capacity(body.len()));
    buffer_metadata.register(
        DebugGovernedBufferKind::ParserScratch,
        event_data.as_ptr(),
        event_data.len(),
        event_data.capacity(),
    );
    let mut event_count = 0_usize;
    let mut saw_done = false;
    let mut process_event = |data: &mut Zeroizing<Vec<u8>>| -> Result<(), ProviderError> {
        if data.is_empty() {
            return Ok(());
        }
        event_count = event_count.saturating_add(1);
        if event_count > event_limit {
            return Err(ProviderError::InvalidResponse(
                "provider SSE exceeded the configured event limit".into(),
            ));
        }
        if data.as_slice() == b"[DONE]" {
            if !facts.saw_terminal || (request.ordinal() == "35" && facts.usage.is_none()) {
                return Err(invalid_governed_json());
            }
            saw_done = true;
            data.zeroize();
            return Ok(());
        }
        if saw_done {
            return Err(invalid_governed_json());
        }
        let parsed = parse_q1_sse_event(data.as_slice(), request, &mut facts);
        buffer_metadata.register(
            DebugGovernedBufferKind::Content,
            facts.content.as_ptr(),
            facts.content.len(),
            facts.content.capacity(),
        );
        data.zeroize();
        parsed
    };
    for raw_line in text.split_inclusive('\n') {
        let line = raw_line.trim_end_matches(['\r', '\n']);
        if line.is_empty() {
            process_event(&mut event_data)?;
            continue;
        }
        if let Some(data) = line.strip_prefix("data:") {
            if !event_data.is_empty() {
                event_data.push(b'\n');
            }
            event_data.extend_from_slice(data.trim_start().as_bytes());
            buffer_metadata.register(
                DebugGovernedBufferKind::ParserScratch,
                event_data.as_ptr(),
                event_data.len(),
                event_data.capacity(),
            );
        }
    }
    process_event(&mut event_data)?;
    if !saw_done {
        return Err(invalid_governed_json());
    }
    let expected = format!("VESTRACE_Q1_TEXT_{}", request.nonce());
    if facts.content.trim() != expected {
        return Err(invalid_governed_json());
    }
    Ok(Q1ChatProbeResult::new(
        facts.usage.unwrap_or(ProviderUsage::Unknown),
    ))
}

struct GovernedJsonCursor<'a> {
    bytes: &'a [u8],
    offset: usize,
}

impl<'a> GovernedJsonCursor<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, offset: 0 }
    }

    fn skip_ws(&mut self) {
        while self
            .bytes
            .get(self.offset)
            .is_some_and(|byte| matches!(byte, b' ' | b'\n' | b'\r' | b'\t'))
        {
            self.offset += 1;
        }
    }

    fn consume(&mut self, expected: u8) -> bool {
        self.skip_ws();
        if self.bytes.get(self.offset) == Some(&expected) {
            self.offset += 1;
            true
        } else {
            false
        }
    }

    fn expect(&mut self, expected: u8) -> Result<(), ProviderError> {
        self.consume(expected)
            .then_some(())
            .ok_or_else(invalid_governed_json)
    }

    fn key(&mut self) -> Result<&'a [u8], ProviderError> {
        self.skip_ws();
        if self.bytes.get(self.offset) != Some(&b'"') {
            return Err(invalid_governed_json());
        }
        self.offset += 1;
        let start = self.offset;
        while let Some(byte) = self.bytes.get(self.offset).copied() {
            match byte {
                b'"' => {
                    let key = &self.bytes[start..self.offset];
                    self.offset += 1;
                    return Ok(key);
                }
                b'\\' | 0..=0x1f => return Err(invalid_governed_json()),
                _ => self.offset += 1,
            }
        }
        Err(invalid_governed_json())
    }

    fn closed_string(&mut self) -> Result<EffectiveChatFinishReason, ProviderError> {
        self.skip_ws();
        if self.bytes.get(self.offset) != Some(&b'"') {
            return Err(invalid_governed_json());
        }
        self.offset += 1;
        let start = self.offset;
        while let Some(byte) = self.bytes.get(self.offset).copied() {
            match byte {
                b'"' => {
                    let value = &self.bytes[start..self.offset];
                    self.offset += 1;
                    return match value {
                        b"stop" => Ok(EffectiveChatFinishReason::Stop),
                        b"length" => Ok(EffectiveChatFinishReason::Length),
                        b"tool_calls" => Ok(EffectiveChatFinishReason::ToolCalls),
                        b"content_filter" => Ok(EffectiveChatFinishReason::ContentFilter),
                        _ => Err(invalid_governed_json()),
                    };
                }
                b'\\' | 0..=0x1f | 0x80..=0xff => return Err(invalid_governed_json()),
                _ => self.offset += 1,
            }
        }
        Err(invalid_governed_json())
    }

    fn string_into(
        &mut self,
        output: &mut Zeroizing<String>,
        maximum: usize,
    ) -> Result<(), ProviderError> {
        self.skip_ws();
        if self.bytes.get(self.offset) != Some(&b'"') {
            return Err(invalid_governed_json());
        }
        self.offset += 1;
        loop {
            let byte = *self
                .bytes
                .get(self.offset)
                .ok_or_else(invalid_governed_json)?;
            match byte {
                b'"' => {
                    self.offset += 1;
                    return Ok(());
                }
                b'\\' => {
                    self.offset += 1;
                    let escaped = *self
                        .bytes
                        .get(self.offset)
                        .ok_or_else(invalid_governed_json)?;
                    self.offset += 1;
                    let character = match escaped {
                        b'"' => '"',
                        b'\\' => '\\',
                        b'/' => '/',
                        b'b' => '\u{0008}',
                        b'f' => '\u{000c}',
                        b'n' => '\n',
                        b'r' => '\r',
                        b't' => '\t',
                        b'u' => self.unicode_escape()?,
                        _ => return Err(invalid_governed_json()),
                    };
                    push_bounded(output, character, maximum)?;
                }
                0..=0x1f => return Err(invalid_governed_json()),
                0x20..=0x7f => {
                    self.offset += 1;
                    push_bounded(output, char::from(byte), maximum)?;
                }
                _ => {
                    let remainder = std::str::from_utf8(&self.bytes[self.offset..])
                        .map_err(|_| invalid_governed_json())?;
                    let character = remainder.chars().next().ok_or_else(invalid_governed_json)?;
                    self.offset += character.len_utf8();
                    push_bounded(output, character, maximum)?;
                }
            }
        }
    }

    fn unicode_escape(&mut self) -> Result<char, ProviderError> {
        let first = self.hex_quad()?;
        let scalar = if (0xd800..=0xdbff).contains(&first) {
            if self.bytes.get(self.offset..self.offset + 2) != Some(b"\\u") {
                return Err(invalid_governed_json());
            }
            self.offset += 2;
            let second = self.hex_quad()?;
            if !(0xdc00..=0xdfff).contains(&second) {
                return Err(invalid_governed_json());
            }
            0x10000 + (((first - 0xd800) as u32) << 10) + (second - 0xdc00) as u32
        } else if (0xdc00..=0xdfff).contains(&first) {
            return Err(invalid_governed_json());
        } else {
            first as u32
        };
        char::from_u32(scalar).ok_or_else(invalid_governed_json)
    }

    fn hex_quad(&mut self) -> Result<u16, ProviderError> {
        let bytes = self
            .bytes
            .get(self.offset..self.offset + 4)
            .ok_or_else(invalid_governed_json)?;
        self.offset += 4;
        bytes.iter().try_fold(0_u16, |value, byte| {
            let digit = match byte {
                b'0'..=b'9' => u16::from(*byte - b'0'),
                b'a'..=b'f' => u16::from(*byte - b'a' + 10),
                b'A'..=b'F' => u16::from(*byte - b'A' + 10),
                _ => return Err(invalid_governed_json()),
            };
            Ok((value << 4) | digit)
        })
    }

    fn u32(&mut self) -> Result<u32, ProviderError> {
        self.skip_ws();
        let start = self.offset;
        while self.bytes.get(self.offset).is_some_and(u8::is_ascii_digit) {
            self.offset += 1;
        }
        if self.offset == start {
            return Err(invalid_governed_json());
        }
        std::str::from_utf8(&self.bytes[start..self.offset])
            .ok()
            .and_then(|value| value.parse().ok())
            .ok_or_else(invalid_governed_json)
    }

    fn finite_number(&mut self) -> Result<(), ProviderError> {
        self.skip_ws();
        let start = self.offset;
        self.skip_number()?;
        std::str::from_utf8(&self.bytes[start..self.offset])
            .ok()
            .and_then(|value| value.parse::<f64>().ok())
            .filter(|value| value.is_finite())
            .map(|_| ())
            .ok_or_else(invalid_governed_json)
    }

    fn skip_value(&mut self, depth: usize) -> Result<(), ProviderError> {
        if depth > 64 {
            return Err(invalid_governed_json());
        }
        self.skip_ws();
        match self.bytes.get(self.offset).copied() {
            Some(b'"') => self.skip_string(),
            Some(b'{') => {
                self.offset += 1;
                if self.consume(b'}') {
                    return Ok(());
                }
                loop {
                    self.key()?;
                    self.expect(b':')?;
                    self.skip_value(depth + 1)?;
                    if self.consume(b'}') {
                        return Ok(());
                    }
                    self.expect(b',')?;
                }
            }
            Some(b'[') => {
                self.offset += 1;
                if self.consume(b']') {
                    return Ok(());
                }
                loop {
                    self.skip_value(depth + 1)?;
                    if self.consume(b']') {
                        return Ok(());
                    }
                    self.expect(b',')?;
                }
            }
            Some(b't') => self.literal(b"true"),
            Some(b'f') => self.literal(b"false"),
            Some(b'n') => self.literal(b"null"),
            Some(b'-' | b'0'..=b'9') => self.skip_number(),
            _ => Err(invalid_governed_json()),
        }
    }

    fn skip_string(&mut self) -> Result<(), ProviderError> {
        self.expect(b'"')?;
        loop {
            let byte = *self
                .bytes
                .get(self.offset)
                .ok_or_else(invalid_governed_json)?;
            self.offset += 1;
            match byte {
                b'"' => return Ok(()),
                b'\\' => {
                    let escaped = *self
                        .bytes
                        .get(self.offset)
                        .ok_or_else(invalid_governed_json)?;
                    self.offset += 1;
                    if escaped == b'u' {
                        self.hex_quad()?;
                    } else if !matches!(
                        escaped,
                        b'"' | b'\\' | b'/' | b'b' | b'f' | b'n' | b'r' | b't'
                    ) {
                        return Err(invalid_governed_json());
                    }
                }
                0..=0x1f => return Err(invalid_governed_json()),
                _ => {}
            }
        }
    }

    fn literal(&mut self, literal: &[u8]) -> Result<(), ProviderError> {
        if self.bytes.get(self.offset..self.offset + literal.len()) == Some(literal) {
            self.offset += literal.len();
            Ok(())
        } else {
            Err(invalid_governed_json())
        }
    }

    fn skip_number(&mut self) -> Result<(), ProviderError> {
        if self.bytes.get(self.offset) == Some(&b'-') {
            self.offset += 1;
        }
        match self.bytes.get(self.offset).copied() {
            Some(b'0') => self.offset += 1,
            Some(b'1'..=b'9') => {
                self.offset += 1;
                while self.bytes.get(self.offset).is_some_and(u8::is_ascii_digit) {
                    self.offset += 1;
                }
            }
            _ => return Err(invalid_governed_json()),
        }
        if self.bytes.get(self.offset) == Some(&b'.') {
            self.offset += 1;
            let fraction = self.offset;
            while self.bytes.get(self.offset).is_some_and(u8::is_ascii_digit) {
                self.offset += 1;
            }
            if self.offset == fraction {
                return Err(invalid_governed_json());
            }
        }
        if self
            .bytes
            .get(self.offset)
            .is_some_and(|byte| matches!(byte, b'e' | b'E'))
        {
            self.offset += 1;
            if self
                .bytes
                .get(self.offset)
                .is_some_and(|byte| matches!(byte, b'+' | b'-'))
            {
                self.offset += 1;
            }
            let exponent = self.offset;
            while self.bytes.get(self.offset).is_some_and(u8::is_ascii_digit) {
                self.offset += 1;
            }
            if self.offset == exponent {
                return Err(invalid_governed_json());
            }
        }
        Ok(())
    }

    fn finish(mut self) -> Result<(), ProviderError> {
        self.skip_ws();
        (self.offset == self.bytes.len())
            .then_some(())
            .ok_or_else(invalid_governed_json)
    }
}

fn push_bounded(
    output: &mut Zeroizing<String>,
    character: char,
    maximum: usize,
) -> Result<(), ProviderError> {
    if output.len().saturating_add(character.len_utf8()) > maximum {
        return Err(ProviderError::InvalidResponse(
            "provider chat content exceeded the fixed material bound".into(),
        ));
    }
    output.push(character);
    Ok(())
}

fn parse_object<F>(cursor: &mut GovernedJsonCursor<'_>, mut field: F) -> Result<(), ProviderError>
where
    F: FnMut(&[u8], &mut GovernedJsonCursor<'_>) -> Result<(), ProviderError>,
{
    cursor.expect(b'{')?;
    if cursor.consume(b'}') {
        return Ok(());
    }
    loop {
        let key = cursor.key()?;
        cursor.expect(b':')?;
        field(key, cursor)?;
        if cursor.consume(b'}') {
            return Ok(());
        }
        cursor.expect(b',')?;
    }
}

fn parse_usage(cursor: &mut GovernedJsonCursor<'_>) -> Result<ProviderUsage, ProviderError> {
    let mut prompt = None;
    let mut completion = None;
    parse_object(cursor, |key, cursor| match key {
        b"prompt_tokens" if prompt.is_none() => {
            prompt = Some(cursor.u32()?);
            Ok(())
        }
        b"completion_tokens" if completion.is_none() => {
            completion = Some(cursor.u32()?);
            Ok(())
        }
        _ => cursor.skip_value(0),
    })?;
    Ok(ProviderUsage::Known {
        prompt_tokens: prompt.ok_or_else(invalid_governed_json)?,
        completion_tokens: completion.ok_or_else(invalid_governed_json)?,
    })
}

fn parse_message(
    cursor: &mut GovernedJsonCursor<'_>,
    content: &mut Zeroizing<String>,
) -> Result<(), ProviderError> {
    let mut saw_content = false;
    parse_object(cursor, |key, cursor| match key {
        b"content" if !saw_content => {
            saw_content = true;
            cursor.string_into(content, MAX_EFFECTIVE_MATERIAL_BYTES)
        }
        _ => cursor.skip_value(0),
    })?;
    saw_content.then_some(()).ok_or_else(invalid_governed_json)
}

fn parse_json_choice(
    cursor: &mut GovernedJsonCursor<'_>,
    content: &mut Zeroizing<String>,
) -> Result<EffectiveChatFinishReason, ProviderError> {
    let mut saw_message = false;
    let mut finish = None;
    parse_object(cursor, |key, cursor| match key {
        b"message" if !saw_message => {
            saw_message = true;
            parse_message(cursor, content)
        }
        b"finish_reason" if finish.is_none() => {
            finish = Some(cursor.closed_string()?);
            Ok(())
        }
        _ => cursor.skip_value(0),
    })?;
    if !saw_message {
        return Err(invalid_governed_json());
    }
    finish.ok_or_else(invalid_governed_json)
}

fn parse_governed_json(
    body: &[u8],
    buffer_metadata: &'static DebugGovernedBufferMetadataSink,
) -> Result<EffectiveChatResult, ProviderError> {
    let mut cursor = GovernedJsonCursor::new(body);
    let mut content = Zeroizing::new(String::with_capacity(
        body.len().min(MAX_EFFECTIVE_MATERIAL_BYTES),
    ));
    buffer_metadata.register(
        DebugGovernedBufferKind::Content,
        content.as_ptr(),
        content.len(),
        content.capacity(),
    );
    let mut choice_count = 0_usize;
    let mut finish_reason = None;
    let mut usage = ProviderUsage::Unknown;
    let parsed = parse_object(&mut cursor, |key, cursor| match key {
        b"choices" if choice_count == 0 => {
            cursor.expect(b'[')?;
            if cursor.consume(b']') {
                return Err(invalid_governed_json());
            }
            loop {
                choice_count = choice_count.saturating_add(1);
                let parsed_finish = parse_json_choice(cursor, &mut content)?;
                if finish_reason.replace(parsed_finish).is_some() {
                    return Err(invalid_governed_json());
                }
                if cursor.consume(b']') {
                    break;
                }
                cursor.expect(b',')?;
            }
            Ok(())
        }
        b"usage" => {
            usage = parse_usage(cursor)?;
            Ok(())
        }
        _ => cursor.skip_value(0),
    })
    .and_then(|()| cursor.finish());
    buffer_metadata.register(
        DebugGovernedBufferKind::Content,
        content.as_ptr(),
        content.len(),
        content.capacity(),
    );
    parsed?;
    if choice_count != 1 {
        return Err(ProviderError::InvalidResponse(
            "provider returned an invalid governed chat choice set".into(),
        ));
    }
    EffectiveChatResult::new(
        content,
        EffectiveChatEvidence::Completed(finish_reason.ok_or_else(invalid_governed_json)?),
        usage,
    )
}

fn parse_sse_delta(
    cursor: &mut GovernedJsonCursor<'_>,
    content: &mut Zeroizing<String>,
) -> Result<(), ProviderError> {
    let mut saw_content = false;
    parse_object(cursor, |key, cursor| match key {
        b"content" if !saw_content => {
            saw_content = true;
            cursor.string_into(content, MAX_EFFECTIVE_MATERIAL_BYTES)
        }
        _ => cursor.skip_value(0),
    })
}

fn parse_sse_choice(
    cursor: &mut GovernedJsonCursor<'_>,
    content: &mut Zeroizing<String>,
    finish_reason: &mut Option<EffectiveChatFinishReason>,
) -> Result<(), ProviderError> {
    parse_object(cursor, |key, cursor| match key {
        b"delta" => parse_sse_delta(cursor, content),
        b"finish_reason" => {
            cursor.skip_ws();
            if cursor.bytes.get(cursor.offset..cursor.offset + 4) == Some(b"null") {
                cursor.offset += 4;
                Ok(())
            } else {
                let reason = cursor.closed_string()?;
                if finish_reason.replace(reason).is_some() {
                    return Err(ProviderError::InvalidResponse(
                        "provider SSE contained multiple terminal choices".into(),
                    ));
                }
                Ok(())
            }
        }
        _ => cursor.skip_value(0),
    })
}

fn parse_sse_event(
    data: &[u8],
    content: &mut Zeroizing<String>,
    finish_reason: &mut Option<EffectiveChatFinishReason>,
    usage: &mut ProviderUsage,
) -> Result<(), ProviderError> {
    let mut cursor = GovernedJsonCursor::new(data);
    parse_object(&mut cursor, |key, cursor| match key {
        b"choices" => {
            cursor.expect(b'[')?;
            if cursor.consume(b']') {
                return Ok(());
            }
            loop {
                parse_sse_choice(cursor, content, finish_reason)?;
                if cursor.consume(b']') {
                    return Ok(());
                }
                cursor.expect(b',')?;
            }
        }
        b"usage" => {
            *usage = parse_usage(cursor)?;
            Ok(())
        }
        _ => cursor.skip_value(0),
    })?;
    cursor.finish()
}

fn parse_governed_sse(
    body: &[u8],
    event_limit: usize,
    buffer_metadata: &'static DebugGovernedBufferMetadataSink,
) -> Result<EffectiveChatResult, ProviderError> {
    let text = std::str::from_utf8(body)
        .map_err(|_| ProviderError::InvalidResponse("provider SSE was not UTF-8".into()))?;
    let capacity = body.len().min(MAX_EFFECTIVE_MATERIAL_BYTES);
    let mut content = Zeroizing::new(String::with_capacity(capacity));
    let mut event_data = Zeroizing::new(Vec::<u8>::with_capacity(body.len()));
    buffer_metadata.register(
        DebugGovernedBufferKind::Content,
        content.as_ptr(),
        content.len(),
        content.capacity(),
    );
    buffer_metadata.register(
        DebugGovernedBufferKind::ParserScratch,
        event_data.as_ptr(),
        event_data.len(),
        event_data.capacity(),
    );
    let mut event_count = 0_usize;
    let mut finish_reason = None;
    let mut usage = ProviderUsage::Unknown;
    let mut saw_done = false;

    let mut process_event = |data: &mut Zeroizing<Vec<u8>>| -> Result<(), ProviderError> {
        if data.is_empty() {
            return Ok(());
        }
        event_count = event_count.saturating_add(1);
        if event_count > event_limit {
            return Err(ProviderError::InvalidResponse(
                "provider SSE exceeded the configured event limit".into(),
            ));
        }
        if data.as_slice() == b"[DONE]" {
            if finish_reason.is_none() {
                return Err(ProviderError::InvalidResponse(
                    "provider SSE emitted [DONE] before a terminal choice".into(),
                ));
            }
            saw_done = true;
            data.zeroize();
            return Ok(());
        }
        if saw_done {
            return Err(ProviderError::InvalidResponse(
                "provider SSE contained data after [DONE]".into(),
            ));
        }
        let parsed = parse_sse_event(
            data.as_slice(),
            &mut content,
            &mut finish_reason,
            &mut usage,
        );
        buffer_metadata.register(
            DebugGovernedBufferKind::Content,
            content.as_ptr(),
            content.len(),
            content.capacity(),
        );
        data.zeroize();
        parsed
    };

    for raw_line in text.split_inclusive('\n') {
        let line = raw_line.trim_end_matches(['\r', '\n']);
        if line.is_empty() {
            process_event(&mut event_data)?;
            continue;
        }
        if let Some(data) = line.strip_prefix("data:") {
            if !event_data.is_empty() {
                event_data.push(b'\n');
            }
            event_data.extend_from_slice(data.trim_start().as_bytes());
            buffer_metadata.register(
                DebugGovernedBufferKind::ParserScratch,
                event_data.as_ptr(),
                event_data.len(),
                event_data.capacity(),
            );
        }
    }
    process_event(&mut event_data)?;
    if !saw_done {
        return Err(ProviderError::InvalidResponse(
            "provider SSE ended before terminal choice and [DONE]".into(),
        ));
    }
    EffectiveChatResult::new(
        content,
        EffectiveChatEvidence::Completed(finish_reason.expect("checked before [DONE]")),
        usage,
    )
}

fn allowlisted_correlation_id(response: &Response) -> Option<String> {
    ["x-request-id", "request-id", "openai-request-id"]
        .into_iter()
        .find_map(|name| response.headers().get(name))
        .and_then(|value| value.to_str().ok())
        .filter(|value| value.chars().count() <= MAX_CORRELATION_ID_CHARS)
        .map(ToOwned::to_owned)
}

fn safe_response_error(
    status: StatusCode,
    correlation_id: Option<String>,
    body: &[u8],
) -> ProviderResponseError {
    let parsed = serde_json::from_slice::<serde_json::Value>(body).ok();
    let field = |name: &str| {
        parsed
            .as_ref()
            .and_then(|body| body.get("error"))
            .and_then(|error| error.get(name))
            .and_then(serde_json::Value::as_str)
            .map(|value| value.chars().take(MAX_ERROR_FIELD_CHARS).collect())
    };
    ProviderResponseError {
        status: status.as_u16(),
        code: field("code"),
        error_type: field("type"),
        correlation_id,
    }
}

fn map_safe_status(status: StatusCode, safe: &ProviderResponseError) -> ProviderError {
    let message = format!("provider returned {status}; {safe}");
    match status.as_u16() {
        429 => ProviderError::RateLimited,
        408 | 504 => ProviderError::Timeout,
        401 | 403 => ProviderError::CredentialRejected(message),
        500..=599 => ProviderError::Unavailable(message),
        _ => ProviderError::InvalidResponse(message),
    }
}

fn q1_transport_failure(_error: ProviderError) -> Q1ProbeFailure {
    Q1ProbeFailure::TransportFailure
}

fn q1_failure_as_provider_error(failure: Q1ProbeFailure) -> ProviderError {
    match failure {
        Q1ProbeFailure::HttpStatus { status } => ProviderError::InvalidResponse(format!(
            "q1 provider returned safe HTTP status {status}"
        )),
        Q1ProbeFailure::OracleViolation => {
            ProviderError::InvalidResponse("q1 provider response violated its closed oracle".into())
        }
        Q1ProbeFailure::StructuralFailure => {
            ProviderError::InvalidResponse("q1 provider response structure was invalid".into())
        }
        Q1ProbeFailure::TransportFailure => {
            ProviderError::Unavailable("q1 provider transport failed".into())
        }
    }
}

/// Read a token count that the accounting path depends on.
///
/// Absent usage is an error rather than zero. A silent zero would understate
/// consumption in a system that enforces budgets, so the budget would be
/// spent without ever appearing spent.
#[cfg(test)]
fn required_token_count(body: &serde_json::Value, field: &str) -> Result<u32, ProviderError> {
    body["usage"][field]
        .as_u64()
        .and_then(|value| u32::try_from(value).ok())
        .ok_or_else(|| {
            ProviderError::InvalidResponse(format!("provider response has no usage.{field}"))
        })
}

#[async_trait]
impl TextGenerationProvider for OpenAiCompatibleClient {
    async fn generate(
        &self,
        request: GenerationRequest,
    ) -> Result<GenerationResponse, ProviderError> {
        let model = request.model.clone();
        let mut payload = serde_json::json!({
            "model": request.model,
            "messages": [{"role": "user", "content": request.prompt}],
        });
        if let Some(max_tokens) = request.max_tokens {
            payload["max_tokens"] = max_tokens.into();
        }
        let response = self
            .chat_completions(ChatCompletionsRequest::from_json_object(payload)?)
            .await?;
        let content = match &response.output {
            ChatCompletionsOutput::Json(body) => body["choices"][0]["message"]["content"]
                .as_str()
                .ok_or_else(|| {
                    ProviderError::InvalidResponse(
                        "provider response has no choices[0].message.content".into(),
                    )
                })?
                .to_owned(),
            ChatCompletionsOutput::Sse(_) => {
                return Err(ProviderError::InvalidResponse(
                    "legacy generation unexpectedly received an SSE response".into(),
                ));
            }
        };
        let (prompt_tokens, completion_tokens) = match response.usage {
            vestrace_application::ProviderUsage::Known {
                prompt_tokens,
                completion_tokens,
            } => (prompt_tokens, completion_tokens),
            vestrace_application::ProviderUsage::Unknown => {
                return Err(ProviderError::InvalidResponse(
                    "provider response has no usage".into(),
                ));
            }
        };

        Ok(GenerationResponse {
            content,
            model,
            prompt_tokens,
            completion_tokens,
        })
    }
}

/// The one production [`GovernedModelAdapter`].
///
/// It holds no destination, no credential and no configuration: every call
/// builds a transport from the kind and base URL that provider dispatch
/// selected from the Run's pinned Connection revision, and consumes the one
/// owned `ConnectionAuth` that the same transaction produced. There is nothing
/// here for a process setting to influence, which is the point — the legacy
/// executor it replaces read both its route and its key from `config.model`.
#[derive(Default)]
pub struct OpenAiGovernedModelAdapter;

#[async_trait::async_trait]
impl GovernedModelAdapter for OpenAiGovernedModelAdapter {
    async fn execute(
        &self,
        kind: ConnectionKind,
        runtime_base_url: &str,
        auth: ConnectionAuth,
        request: EffectiveModelRequest,
    ) -> Result<EffectiveModelResponse, ProviderError> {
        OpenAiCompatibleClient::for_governed_connection(kind, runtime_base_url)?
            .execute_effective_once(auth, request)
            .await
    }
}

#[cfg(test)]
mod tests {
    use std::io::{Read, Write};
    use std::sync::Arc;
    use std::thread::JoinHandle;

    use super::*;
    use vestrace_domain::DataDestination;

    #[test]
    fn mixed_public_and_forbidden_remote_addresses_are_refused() {
        let addresses = [
            "203.0.113.7:443".parse().unwrap(),
            "127.0.0.1:443".parse().unwrap(),
        ];

        assert!(validate_remote_peers(&addresses).is_err());
    }

    #[test]
    fn constructor_refuses_mixed_resolver_results_before_building_the_client() {
        let result = OpenAiCompatibleClient::for_connection_with_components(
            ConnectionKind::OpenAiChatCompletionsV1,
            "https://provider.test/v1",
            ConnectionAuth::None,
            (Duration::from_secs(1), Duration::from_secs(1)),
            ChatTransportBounds::Q1,
            |_endpoint, _timeout| {
                Ok(vec![
                    "203.0.113.7:443".parse().unwrap(),
                    "127.0.0.1:443".parse().unwrap(),
                ])
            },
            Arc::new(PinnedResponsePeerObserver),
        );

        assert!(matches!(result, Err(ProviderError::Unavailable(_))));
    }

    struct MismatchedResponsePeerObserver;

    impl ResponsePeerObserver for MismatchedResponsePeerObserver {
        fn validate(
            &self,
            accepted_peers: &[SocketAddr],
            _actual_peer: Option<SocketAddr>,
        ) -> Result<(), ProviderError> {
            validate_response_peer(accepted_peers, Some("127.0.0.2:1".parse().unwrap()))
        }
    }

    #[tokio::test]
    async fn request_path_refuses_a_response_peer_outside_the_pinned_set() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            let mut request = [0_u8; 1024];
            let _ = socket.read(&mut request).unwrap();
            socket
                .write_all(
                    b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 11\r\nConnection: close\r\n\r\n{\"data\":[]}",
                )
                .unwrap();
        });
        let client = OpenAiCompatibleClient::for_connection_with_components(
            ConnectionKind::LMStudioLocal,
            format!("http://provider.test:{}/v1", address.port()),
            ConnectionAuth::None,
            (Duration::from_secs(1), Duration::from_secs(1)),
            ChatTransportBounds::Q1,
            move |_endpoint, _timeout| Ok(vec![address]),
            Arc::new(MismatchedResponsePeerObserver),
        )
        .unwrap();

        let error = client
            .models_list(ModelsListRequest::new())
            .await
            .unwrap_err();
        server.join().unwrap();
        assert!(error.to_string().contains("pinned set"));
    }

    fn streaming_request() -> ChatCompletionsRequest {
        ChatCompletionsRequest::from_json_object(serde_json::json!({
            "model": "test-model",
            "messages": [],
            "stream": true
        }))
        .unwrap()
    }

    fn streaming_client(
        address: SocketAddr,
        bounds: ChatTransportBounds,
    ) -> OpenAiCompatibleClient {
        OpenAiCompatibleClient::for_connection_with_components(
            ConnectionKind::LMStudioLocal,
            format!("http://{address}/v1"),
            ConnectionAuth::None,
            (Duration::from_secs(2), Duration::from_secs(2)),
            bounds,
            move |_endpoint, _timeout| Ok(vec![address]),
            Arc::new(PinnedResponsePeerObserver),
        )
        .unwrap()
    }

    fn spawn_static_sse(body: String) -> (SocketAddr, JoinHandle<()>) {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            let mut request = [0_u8; 4096];
            let _ = socket.read(&mut request).unwrap();
            let headers = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            socket.write_all(headers.as_bytes()).unwrap();
            socket.write_all(body.as_bytes()).unwrap();
        });
        (address, server)
    }

    fn write_chunk(socket: &mut std::net::TcpStream, chunk: &str) -> std::io::Result<()> {
        write!(socket, "{:X}\r\n{chunk}\r\n", chunk.len())?;
        socket.flush()
    }

    #[tokio::test]
    async fn streaming_response_is_refused_at_the_byte_bound() {
        let body = format!(
            "data: {{\"choices\":[{{\"delta\":{{\"content\":\"{}\"}},\"finish_reason\":null}}]}}\n\ndata: {{\"choices\":[{{\"delta\":{{}},\"finish_reason\":\"stop\"}}]}}\n\ndata: [DONE]\n\n",
            "x".repeat(256)
        );
        let (address, server) = spawn_static_sse(body);
        let mut bounds = ChatTransportBounds::Q1;
        bounds.sse_bytes = 128;
        let client = streaming_client(address, bounds);

        let error = client
            .chat_completions(streaming_request())
            .await
            .unwrap_err();
        server.join().unwrap();

        assert!(error.to_string().contains("byte limit"));
    }

    #[tokio::test]
    async fn streaming_response_is_refused_at_the_event_bound() {
        let body = concat!(
            "data: {\"choices\":[{\"delta\":{\"content\":\"one\"},\"finish_reason\":null}]}\n\n",
            "data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}]}\n\n",
            "data: [DONE]\n\n"
        )
        .to_owned();
        let (address, server) = spawn_static_sse(body);
        let mut bounds = ChatTransportBounds::Q1;
        bounds.sse_events = 2;
        let client = streaming_client(address, bounds);

        let error = client
            .chat_completions(streaming_request())
            .await
            .unwrap_err();
        server.join().unwrap();

        assert!(error.to_string().contains("event limit"));
    }

    #[tokio::test]
    async fn streaming_response_is_refused_at_the_idle_bound() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            let mut request = [0_u8; 4096];
            let _ = socket.read(&mut request).unwrap();
            socket
                .write_all(
                    b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n",
                )
                .unwrap();
            socket.flush().unwrap();
            std::thread::sleep(Duration::from_millis(120));
            let _ = write_chunk(
                &mut socket,
                "data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}]}\n\ndata: [DONE]\n\n",
            );
        });
        let mut bounds = ChatTransportBounds::Q1;
        bounds.sse_idle_timeout = Duration::from_millis(40);
        bounds.sse_total_timeout = Duration::from_millis(500);
        let client = streaming_client(address, bounds);

        let error = client
            .chat_completions(streaming_request())
            .await
            .unwrap_err();
        server.join().unwrap();

        assert!(matches!(error, ProviderError::Timeout));
    }

    #[tokio::test]
    async fn streaming_response_is_refused_at_the_total_bound() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            let mut request = [0_u8; 4096];
            let _ = socket.read(&mut request).unwrap();
            socket
                .write_all(
                    b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n",
                )
                .unwrap();
            socket.flush().unwrap();
            for _ in 0..8 {
                std::thread::sleep(Duration::from_millis(30));
                if write_chunk(&mut socket, ": keepalive\n\n").is_err() {
                    break;
                }
            }
        });
        let mut bounds = ChatTransportBounds::Q1;
        bounds.sse_idle_timeout = Duration::from_millis(80);
        bounds.sse_total_timeout = Duration::from_millis(110);
        let client = streaming_client(address, bounds);

        let error = client
            .chat_completions(streaming_request())
            .await
            .unwrap_err();
        server.join().unwrap();

        assert!(matches!(error, ProviderError::Timeout));
    }

    #[tokio::test]
    async fn streaming_response_refuses_data_after_the_terminal_marker() {
        let body = concat!(
            "data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}]}\n\n",
            "data: {\"choices\":[{\"delta\":{\"content\":\"late\"},\"finish_reason\":null}]}\n\n",
            "data: [DONE]\n\n"
        )
        .to_owned();
        let (address, server) = spawn_static_sse(body);
        let client = streaming_client(address, ChatTransportBounds::Q1);

        let error = client
            .chat_completions(streaming_request())
            .await
            .unwrap_err();
        server.join().unwrap();

        assert!(error.to_string().contains("after the terminal choice"));
    }

    #[test]
    fn a_response_peer_must_belong_to_the_pinned_set() {
        let accepted = ["203.0.113.7:443".parse().unwrap()];
        let different = "203.0.113.8:443".parse().unwrap();

        assert!(validate_response_peer(&accepted, Some(different)).is_err());
    }

    #[test]
    fn a_hostname_that_only_looks_like_loopback_is_not_a_local_model() {
        let endpoint = Url::parse("http://127.example:12345/v1").unwrap();
        let peers = ["127.0.0.1:12345".parse().unwrap()];

        assert_eq!(
            classify_and_validate_peers(ConnectionKind::LMStudioLocal, &endpoint, &peers).unwrap(),
            DataDestination::RemoteProvider
        );
    }

    /// The pinned-revision classifier is the policy-time half of
    /// `classify_and_validate_peers`. It must never be more permissive than
    /// the dispatch-time check, or a disclosure could be authorized against a
    /// destination the adapter would later treat as remote.
    #[test]
    fn a_pinned_loopback_lm_studio_revision_is_a_local_model() {
        assert_eq!(
            pinned_destination(ConnectionKind::LMStudioLocal, "http://127.0.0.1:12345/v1"),
            Some(DataDestination::LocalModel)
        );
        assert_eq!(
            pinned_destination(ConnectionKind::LMStudioLocal, "http://localhost:12345/v1"),
            Some(DataDestination::LocalModel)
        );
    }

    #[test]
    fn a_pinned_non_loopback_lm_studio_revision_is_a_remote_provider() {
        assert_eq!(
            pinned_destination(ConnectionKind::LMStudioLocal, "http://10.0.0.4:12345/v1"),
            Some(DataDestination::RemoteProvider)
        );
        assert_eq!(
            pinned_destination(ConnectionKind::LMStudioLocal, "http://127.example:12345/v1"),
            Some(DataDestination::RemoteProvider)
        );
    }

    #[test]
    fn a_pinned_openai_revision_is_always_a_remote_provider() {
        // Even pointed at loopback: the kind alone settles it, exactly as the
        // dispatch-time classifier does.
        assert_eq!(
            pinned_destination(
                ConnectionKind::OpenAiChatCompletionsV1,
                "http://127.0.0.1:12345/v1"
            ),
            Some(DataDestination::RemoteProvider)
        );
        assert_eq!(
            pinned_destination(
                ConnectionKind::OpenAiChatCompletionsV1,
                "https://api.provider.test/v1"
            ),
            Some(DataDestination::RemoteProvider)
        );
    }

    #[test]
    fn an_unparseable_pinned_base_url_has_no_destination() {
        // Fail closed: the caller must refuse routing rather than guess a class.
        assert_eq!(
            pinned_destination(ConnectionKind::LMStudioLocal, "not-a-url"),
            None
        );
    }

    #[test]
    fn the_pinned_classifier_never_claims_local_where_the_dispatch_check_would_not() {
        // Every case the pinned classifier calls LocalModel must be one the
        // dispatch-time check also calls LocalModel when its peers are loopback.
        for base in [
            "http://127.0.0.1:12345/v1",
            "http://localhost:12345/v1",
            "http://[::1]:12345/v1",
        ] {
            if pinned_destination(ConnectionKind::LMStudioLocal, base)
                != Some(DataDestination::LocalModel)
            {
                continue;
            }
            let endpoint = Url::parse(base).unwrap();
            let peers = ["127.0.0.1:12345".parse().unwrap()];
            assert_eq!(
                classify_and_validate_peers(ConnectionKind::LMStudioLocal, &endpoint, &peers)
                    .unwrap(),
                DataDestination::LocalModel,
                "pinned classification of {base} disagrees with the dispatch check"
            );
        }
    }

    #[test]
    fn the_api_key_is_never_rendered() {
        let client =
            OpenAiCompatibleClient::new("http://127.0.0.1:12345/v1", Some("sk-secret".into()))
                .unwrap();
        let rendered = format!("{client:?}");
        assert!(!rendered.contains("sk-secret"));
        assert!(rendered.contains("[REDACTED]"));
    }

    #[test]
    fn a_trailing_slash_does_not_produce_a_doubled_path() {
        let client = OpenAiCompatibleClient::new("http://127.0.0.1:12345/v1/", None).unwrap();
        assert_eq!(
            client.egress.endpoint(),
            "http://127.0.0.1:12345/v1/chat/completions"
        );
    }

    #[test]
    fn missing_usage_is_an_error_rather_than_zero() {
        // Zero here would understate consumption in a system that enforces
        // budgets, so the budget would be spent without appearing spent.
        let body = serde_json::json!({ "choices": [] });
        assert!(matches!(
            required_token_count(&body, "prompt_tokens"),
            Err(ProviderError::InvalidResponse(_))
        ));
    }

    #[test]
    fn present_usage_is_read() {
        let body = serde_json::json!({ "usage": { "prompt_tokens": 12 } });
        assert_eq!(required_token_count(&body, "prompt_tokens").unwrap(), 12);
    }

    #[test]
    fn rate_limiting_is_distinguished_from_other_failures() {
        assert!(matches!(
            OpenAiCompatibleClient::map_status(reqwest::StatusCode::TOO_MANY_REQUESTS),
            ProviderError::RateLimited
        ));
        assert!(matches!(
            OpenAiCompatibleClient::map_status(reqwest::StatusCode::INTERNAL_SERVER_ERROR),
            ProviderError::Unavailable(_)
        ));
        assert!(matches!(
            OpenAiCompatibleClient::map_status(reqwest::StatusCode::BAD_REQUEST),
            ProviderError::InvalidResponse(_)
        ));
    }

    #[test]
    fn a_refused_credential_is_distinguished_from_a_malformed_reply() {
        // Providers disagree on the status: NVIDIA answers 403 to a bad key,
        // OpenAI 401. Both mean the same thing to the operator.
        for status in [
            reqwest::StatusCode::UNAUTHORIZED,
            reqwest::StatusCode::FORBIDDEN,
        ] {
            assert!(
                matches!(
                    OpenAiCompatibleClient::map_status(status),
                    ProviderError::CredentialRejected(_)
                ),
                "{status} should be reported as a rejected credential"
            );
        }
    }

    #[test]
    fn a_rejected_credential_does_not_render_the_key() {
        // The status line is safe to repeat; a provider's 403 body is not
        // guaranteed to be, and neither is anything derived from the key.
        let rendered =
            OpenAiCompatibleClient::map_status(reqwest::StatusCode::FORBIDDEN).to_string();
        assert_eq!(
            rendered,
            "Credential rejected: provider returned 403 Forbidden"
        );
    }

    #[test]
    fn a_provider_error_does_not_carry_the_response_body() {
        // The body echoes request content back, which may be the data a caller
        // is careful about.
        let rendered =
            OpenAiCompatibleClient::map_status(reqwest::StatusCode::BAD_REQUEST).to_string();
        assert_eq!(
            rendered,
            "Invalid response: provider returned 400 Bad Request"
        );
    }

    #[test]
    fn loopback_is_local_only_with_redirects_and_proxies_disabled() {
        let client = OpenAiCompatibleClient::new("http://[::1]:12345/v1", None).unwrap();
        let egress = client.egress();

        assert_eq!(egress.endpoint(), "http://[::1]:12345/v1/chat/completions");
        assert_eq!(egress.destination(), DataDestination::LocalModel);
        assert!(egress.redirects_disabled());
        assert!(egress.proxy_disabled());
    }

    #[test]
    fn private_network_hosts_are_remote_providers() {
        for endpoint in [
            "http://10.0.0.4:12345/v1",
            "http://172.16.0.4:12345/v1",
            "http://192.168.0.4:12345/v1",
        ] {
            let client = OpenAiCompatibleClient::new(endpoint, None).unwrap();
            assert_eq!(
                client.egress().destination(),
                DataDestination::RemoteProvider,
                "{endpoint} was mistaken for a local model"
            );
        }
    }

    #[test]
    fn localhost_requires_every_resolved_address_to_be_loopback() {
        let mixed_addresses = [
            "127.0.0.1:12345".parse().unwrap(),
            "203.0.113.7:12345".parse().unwrap(),
        ];

        assert!(
            classify_and_validate_peers(
                ConnectionKind::LMStudioLocal,
                &Url::parse("http://localhost:12345/v1").unwrap(),
                &mixed_addresses,
            )
            .is_err()
        );
    }

    #[test]
    fn loopback_without_either_transport_guard_is_remote() {
        let client = OpenAiCompatibleClient::new("http://127.0.0.1:12345/v1", None).unwrap();
        assert!(client.egress().redirects_disabled());
        assert!(client.egress().proxy_disabled());
    }

    #[tokio::test]
    async fn a_completion_redirect_to_a_remote_host_is_refused() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let received = std::thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            let mut request = [0_u8; 8192];
            let count = socket.read(&mut request).unwrap();
            let request = String::from_utf8_lossy(&request[..count]).into_owned();
            socket
                .write_all(
                    b"HTTP/1.1 302 Found\r\nLocation: http://192.0.2.1/collect\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                )
                .unwrap();
            request
        });

        let client = OpenAiCompatibleClient::with_timeout(
            format!("http://{address}/v1"),
            Some("credential-that-must-not-be-forwarded".into()),
            Duration::from_secs(2),
        )
        .unwrap();
        let result = client
            .generate(GenerationRequest {
                model: "test-model".into(),
                prompt: "prompt-that-must-not-leave-loopback".into(),
                max_tokens: Some(1),
            })
            .await;

        assert!(
            matches!(result, Err(ProviderError::InvalidResponse(ref message)) if message.contains("302 Found")),
            "redirect was not returned to the caller: {result:?}"
        );
        let request = received.join().unwrap();
        assert!(request.contains("prompt-that-must-not-leave-loopback"));
    }
}
