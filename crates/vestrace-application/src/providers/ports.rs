use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use vestrace_domain::DataDestination;
use zeroize::Zeroizing;

pub const MAX_EFFECTIVE_MATERIAL_BYTES: usize = 1024 * 1024;
pub const MAX_AGGREGATE_EFFECTIVE_REQUEST_BYTES: usize = 4 * 1024 * 1024;
pub const MAX_EFFECTIVE_REQUEST_ITEMS: usize = 4096;

pub use vestrace_domain::ConnectionKind;

pub struct ProviderCredential(Zeroizing<String>);

impl ProviderCredential {
    pub fn new(value: impl Into<String>) -> Self {
        Self(Zeroizing::new(value.into()))
    }

    pub fn expose(&self) -> &str {
        self.0.as_str()
    }
}

impl From<String> for ProviderCredential {
    fn from(value: String) -> Self {
        Self::new(value)
    }
}

impl std::fmt::Debug for ProviderCredential {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("ProviderCredential([REDACTED])")
    }
}

pub enum ConnectionAuth {
    None,
    Bearer(ProviderCredential),
    ApiKey(ProviderCredential),
    XApiKey(ProviderCredential),
}

impl std::fmt::Debug for ConnectionAuth {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::None => "ConnectionAuth::None",
            Self::Bearer(_) => "ConnectionAuth::Bearer([REDACTED])",
            Self::ApiKey(_) => "ConnectionAuth::ApiKey([REDACTED])",
            Self::XApiKey(_) => "ConnectionAuth::XApiKey([REDACTED])",
        })
    }
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct ModelsListRequest {}

impl ModelsListRequest {
    pub const fn new() -> Self {
        Self {}
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct ProviderModel {
    pub id: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct ModelsListResponse {
    pub data: Vec<ProviderModel>,
}

#[derive(Clone, Debug, Serialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ChatCompletionsMode {
    Json,
    Sse,
}

#[derive(Clone, Debug)]
pub struct ChatCompletionsRequest {
    payload: serde_json::Value,
    mode: ChatCompletionsMode,
}

impl ChatCompletionsRequest {
    pub fn single_user_message(model: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            payload: serde_json::json!({
                "model": model.into(),
                "messages": [{"role": "user", "content": content.into()}],
            }),
            mode: ChatCompletionsMode::Json,
        }
    }

    pub fn from_json_object(payload: serde_json::Value) -> Result<Self, ProviderError> {
        if !payload.is_object() {
            return Err(ProviderError::InvalidResponse(
                "chat completions request must be a JSON object".into(),
            ));
        }
        let mode = match payload.get("stream") {
            None | Some(serde_json::Value::Bool(false)) => ChatCompletionsMode::Json,
            Some(serde_json::Value::Bool(true)) => ChatCompletionsMode::Sse,
            Some(_) => {
                return Err(ProviderError::InvalidResponse(
                    "chat completions stream must be a boolean".into(),
                ));
            }
        };
        Ok(Self { payload, mode })
    }

    pub fn payload(&self) -> &serde_json::Value {
        &self.payload
    }

    pub const fn mode(&self) -> ChatCompletionsMode {
        self.mode
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProviderUsage {
    Known {
        prompt_tokens: u32,
        completion_tokens: u32,
    },
    Unknown,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EffectiveChatFinishReason {
    Stop,
    Length,
    ToolCalls,
    ContentFilter,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EffectiveChatEvidence {
    Completed(EffectiveChatFinishReason),
}

/// A governed chat result contains only the retained bounded content and
/// closed structural facts. It deliberately implements neither `Clone` nor
/// `Serialize`, so ownership crosses the result-material boundary once.
///
/// ```compile_fail
/// let result: vestrace_application::EffectiveChatResult = todo!();
/// let _copy = result.clone();
/// ```
/// ```compile_fail
/// let result: vestrace_application::EffectiveChatResult = todo!();
/// let _json = serde_json::to_vec(&result).unwrap();
/// ```
pub struct EffectiveChatResult {
    content: Zeroizing<String>,
    evidence: EffectiveChatEvidence,
    usage: ProviderUsage,
}

impl EffectiveChatResult {
    pub fn new(
        content: Zeroizing<String>,
        evidence: EffectiveChatEvidence,
        usage: ProviderUsage,
    ) -> Result<Self, ProviderError> {
        validate_effective_content(content.as_str())?;
        Ok(Self {
            content,
            evidence,
            usage,
        })
    }

    pub fn content(&self) -> &str {
        self.content.as_str()
    }

    pub const fn evidence(&self) -> EffectiveChatEvidence {
        self.evidence
    }

    pub const fn usage(&self) -> &ProviderUsage {
        &self.usage
    }

    pub fn into_content(self) -> Zeroizing<String> {
        self.content
    }

    pub fn into_parts(self) -> (Zeroizing<String>, EffectiveChatEvidence, ProviderUsage) {
        (self.content, self.evidence, self.usage)
    }
}

impl std::fmt::Debug for EffectiveChatResult {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("EffectiveChatResult")
            .field("content", &"[REDACTED]")
            .field("evidence", &self.evidence)
            .field("usage", &self.usage)
            .finish()
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum ChatSseEvent {
    Json(serde_json::Value),
    Done,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ChatCompletionsOutput {
    Json(serde_json::Value),
    Sse(Vec<ChatSseEvent>),
}

#[derive(Clone, Debug, PartialEq)]
pub struct ChatCompletionsResponse {
    pub content: String,
    pub usage: ProviderUsage,
    pub output: ChatCompletionsOutput,
}

#[derive(Clone, Debug, Serialize)]
pub struct EmbeddingsRequest {
    pub model: String,
    pub input: Vec<String>,
    pub encoding_format: &'static str,
}

impl EmbeddingsRequest {
    pub fn new(model: impl Into<String>, input: Vec<String>) -> Self {
        Self {
            model: model.into(),
            input,
            encoding_format: "float",
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct EmbeddingVector {
    pub index: usize,
    pub embedding: Vec<f32>,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct EmbeddingsResponse {
    pub data: Vec<EmbeddingVector>,
}

/// A bounded embeddings response accepted for governed result preparation.
///
/// It is intentionally separate from the backwards-compatible
/// [`EmbeddingsResponse`] carrier.  A governed provider attempt owns its
/// vectors exactly once and keeps them in zeroizing storage until the result
/// service seals them under the delivery output keys.
pub struct GovernedEmbeddingVector {
    index: usize,
    components: Zeroizing<Vec<f32>>,
}

impl GovernedEmbeddingVector {
    pub fn new(index: usize, components: Zeroizing<Vec<f32>>) -> Result<Self, ProviderError> {
        if components.is_empty() || components.iter().any(|component| !component.is_finite()) {
            return Err(ProviderError::InvalidResponse(
                "embedding response contains an invalid vector".into(),
            ));
        }
        Ok(Self { index, components })
    }

    /// Transfers provider components into the governed, zeroizing owner.
    pub fn from_provider_components(
        index: usize,
        components: Vec<f32>,
    ) -> Result<Self, ProviderError> {
        Self::new(index, Zeroizing::new(components))
    }

    pub const fn index(&self) -> usize {
        self.index
    }

    pub fn components(&self) -> &[f32] {
        self.components.as_slice()
    }

    pub fn into_components(self) -> Zeroizing<Vec<f32>> {
        self.components
    }
}

impl std::fmt::Debug for GovernedEmbeddingVector {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("GovernedEmbeddingVector")
            .field("index", &self.index)
            .field("dimensions", &self.components.len())
            .field("components", &"[REDACTED]")
            .finish()
    }
}

/// Production-only response shape with exact model and ordinal validation.
pub struct GovernedEmbeddingsResponse {
    model: String,
    data: Vec<GovernedEmbeddingVector>,
}

impl GovernedEmbeddingsResponse {
    pub fn new(
        expected_model: &str,
        model: String,
        data: Vec<GovernedEmbeddingVector>,
        expected_outputs: usize,
    ) -> Result<Self, ProviderError> {
        if model != expected_model {
            return Err(ProviderError::InvalidResponse(
                "embedding response model does not match the governed request".into(),
            ));
        }
        if data.len() != expected_outputs
            || data
                .iter()
                .enumerate()
                .any(|(ordinal, vector)| vector.index != ordinal)
        {
            return Err(ProviderError::InvalidResponse(
                "embedding response indices do not match the governed request".into(),
            ));
        }
        Ok(Self { model, data })
    }

    pub fn model(&self) -> &str {
        &self.model
    }

    pub fn vectors(&self) -> &[GovernedEmbeddingVector] {
        &self.data
    }

    pub fn into_vectors(self) -> Vec<GovernedEmbeddingVector> {
        self.data
    }
}

impl std::fmt::Debug for GovernedEmbeddingsResponse {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("GovernedEmbeddingsResponse")
            .field("model", &self.model)
            .field("vector_count", &self.data.len())
            .field("vectors", &"[REDACTED]")
            .finish()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EffectiveRequestKind {
    ModelsList,
    ChatCompletions,
    Embeddings,
}

impl EffectiveRequestKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ModelsList => "models_list",
            Self::ChatCompletions => "chat_completions",
            Self::Embeddings => "embeddings",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EffectiveRequestLimits {
    max_output_tokens: u32,
    max_inputs: u32,
    max_input_bytes: u32,
}

impl EffectiveRequestLimits {
    pub fn new(
        max_output_tokens: u32,
        max_inputs: u32,
        max_input_bytes: u32,
    ) -> Result<Self, ProviderError> {
        if max_output_tokens == 0
            || max_inputs == 0
            || max_inputs as usize > MAX_EFFECTIVE_REQUEST_ITEMS
            || max_input_bytes == 0
        {
            return Err(ProviderError::InvalidResponse(
                "effective request limits must be positive".into(),
            ));
        }
        if usize::try_from(max_input_bytes).unwrap_or(usize::MAX)
            > MAX_AGGREGATE_EFFECTIVE_REQUEST_BYTES
        {
            return Err(ProviderError::InvalidResponse(
                "effective request input-byte limit exceeds the fixed aggregate ceiling".into(),
            ));
        }
        Ok(Self {
            max_output_tokens,
            max_inputs,
            max_input_bytes,
        })
    }

    pub const fn max_output_tokens(self) -> u32 {
        self.max_output_tokens
    }

    pub const fn max_inputs(self) -> u32 {
        self.max_inputs
    }

    pub const fn max_input_bytes(self) -> u32 {
        self.max_input_bytes
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EffectiveSampling {
    temperature: f64,
    top_p: f64,
}

impl EffectiveSampling {
    pub fn new(temperature: f64, top_p: f64) -> Result<Self, ProviderError> {
        if !temperature.is_finite()
            || !(0.0..=2.0).contains(&temperature)
            || !top_p.is_finite()
            || !(0.0..=1.0).contains(&top_p)
            || top_p == 0.0
        {
            return Err(ProviderError::InvalidResponse(
                "effective sampling values are outside the pinned bounds".into(),
            ));
        }
        Ok(Self { temperature, top_p })
    }

    pub const fn temperature(self) -> f64 {
        self.temperature
    }

    pub const fn top_p(self) -> f64 {
        self.top_p
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EffectiveChatRole {
    System,
    User,
    Assistant,
    Tool,
}

impl EffectiveChatRole {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::System => "system",
            Self::User => "user",
            Self::Assistant => "assistant",
            Self::Tool => "tool",
        }
    }
}

pub struct EffectiveChatMessage {
    role: EffectiveChatRole,
    content: Zeroizing<String>,
}

impl EffectiveChatMessage {
    pub fn new(role: EffectiveChatRole, content: impl Into<String>) -> Result<Self, ProviderError> {
        Self::from_zeroizing_content(role, Zeroizing::new(content.into()))
    }

    pub fn from_zeroizing_content(
        role: EffectiveChatRole,
        content: Zeroizing<String>,
    ) -> Result<Self, ProviderError> {
        validate_effective_content(content.as_str())?;
        Ok(Self { role, content })
    }

    pub fn user(content: impl Into<String>) -> Self {
        Self::new(EffectiveChatRole::User, content)
            .expect("EffectiveChatMessage::user requires bounded non-empty content")
    }

    pub const fn role(&self) -> EffectiveChatRole {
        self.role
    }

    pub fn content(&self) -> &str {
        self.content.as_str()
    }
}

impl std::fmt::Debug for EffectiveChatMessage {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("EffectiveChatMessage")
            .field("role", &self.role)
            .field("content", &"[REDACTED]")
            .finish()
    }
}

pub struct EffectiveToolSchema {
    name: String,
    description: Option<String>,
    parameters: serde_json::Value,
    encoded_bytes: usize,
}

impl EffectiveToolSchema {
    pub fn new(
        name: impl Into<String>,
        description: Option<String>,
        parameters: serde_json::Value,
    ) -> Result<Self, ProviderError> {
        let name = name.into();
        if name.trim().is_empty() || name.len() > 128 || !parameters.is_object() {
            return Err(ProviderError::InvalidResponse(
                "effective tool schema is malformed".into(),
            ));
        }
        let parameter_bytes = serde_json::to_vec(&parameters)
            .map_err(|_| ProviderError::InvalidResponse("tool schema is invalid".into()))?
            .len();
        if description.as_ref().is_some_and(|value| value.len() > 1024)
            || parameter_bytes > MAX_EFFECTIVE_MATERIAL_BYTES
        {
            return Err(ProviderError::InvalidResponse(
                "effective tool schema exceeds its fixed bound".into(),
            ));
        }
        Ok(Self {
            name,
            description,
            parameters,
            encoded_bytes: parameter_bytes,
        })
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn description(&self) -> Option<&str> {
        self.description.as_deref()
    }

    pub fn parameters(&self) -> &serde_json::Value {
        &self.parameters
    }
}

impl std::fmt::Debug for EffectiveToolSchema {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("EffectiveToolSchema")
            .field("name", &self.name)
            .field("description_present", &self.description.is_some())
            .field("parameters", &"[REDACTED]")
            .finish()
    }
}

pub struct EffectiveChatCompletionsRequest {
    model: Zeroizing<String>,
    messages: Vec<EffectiveChatMessage>,
    sampling: EffectiveSampling,
    limits: EffectiveRequestLimits,
    tools: Vec<EffectiveToolSchema>,
    stream: bool,
}

pub struct EffectiveEmbeddingsRequest {
    model: Zeroizing<String>,
    inputs: Vec<Zeroizing<String>>,
    limits: EffectiveRequestLimits,
}

/// A reconstructed semantic provider request. It deliberately implements
/// neither `Clone` nor `Serialize`; the production adapter consumes it once.
///
/// ```compile_fail
/// let request = vestrace_application::EffectiveModelRequest::models_list();
/// let _copy = request.clone();
/// ```
/// ```compile_fail
/// let request = vestrace_application::EffectiveModelRequest::models_list();
/// let _bytes = serde_json::to_vec(&request).unwrap();
/// ```
pub enum EffectiveModelRequest {
    ModelsList,
    ChatCompletions(EffectiveChatCompletionsRequest),
    Embeddings(EffectiveEmbeddingsRequest),
}

/// Closed q1 request vocabulary.  This is deliberately separate from the
/// backwards-compatible `ChatCompletionsRequest`, whose JSON payload is not a
/// lawful carrier for qualification traffic.
pub enum Q1ProbeRequest {
    ModelsList,
    Chat(Q1ChatProbeRequest),
    Embeddings(Q1EmbeddingsProbeRequest),
}

/// A closed q1 outcome.  The adapter has already checked every
/// ordinal-specific oracle before this value exists, so it deliberately keeps
/// only safe structural facts rather than a provider body, tool arguments or
/// response content.
pub enum Q1ProbeResponse {
    ModelsList(Q1ModelsListProbeResult),
    Chat(Q1ChatProbeResult),
    Embeddings(Q1EmbeddingsProbeResult),
}

/// Closed failures for the qualification-only adapter entrypoint.  Numeric
/// HTTP status is safe protocol evidence; provider bodies, error strings,
/// correlation values and credentials never cross this boundary.
#[derive(Clone, Copy, Eq, PartialEq)]
pub enum Q1ProbeFailure {
    HttpStatus { status: u16 },
    OracleViolation,
    StructuralFailure,
    TransportFailure,
}

impl std::fmt::Debug for Q1ProbeFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::HttpStatus { status } => formatter
                .debug_struct("Q1ProbeFailure::HttpStatus")
                .field("status", status)
                .finish(),
            Self::OracleViolation => formatter.write_str("Q1ProbeFailure::OracleViolation"),
            Self::StructuralFailure => formatter.write_str("Q1ProbeFailure::StructuralFailure"),
            Self::TransportFailure => formatter.write_str("Q1ProbeFailure::TransportFailure"),
        }
    }
}

pub struct Q1ModelsListProbeResult {
    model_count: u16,
}

impl Q1ModelsListProbeResult {
    pub fn new(model_count: usize) -> Result<Self, ProviderError> {
        let model_count = u16::try_from(model_count).map_err(|_| {
            ProviderError::InvalidResponse(
                "q1 models response exceeds its pinned count bound".into(),
            )
        })?;
        if model_count == 0 {
            return Err(ProviderError::InvalidResponse(
                "q1 models response is empty".into(),
            ));
        }
        Ok(Self { model_count })
    }

    pub const fn model_count(&self) -> u16 {
        self.model_count
    }
}

pub struct Q1ChatProbeResult {
    usage: ProviderUsage,
}

impl Q1ChatProbeResult {
    pub const fn new(usage: ProviderUsage) -> Self {
        Self { usage }
    }

    pub const fn usage(&self) -> &ProviderUsage {
        &self.usage
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Q1EmbeddingUsage {
    prompt_tokens: u32,
    total_tokens: u32,
}

impl Q1EmbeddingUsage {
    pub fn new(prompt_tokens: u32, total_tokens: u32) -> Result<Self, ProviderError> {
        if total_tokens < prompt_tokens {
            return Err(ProviderError::InvalidResponse(
                "q1 embedding usage total is smaller than prompt usage".into(),
            ));
        }
        Ok(Self {
            prompt_tokens,
            total_tokens,
        })
    }

    pub const fn prompt_tokens(&self) -> u32 {
        self.prompt_tokens
    }

    pub const fn total_tokens(&self) -> u32 {
        self.total_tokens
    }
}

pub struct Q1EmbeddingsProbeResult {
    usage: Option<Q1EmbeddingUsage>,
}

impl Q1EmbeddingsProbeResult {
    pub const fn new(usage: Option<Q1EmbeddingUsage>) -> Self {
        Self { usage }
    }

    pub const fn usage(&self) -> Option<Q1EmbeddingUsage> {
        self.usage
    }
}

impl std::fmt::Debug for Q1ProbeResponse {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::ModelsList(_) => "Q1ProbeResponse::ModelsList",
            Self::Chat(_) => "Q1ProbeResponse::Chat",
            Self::Embeddings(_) => "Q1ProbeResponse::Embeddings",
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Q1ToolChoice {
    None,
    NamedProbe,
    Required,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Q1ResponseFormat {
    None,
    StrictNonceJsonSchema,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Q1ToolSet {
    None,
    SingleProbe,
    ParallelProbes,
}

pub enum Q1ChatMessage {
    UserText(Zeroizing<String>),
    AssistantToolCall {
        call_id: String,
        function_name: &'static str,
        argument_value: Zeroizing<String>,
    },
    ToolResult {
        call_id: String,
        content: Zeroizing<String>,
    },
    UserImage {
        text: Zeroizing<String>,
        data_url: Zeroizing<String>,
    },
}

impl Q1ChatMessage {
    pub fn user_text(content: impl Into<String>) -> Result<Self, ProviderError> {
        let content = Zeroizing::new(content.into());
        validate_effective_content(content.as_str())?;
        Ok(Self::UserText(content))
    }

    pub fn assistant_tool_call(
        call_id: impl Into<String>,
        function_name: &'static str,
        argument_value: impl Into<String>,
    ) -> Result<Self, ProviderError> {
        let call_id = call_id.into();
        let argument_value = Zeroizing::new(argument_value.into());
        if call_id.trim().is_empty()
            || call_id.len() > 256
            || function_name != "vestrace_probe"
            || argument_value.len() > MAX_EFFECTIVE_MATERIAL_BYTES
        {
            return Err(ProviderError::InvalidResponse(
                "q1 assistant tool-call replay is malformed".into(),
            ));
        }
        Ok(Self::AssistantToolCall {
            call_id,
            function_name,
            argument_value,
        })
    }

    pub fn tool_result(
        call_id: impl Into<String>,
        content: impl Into<String>,
    ) -> Result<Self, ProviderError> {
        let call_id = call_id.into();
        let content = Zeroizing::new(content.into());
        if call_id.trim().is_empty() || call_id.len() > 256 {
            return Err(ProviderError::InvalidResponse(
                "q1 tool result call id is malformed".into(),
            ));
        }
        validate_effective_content(content.as_str())?;
        Ok(Self::ToolResult { call_id, content })
    }

    pub fn user_image(
        text: impl Into<String>,
        data_url: impl Into<String>,
    ) -> Result<Self, ProviderError> {
        let text = Zeroizing::new(text.into());
        let data_url = Zeroizing::new(data_url.into());
        validate_effective_content(text.as_str())?;
        if !data_url.starts_with("data:image/png;base64,")
            || data_url.len() > MAX_EFFECTIVE_MATERIAL_BYTES
        {
            return Err(ProviderError::InvalidResponse(
                "q1 image input is malformed".into(),
            ));
        }
        Ok(Self::UserImage { text, data_url })
    }
}

impl std::fmt::Debug for Q1ChatMessage {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("Q1ChatMessage([REDACTED])")
    }
}

pub struct Q1ChatProbeRequest {
    ordinal: String,
    model: Zeroizing<String>,
    nonce: Zeroizing<String>,
    messages: Vec<Q1ChatMessage>,
    max_tokens: Option<u32>,
    temperature: Option<f64>,
    stream: bool,
    stream_include_usage: bool,
    tool_choice: Q1ToolChoice,
    parallel_tool_calls: bool,
    response_format: Q1ResponseFormat,
    tools: Q1ToolSet,
}

impl Q1ChatProbeRequest {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        ordinal: impl Into<String>,
        model: impl Into<String>,
        nonce: impl Into<String>,
        messages: Vec<Q1ChatMessage>,
        max_tokens: Option<u32>,
        temperature: Option<f64>,
        stream: bool,
        stream_include_usage: bool,
        tool_choice: Q1ToolChoice,
        parallel_tool_calls: bool,
        response_format: Q1ResponseFormat,
        tools: Q1ToolSet,
    ) -> Result<Self, ProviderError> {
        let ordinal = ordinal.into();
        let model = model.into();
        let nonce = nonce.into();
        validate_wire_model_id(&model)?;
        if nonce.len() != 24
            || !nonce
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
        {
            return Err(ProviderError::InvalidResponse(
                "q1 nonce is malformed".into(),
            ));
        }
        let exact_shape = match ordinal.as_str() {
            "20" => matches!(
                (
                    max_tokens,
                    temperature,
                    stream,
                    stream_include_usage,
                    tool_choice,
                    parallel_tool_calls,
                    response_format,
                    tools
                ),
                (
                    Some(32),
                    Some(0.0),
                    false,
                    false,
                    Q1ToolChoice::None,
                    false,
                    Q1ResponseFormat::None,
                    Q1ToolSet::None
                )
            ),
            "30" => matches!(
                (
                    max_tokens,
                    temperature,
                    stream,
                    stream_include_usage,
                    tool_choice,
                    parallel_tool_calls,
                    response_format,
                    tools
                ),
                (
                    Some(32),
                    Some(0.0),
                    true,
                    false,
                    Q1ToolChoice::None,
                    false,
                    Q1ResponseFormat::None,
                    Q1ToolSet::None
                )
            ),
            "35" => matches!(
                (
                    max_tokens,
                    temperature,
                    stream,
                    stream_include_usage,
                    tool_choice,
                    parallel_tool_calls,
                    response_format,
                    tools
                ),
                (
                    Some(32),
                    Some(0.0),
                    true,
                    true,
                    Q1ToolChoice::None,
                    false,
                    Q1ResponseFormat::None,
                    Q1ToolSet::None
                )
            ),
            "40" => matches!(
                (
                    max_tokens,
                    temperature,
                    stream,
                    stream_include_usage,
                    tool_choice,
                    parallel_tool_calls,
                    response_format,
                    tools
                ),
                (
                    None,
                    None,
                    false,
                    false,
                    Q1ToolChoice::NamedProbe,
                    false,
                    Q1ResponseFormat::None,
                    Q1ToolSet::SingleProbe
                )
            ),
            "50" => matches!(
                (
                    max_tokens,
                    temperature,
                    stream,
                    stream_include_usage,
                    tool_choice,
                    parallel_tool_calls,
                    response_format,
                    tools
                ),
                (
                    None,
                    None,
                    false,
                    false,
                    Q1ToolChoice::None,
                    false,
                    Q1ResponseFormat::None,
                    Q1ToolSet::None
                )
            ),
            "60" => matches!(
                (
                    max_tokens,
                    temperature,
                    stream,
                    stream_include_usage,
                    tool_choice,
                    parallel_tool_calls,
                    response_format,
                    tools
                ),
                (
                    None,
                    None,
                    false,
                    false,
                    Q1ToolChoice::Required,
                    true,
                    Q1ResponseFormat::None,
                    Q1ToolSet::ParallelProbes
                )
            ),
            "70" => matches!(
                (
                    max_tokens,
                    temperature,
                    stream,
                    stream_include_usage,
                    tool_choice,
                    parallel_tool_calls,
                    response_format,
                    tools
                ),
                (
                    None,
                    None,
                    false,
                    false,
                    Q1ToolChoice::None,
                    false,
                    Q1ResponseFormat::StrictNonceJsonSchema,
                    Q1ToolSet::None
                )
            ),
            "80" => matches!(
                (
                    max_tokens,
                    temperature,
                    stream,
                    stream_include_usage,
                    tool_choice,
                    parallel_tool_calls,
                    response_format,
                    tools
                ),
                (
                    None,
                    None,
                    false,
                    false,
                    Q1ToolChoice::None,
                    false,
                    Q1ResponseFormat::None,
                    Q1ToolSet::None
                )
            ),
            _ => false,
        };
        if !exact_shape || messages.is_empty() || messages.len() > 3 {
            return Err(ProviderError::InvalidResponse(
                "q1 chat request does not match a frozen probe shape".into(),
            ));
        }
        Ok(Self {
            ordinal,
            model: Zeroizing::new(model),
            nonce: Zeroizing::new(nonce),
            messages,
            max_tokens,
            temperature,
            stream,
            stream_include_usage,
            tool_choice,
            parallel_tool_calls,
            response_format,
            tools,
        })
    }

    pub fn ordinal(&self) -> &str {
        &self.ordinal
    }
    pub fn model(&self) -> &str {
        self.model.as_str()
    }
    pub fn nonce(&self) -> &str {
        self.nonce.as_str()
    }
    pub fn messages(&self) -> &[Q1ChatMessage] {
        &self.messages
    }
    pub const fn max_tokens(&self) -> Option<u32> {
        self.max_tokens
    }
    pub const fn temperature(&self) -> Option<f64> {
        self.temperature
    }
    pub const fn stream(&self) -> bool {
        self.stream
    }
    pub const fn stream_include_usage(&self) -> bool {
        self.stream_include_usage
    }
    pub const fn tool_choice(&self) -> Q1ToolChoice {
        self.tool_choice
    }
    pub const fn parallel_tool_calls(&self) -> bool {
        self.parallel_tool_calls
    }
    pub const fn response_format(&self) -> Q1ResponseFormat {
        self.response_format
    }
    pub const fn tools(&self) -> Q1ToolSet {
        self.tools
    }
}

pub struct Q1EmbeddingsProbeRequest {
    model: Zeroizing<String>,
    inputs: [Zeroizing<String>; 2],
}

impl Q1EmbeddingsProbeRequest {
    pub fn new(model: impl Into<String>, inputs: [String; 2]) -> Result<Self, ProviderError> {
        let model = model.into();
        validate_wire_model_id(&model)?;
        let inputs = inputs.map(Zeroizing::new);
        validate_input_set(
            inputs.iter().map(|input| input.as_str()),
            EffectiveRequestLimits::new(1, 2, MAX_EFFECTIVE_MATERIAL_BYTES as u32)?,
        )?;
        Ok(Self {
            model: Zeroizing::new(model),
            inputs,
        })
    }
    pub fn model(&self) -> &str {
        self.model.as_str()
    }
    pub fn inputs(&self) -> impl Iterator<Item = &str> {
        self.inputs.iter().map(|input| input.as_str())
    }
}

impl EffectiveModelRequest {
    pub const fn models_list() -> Self {
        Self::ModelsList
    }

    pub fn chat_completions(
        model: impl Into<String>,
        messages: Vec<EffectiveChatMessage>,
        sampling: EffectiveSampling,
        limits: EffectiveRequestLimits,
        tools: Vec<EffectiveToolSchema>,
        stream: bool,
    ) -> Result<Self, ProviderError> {
        let model = model.into();
        validate_wire_model_id(&model)?;
        validate_input_set(messages.iter().map(|message| message.content()), limits)?;
        let aggregate_bytes = messages
            .iter()
            .map(|message| message.content().len())
            .chain(tools.iter().map(|tool| {
                tool.name.len()
                    + tool.description.as_ref().map_or(0, String::len)
                    + tool.encoded_bytes
            }))
            .try_fold(0_usize, usize::checked_add)
            .ok_or_else(|| {
                ProviderError::InvalidResponse(
                    "effective request exceeds the fixed aggregate ceiling".into(),
                )
            })?;
        if tools.len() > MAX_EFFECTIVE_REQUEST_ITEMS
            || aggregate_bytes > MAX_AGGREGATE_EFFECTIVE_REQUEST_BYTES
        {
            return Err(ProviderError::InvalidResponse(
                "effective request exceeds the fixed aggregate ceiling".into(),
            ));
        }
        Ok(Self::ChatCompletions(EffectiveChatCompletionsRequest {
            model: Zeroizing::new(model),
            messages,
            sampling,
            limits,
            tools,
            stream,
        }))
    }

    pub fn embeddings(
        model: impl Into<String>,
        inputs: Vec<String>,
        limits: EffectiveRequestLimits,
    ) -> Result<Self, ProviderError> {
        Self::embeddings_from_zeroizing_inputs(
            model,
            inputs.into_iter().map(Zeroizing::new).collect(),
            limits,
        )
    }

    pub fn embeddings_from_zeroizing_inputs(
        model: impl Into<String>,
        inputs: Vec<Zeroizing<String>>,
        limits: EffectiveRequestLimits,
    ) -> Result<Self, ProviderError> {
        let model = model.into();
        validate_wire_model_id(&model)?;
        validate_input_set(inputs.iter().map(|input| input.as_str()), limits)?;
        Ok(Self::Embeddings(EffectiveEmbeddingsRequest {
            model: Zeroizing::new(model),
            inputs,
            limits,
        }))
    }

    pub const fn kind(&self) -> EffectiveRequestKind {
        match self {
            Self::ModelsList => EffectiveRequestKind::ModelsList,
            Self::ChatCompletions(_) => EffectiveRequestKind::ChatCompletions,
            Self::Embeddings(_) => EffectiveRequestKind::Embeddings,
        }
    }
}

impl EffectiveChatCompletionsRequest {
    pub fn model(&self) -> &str {
        self.model.as_str()
    }

    pub fn messages(&self) -> &[EffectiveChatMessage] {
        &self.messages
    }

    pub const fn sampling(&self) -> EffectiveSampling {
        self.sampling
    }

    pub const fn limits(&self) -> EffectiveRequestLimits {
        self.limits
    }

    pub fn tools(&self) -> &[EffectiveToolSchema] {
        &self.tools
    }

    pub const fn stream(&self) -> bool {
        self.stream
    }
}

impl EffectiveEmbeddingsRequest {
    pub fn model(&self) -> &str {
        self.model.as_str()
    }

    pub fn inputs(&self) -> impl Iterator<Item = &str> {
        self.inputs.iter().map(|input| input.as_str())
    }

    pub const fn limits(&self) -> EffectiveRequestLimits {
        self.limits
    }
}

impl std::fmt::Debug for EffectiveModelRequest {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::ModelsList => "EffectiveModelRequest::ModelsList",
            Self::ChatCompletions(_) => "EffectiveModelRequest::ChatCompletions([REDACTED])",
            Self::Embeddings(_) => "EffectiveModelRequest::Embeddings([REDACTED])",
        })
    }
}

pub enum EffectiveModelResponse {
    ModelsList(ModelsListResponse),
    ChatCompletions(EffectiveChatResult),
    Embeddings(GovernedEmbeddingsResponse),
}

fn validate_wire_model_id(model: &str) -> Result<(), ProviderError> {
    if model.trim().is_empty() || model.len() > 512 {
        return Err(ProviderError::InvalidResponse(
            "effective wire model id is malformed".into(),
        ));
    }
    Ok(())
}

fn validate_effective_content(content: &str) -> Result<(), ProviderError> {
    if content.is_empty() || content.len() > MAX_EFFECTIVE_MATERIAL_BYTES {
        return Err(ProviderError::InvalidResponse(
            "effective governed input is empty or exceeds its fixed bound".into(),
        ));
    }
    Ok(())
}

fn validate_input_set<'a>(
    inputs: impl Iterator<Item = &'a str>,
    limits: EffectiveRequestLimits,
) -> Result<(), ProviderError> {
    let mut count = 0_usize;
    let mut bytes = 0_usize;
    for input in inputs {
        validate_effective_content(input)?;
        count = count.saturating_add(1);
        bytes = bytes.saturating_add(input.len());
    }
    if count == 0
        || count > limits.max_inputs() as usize
        || bytes > limits.max_input_bytes() as usize
        || bytes > MAX_AGGREGATE_EFFECTIVE_REQUEST_BYTES
    {
        return Err(ProviderError::InvalidResponse(
            "effective governed inputs violate the pinned aggregate limits".into(),
        ));
    }
    Ok(())
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProviderResponseError {
    pub status: u16,
    pub code: Option<String>,
    pub error_type: Option<String>,
    pub correlation_id: Option<String>,
}

impl std::fmt::Display for ProviderResponseError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "provider returned status={}", self.status)?;
        if let Some(code) = &self.code {
            write!(formatter, " code={code}")?;
        }
        if let Some(error_type) = &self.error_type {
            write!(formatter, " type={error_type}")?;
        }
        if let Some(correlation_id) = &self.correlation_id {
            write!(formatter, " correlation_id={correlation_id}")?;
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GenerationRequest {
    pub model: String,
    pub prompt: String,
    pub max_tokens: Option<u32>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GenerationResponse {
    pub content: String,
    pub model: String,
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
}

#[derive(Debug, thiserror::Error)]
pub enum ProviderError {
    #[error("Timeout occurred")]
    Timeout,
    #[error("Rate limited")]
    RateLimited,
    #[error("Service unavailable: {0}")]
    Unavailable(String),
    /// The provider refused the credential.
    ///
    /// Separate from [`Self::InvalidResponse`] because it is the one provider
    /// failure an operator can act on directly, and calling it an unusable
    /// response sends them to read adapter code instead of rotating a key.
    #[error("Credential rejected: {0}")]
    CredentialRejected(String),
    #[error("Invalid response: {0}")]
    InvalidResponse(String),
}

#[async_trait]
pub trait TextGenerationProvider: Send + Sync {
    async fn generate(
        &self,
        request: GenerationRequest,
    ) -> Result<GenerationResponse, ProviderError>;
}

/// Immutable network facts supplied by the adapter that owns the client.
///
/// The executor must not infer locality from deployment configuration: this
/// descriptor describes the endpoint and transport policy of the client that
/// will actually carry provider input.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProviderEgress {
    endpoint: String,
    destination: DataDestination,
    redirects_disabled: bool,
    proxy_disabled: bool,
}

impl ProviderEgress {
    pub fn new(
        endpoint: impl Into<String>,
        destination: DataDestination,
        redirects_disabled: bool,
        proxy_disabled: bool,
    ) -> Self {
        Self {
            endpoint: endpoint.into(),
            destination,
            redirects_disabled,
            proxy_disabled,
        }
    }

    pub fn endpoint(&self) -> &str {
        &self.endpoint
    }

    pub fn destination(&self) -> DataDestination {
        self.destination
    }

    pub fn redirects_disabled(&self) -> bool {
        self.redirects_disabled
    }

    pub fn proxy_disabled(&self) -> bool {
        self.proxy_disabled
    }
}

pub struct ResolvedTextGenerationProvider {
    pub provider: std::sync::Arc<dyn TextGenerationProvider>,
    pub egress: ProviderEgress,
}

/// Builds a provider for a specific workspace.
///
/// A factory rather than a single shared provider, because the credential is a
/// per-workspace secret. One process-wide client would mean one workspace's key
/// being used to bill another's work.
///
/// Resolving per call also means a rotated key takes effect without a restart,
/// and the plaintext key is not held for the process's lifetime.
#[async_trait]
pub trait TextGenerationProviderFactory: Send + Sync {
    async fn provider_for(
        &self,
        context: &crate::RequestContext,
    ) -> Result<ResolvedTextGenerationProvider, crate::ApplicationError>;
}

pub type SharedTextGenerationProviderFactory = std::sync::Arc<dyn TextGenerationProviderFactory>;
