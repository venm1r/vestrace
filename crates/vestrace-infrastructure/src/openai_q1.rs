//! The immutable OpenAI-compatible `q1` qualification profile.
//!
//! This module deliberately reads the checked-in manifest at compile time.
//! Qualification is a protocol, not an operator configurable collection of
//! probes: accepting a different order, optionality, or nonce construction
//! would make persisted evidence incomparable across restarts.

use std::fmt::Write as _;

use base64::{Engine as _, engine::general_purpose::STANDARD};
use sha2::{Digest, Sha256};
use vestrace_application::{
    Q1AssistantToolCallReplay, Q1ChatMessage, Q1ChatProbeRequest, Q1EmbeddingsProbeRequest,
    Q1MreSource, Q1ProbeRequest, Q1ResponseFormat, Q1SafeMessageLayout, Q1SafeResponseFormat,
    Q1SafeToolChoice, Q1ToolChoice, Q1ToolSet,
};
use vestrace_domain::{QualificationJobId, QualificationProbeResult};

/// The only profile revision understood by this runner.
pub const PROFILE_ID: &str = "openai-chat-completions-v1/q1";
const MANIFEST: &str =
    include_str!("../../../schemas/openai-compatible/openai-chat-completions-v1-q1.json");
const ORDINALS: [&str; 12] = [
    "00", "10", "15", "20", "30", "35", "40", "50", "60", "70", "80", "90",
];
const IMAGE_MARKER_PNG: &[u8] = include_bytes!("../../../tests/fixtures/openai-q1/marker.png");

#[derive(Debug, thiserror::Error)]
pub enum OpenAiQ1ProfileError {
    #[error("the pinned q1 manifest is malformed")]
    MalformedManifest,
    #[error("the pinned q1 manifest contract does not match its compiled runner")]
    ContractMismatch,
    #[error("{0} is not a q1 probe ordinal")]
    UnknownOrdinal(String),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OpenAiQ1Probe {
    ordinal: &'static str,
    network: bool,
    optional: bool,
    prerequisite: Option<&'static str>,
}

impl OpenAiQ1Probe {
    pub const fn ordinal(self) -> &'static str {
        self.ordinal
    }

    pub const fn is_network(self) -> bool {
        self.network
    }

    pub const fn is_optional(self) -> bool {
        self.optional
    }

    pub const fn prerequisite(self) -> Option<&'static str> {
        self.prerequisite
    }
}

/// Checked and fixed representation of the repository's q1 JSON contract.
#[derive(Clone, Debug)]
pub struct OpenAiQ1Profile {
    manifest_digest: [u8; 32],
    probes: [OpenAiQ1Probe; 12],
}

impl OpenAiQ1Profile {
    /// Loads and validates the source-controlled manifest.  The parser keeps
    /// the full JSON out of execution/persistence; only the closed structural
    /// facts needed to sequence the probe runner are retained.
    pub fn from_pinned_manifest() -> Result<Self, OpenAiQ1ProfileError> {
        let manifest: Manifest =
            serde_json::from_str(MANIFEST).map_err(|_| OpenAiQ1ProfileError::MalformedManifest)?;
        if manifest.profile_id != PROFILE_ID
            || manifest.schema_version != 1
            || manifest.no_automatic_retry_after_dispatch != Some(true)
            || manifest
                .nonce_derivation
                .as_ref()
                .map(NonceDerivation::is_exact)
                != Some(true)
        {
            return Err(OpenAiQ1ProfileError::ContractMismatch);
        }

        let mut documents = manifest.connection_probes;
        documents.extend(manifest.model_probes);
        let probes = std::array::from_fn(|index| {
            let expected = ORDINALS[index];
            let document = documents
                .iter()
                .find(|document| document.ordinal == expected)
                .expect("validated immediately below");
            OpenAiQ1Probe {
                ordinal: expected,
                network: document.network,
                optional: document.optional.unwrap_or(false),
                prerequisite: match expected {
                    "35" => Some("30"),
                    "50" | "60" => Some("40"),
                    _ => None,
                },
            }
        });

        if documents.len() != ORDINALS.len()
            || documents.iter().any(|document| {
                !ORDINALS.contains(&document.ordinal.as_str())
                    || documents
                        .iter()
                        .filter(|other| other.ordinal == document.ordinal)
                        .count()
                        != 1
            })
            || probes.iter().any(|probe| {
                let document = documents
                    .iter()
                    .find(|document| document.ordinal == probe.ordinal)
                    .expect("the ordinal was just checked");
                document.network != probe.network
                    || document.optional.unwrap_or(false) != probe.optional
                    || !document.has_exact_prerequisites(probe.ordinal)
            })
        {
            return Err(OpenAiQ1ProfileError::ContractMismatch);
        }

        Ok(Self {
            manifest_digest: Sha256::digest(MANIFEST.as_bytes()).into(),
            probes,
        })
    }

    pub const fn profile_id(&self) -> &'static str {
        PROFILE_ID
    }

    pub fn manifest_digest(&self) -> [u8; 32] {
        self.manifest_digest
    }

    pub fn probes(&self) -> &[OpenAiQ1Probe; 12] {
        &self.probes
    }

    pub fn probe(&self, ordinal: &str) -> Result<OpenAiQ1Probe, OpenAiQ1ProfileError> {
        self.probes
            .iter()
            .copied()
            .find(|probe| probe.ordinal == ordinal)
            .ok_or_else(|| OpenAiQ1ProfileError::UnknownOrdinal(ordinal.to_owned()))
    }

    /// The exact closed safe MRE source for one network ordinal.  The only
    /// cross-probe value is the bounded assistant tool-call identity needed by
    /// ordinal 50; it is not request content or a serialized response body.
    pub fn mre_source(
        &self,
        ordinal: &str,
        assistant_tool_call: Option<Q1AssistantToolCallReplay>,
    ) -> Result<Q1MreSource, OpenAiQ1ProfileError> {
        if !self.probe(ordinal)?.is_network() {
            return Err(OpenAiQ1ProfileError::UnknownOrdinal(ordinal.to_owned()));
        }
        let source = match ordinal {
            "10" | "90" => Q1MreSource::new(
                ordinal,
                Q1SafeMessageLayout::PlainText,
                Q1SafeToolChoice::None,
                false,
                Q1SafeResponseFormat::None,
                false,
                false,
                None,
            ),
            "20" => Q1MreSource::new(
                ordinal,
                Q1SafeMessageLayout::PlainText,
                Q1SafeToolChoice::None,
                false,
                Q1SafeResponseFormat::None,
                false,
                false,
                None,
            ),
            "30" => Q1MreSource::new(
                ordinal,
                Q1SafeMessageLayout::PlainText,
                Q1SafeToolChoice::None,
                false,
                Q1SafeResponseFormat::None,
                true,
                false,
                None,
            ),
            "35" => Q1MreSource::new(
                ordinal,
                Q1SafeMessageLayout::PlainText,
                Q1SafeToolChoice::None,
                false,
                Q1SafeResponseFormat::None,
                true,
                true,
                None,
            ),
            "40" => Q1MreSource::new(
                ordinal,
                Q1SafeMessageLayout::PlainText,
                Q1SafeToolChoice::NamedProbe,
                false,
                Q1SafeResponseFormat::None,
                false,
                false,
                None,
            ),
            "50" => Q1MreSource::new(
                ordinal,
                Q1SafeMessageLayout::AssistantToolCallReplay,
                Q1SafeToolChoice::None,
                false,
                Q1SafeResponseFormat::None,
                false,
                false,
                assistant_tool_call,
            ),
            "60" => Q1MreSource::new(
                ordinal,
                Q1SafeMessageLayout::PlainText,
                Q1SafeToolChoice::Required,
                true,
                Q1SafeResponseFormat::None,
                false,
                false,
                None,
            ),
            "70" => Q1MreSource::new(
                ordinal,
                Q1SafeMessageLayout::PlainText,
                Q1SafeToolChoice::None,
                false,
                Q1SafeResponseFormat::StrictNonceJsonSchema,
                false,
                false,
                None,
            ),
            "80" => Q1MreSource::new(
                ordinal,
                Q1SafeMessageLayout::MultipartImageMarker,
                Q1SafeToolChoice::None,
                false,
                Q1SafeResponseFormat::None,
                false,
                false,
                None,
            ),
            _ => return Err(OpenAiQ1ProfileError::UnknownOrdinal(ordinal.to_owned())),
        };
        source.map_err(|_| OpenAiQ1ProfileError::ContractMismatch)
    }

    /// Builds the only request forms that q1 is allowed to hand to the
    /// production adapter.  The profile, target-pinned wire ids and nonce are
    /// the complete input; callers cannot supply a compatibility JSON body.
    pub fn typed_request(
        &self,
        qualification_job_id: QualificationJobId,
        ordinal: &str,
        qualified_chat_wire_model_id: &str,
        qualified_embedding_wire_model_id: &str,
        assistant_tool_call: Option<Q1AssistantToolCallReplay>,
    ) -> Result<Q1ProbeRequest, OpenAiQ1ProfileError> {
        let probe = self.probe(ordinal)?;
        if !probe.is_network() {
            return Err(OpenAiQ1ProfileError::UnknownOrdinal(ordinal.to_owned()));
        }
        let nonce = self.nonce(qualification_job_id, ordinal)?;
        let text = format!("Return exactly VESTRACE_Q1_TEXT_{nonce} and nothing else.");
        let request = match ordinal {
            "10" => Q1ProbeRequest::ModelsList,
            "20" => Q1ProbeRequest::Chat(q1_chat(
                ordinal,
                qualified_chat_wire_model_id,
                &nonce,
                vec![
                    Q1ChatMessage::user_text(text)
                        .map_err(|_| OpenAiQ1ProfileError::ContractMismatch)?,
                ],
                Some(32),
                Some(0.0),
                false,
                false,
                Q1ToolChoice::None,
                false,
                Q1ResponseFormat::None,
                Q1ToolSet::None,
            )?),
            "30" => Q1ProbeRequest::Chat(q1_chat(
                ordinal,
                qualified_chat_wire_model_id,
                &nonce,
                vec![
                    Q1ChatMessage::user_text(text)
                        .map_err(|_| OpenAiQ1ProfileError::ContractMismatch)?,
                ],
                Some(32),
                Some(0.0),
                true,
                false,
                Q1ToolChoice::None,
                false,
                Q1ResponseFormat::None,
                Q1ToolSet::None,
            )?),
            "35" => Q1ProbeRequest::Chat(q1_chat(
                ordinal,
                qualified_chat_wire_model_id,
                &nonce,
                vec![
                    Q1ChatMessage::user_text(text)
                        .map_err(|_| OpenAiQ1ProfileError::ContractMismatch)?,
                ],
                Some(32),
                Some(0.0),
                true,
                true,
                Q1ToolChoice::None,
                false,
                Q1ResponseFormat::None,
                Q1ToolSet::None,
            )?),
            "40" => Q1ProbeRequest::Chat(q1_chat(
                ordinal,
                qualified_chat_wire_model_id,
                &nonce,
                vec![
                    Q1ChatMessage::user_text(format!("Call vestrace_probe with value {nonce}."))
                        .map_err(|_| OpenAiQ1ProfileError::ContractMismatch)?,
                ],
                None,
                None,
                false,
                false,
                Q1ToolChoice::NamedProbe,
                false,
                Q1ResponseFormat::None,
                Q1ToolSet::SingleProbe,
            )?),
            "50" => {
                let call = assistant_tool_call.ok_or(OpenAiQ1ProfileError::ContractMismatch)?;
                let call_id = call.call_id().to_owned();
                Q1ProbeRequest::Chat(q1_chat(
                    ordinal,
                    qualified_chat_wire_model_id,
                    &nonce,
                    vec![
                        Q1ChatMessage::assistant_tool_call(
                            call_id.clone(),
                            "vestrace_probe",
                            nonce.clone(),
                        )
                        .map_err(|_| OpenAiQ1ProfileError::ContractMismatch)?,
                        Q1ChatMessage::tool_result(
                            call_id,
                            format!("VESTRACE_Q1_TOOL_RESULT_{nonce}"),
                        )
                        .map_err(|_| OpenAiQ1ProfileError::ContractMismatch)?,
                        Q1ChatMessage::user_text(format!(
                            "Return exactly VESTRACE_Q1_TOOL_DONE_{nonce} and nothing else."
                        ))
                        .map_err(|_| OpenAiQ1ProfileError::ContractMismatch)?,
                    ],
                    None,
                    None,
                    false,
                    false,
                    Q1ToolChoice::None,
                    false,
                    Q1ResponseFormat::None,
                    Q1ToolSet::None,
                )?)
            }
            "60" => Q1ProbeRequest::Chat(q1_chat(
                ordinal,
                qualified_chat_wire_model_id,
                &nonce,
                vec![
                    Q1ChatMessage::user_text(
                        "Call both required tools with their exact nonce values.",
                    )
                    .map_err(|_| OpenAiQ1ProfileError::ContractMismatch)?,
                ],
                None,
                None,
                false,
                false,
                Q1ToolChoice::Required,
                true,
                Q1ResponseFormat::None,
                Q1ToolSet::ParallelProbes,
            )?),
            "70" => Q1ProbeRequest::Chat(q1_chat(
                ordinal,
                qualified_chat_wire_model_id,
                &nonce,
                vec![
                    Q1ChatMessage::user_text(format!(
                        "Return the required strict JSON object for nonce {nonce}."
                    ))
                    .map_err(|_| OpenAiQ1ProfileError::ContractMismatch)?,
                ],
                None,
                None,
                false,
                false,
                Q1ToolChoice::None,
                false,
                Q1ResponseFormat::StrictNonceJsonSchema,
                Q1ToolSet::None,
            )?),
            "80" => Q1ProbeRequest::Chat(q1_chat(
                ordinal,
                qualified_chat_wire_model_id,
                &nonce,
                vec![
                    Q1ChatMessage::user_image(
                        "Return exactly VESTRACE_Q1_IMAGE and nothing else.",
                        format!(
                            "data:image/png;base64,{}",
                            STANDARD.encode(IMAGE_MARKER_PNG)
                        ),
                    )
                    .map_err(|_| OpenAiQ1ProfileError::ContractMismatch)?,
                ],
                None,
                None,
                false,
                false,
                Q1ToolChoice::None,
                false,
                Q1ResponseFormat::None,
                Q1ToolSet::None,
            )?),
            "90" => Q1ProbeRequest::Embeddings(
                Q1EmbeddingsProbeRequest::new(
                    qualified_embedding_wire_model_id,
                    [
                        format!("VESTRACE_Q1_EMBED_{nonce}_A"),
                        format!("VESTRACE_Q1_EMBED_{nonce}_B"),
                    ],
                )
                .map_err(|_| OpenAiQ1ProfileError::ContractMismatch)?,
            ),
            _ => return Err(OpenAiQ1ProfileError::UnknownOrdinal(ordinal.to_owned())),
        };
        Ok(request)
    }

    /// RFC 4122 UUID bytes, then the raw (not hex encoded) manifest SHA-256,
    /// then exactly two ASCII ordinal bytes. The first 96 output bits are
    /// lower-case hexadecimal.  This has a fixed vector below so changing byte
    /// order cannot silently look plausible.
    pub fn nonce(
        &self,
        qualification_job_id: QualificationJobId,
        ordinal: &str,
    ) -> Result<String, OpenAiQ1ProfileError> {
        self.probe(ordinal)?;
        let mut digest = Sha256::new();
        digest.update(qualification_job_id.as_uuid().as_bytes());
        digest.update(self.manifest_digest);
        digest.update(ordinal.as_bytes());
        let output = digest.finalize();
        let mut nonce = String::with_capacity(24);
        for byte in &output[..12] {
            write!(&mut nonce, "{byte:02x}").expect("writing to a String cannot fail");
        }
        Ok(nonce)
    }

    /// Classification is intentionally small and safe: it maps the frozen
    /// q1 transport rules without retaining provider bodies or errors.
    pub fn classify_completed_status(
        &self,
        ordinal: &str,
        status: u16,
    ) -> Result<QualificationProbeResult, OpenAiQ1ProfileError> {
        let probe = self.probe(ordinal)?;
        Ok(match status {
            200..=299 => QualificationProbeResult::Pass,
            400 | 404 | 405 | 415 | 422 if probe.optional => {
                QualificationProbeResult::UnsupportedDefinite
            }
            _ => QualificationProbeResult::FailedDefinite,
        })
    }
}

#[allow(clippy::too_many_arguments)]
fn q1_chat(
    ordinal: &str,
    model: &str,
    nonce: &str,
    messages: Vec<Q1ChatMessage>,
    max_tokens: Option<u32>,
    temperature: Option<f64>,
    stream: bool,
    stream_include_usage: bool,
    tool_choice: Q1ToolChoice,
    parallel_tool_calls: bool,
    response_format: Q1ResponseFormat,
    tools: Q1ToolSet,
) -> Result<Q1ChatProbeRequest, OpenAiQ1ProfileError> {
    Q1ChatProbeRequest::new(
        ordinal,
        model,
        nonce,
        messages,
        max_tokens,
        temperature,
        stream,
        stream_include_usage,
        tool_choice,
        parallel_tool_calls,
        response_format,
        tools,
    )
    .map_err(|_| OpenAiQ1ProfileError::ContractMismatch)
}

#[derive(serde::Deserialize)]
struct Manifest {
    profile_id: String,
    schema_version: u32,
    #[serde(default)]
    no_automatic_retry_after_dispatch: Option<bool>,
    nonce_derivation: Option<NonceDerivation>,
    connection_probes: Vec<ProbeDocument>,
    model_probes: Vec<ProbeDocument>,
}

#[derive(serde::Deserialize)]
struct NonceDerivation {
    encoding: String,
    input: String,
    sha256_bits: u16,
}

impl NonceDerivation {
    fn is_exact(&self) -> bool {
        self.encoding == "24_lowercase_hex_characters"
            && self.input == "QualificationJobId || profile_digest || probe_ordinal"
            && self.sha256_bits == 96
    }
}

#[derive(serde::Deserialize)]
struct ProbeDocument {
    ordinal: String,
    network: bool,
    optional: Option<bool>,
    prerequisites: Option<Vec<PrerequisiteDocument>>,
}

impl ProbeDocument {
    fn has_exact_prerequisites(&self, ordinal: &str) -> bool {
        let prerequisites = self.prerequisites.as_deref().unwrap_or_default();
        match ordinal {
            "35" => matches!(
                prerequisites,
                [PrerequisiteDocument { ordinal, result }]
                    if ordinal == "30" && result == "Pass"
            ),
            "50" | "60" => matches!(
                prerequisites,
                [PrerequisiteDocument { ordinal, result }]
                    if ordinal == "40" && result == "Pass"
            ),
            _ => prerequisites.is_empty(),
        }
    }
}

#[derive(serde::Deserialize)]
struct PrerequisiteDocument {
    ordinal: String,
    result: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pinned_profile_has_the_exact_order_and_static_probe_boundary() {
        let profile = OpenAiQ1Profile::from_pinned_manifest().unwrap();

        assert_eq!(
            profile
                .probes()
                .iter()
                .map(|probe| probe.ordinal())
                .collect::<Vec<_>>(),
            ORDINALS
        );
        assert!(!profile.probe("00").unwrap().is_network());
        assert!(!profile.probe("15").unwrap().is_network());
        assert!(profile.probe("10").unwrap().is_network());
        assert!(profile.probe("30").unwrap().is_optional());
        assert_eq!(profile.probe("35").unwrap().prerequisite(), Some("30"));
    }

    #[test]
    fn nonce_uses_uuid_bytes_raw_manifest_digest_and_ascii_ordinal() {
        let profile = OpenAiQ1Profile::from_pinned_manifest().unwrap();
        let job = QualificationJobId::from_uuid(
            uuid::Uuid::parse_str("00112233-4455-6677-8899-aabbccddeeff").unwrap(),
        );

        assert_eq!(
            profile.nonce(job, "20").unwrap(),
            "f4ce2be7497ee8d14ebc5518"
        );
    }

    #[test]
    fn optional_unsupported_is_closed_to_the_manifest_statuses() {
        let profile = OpenAiQ1Profile::from_pinned_manifest().unwrap();

        assert_eq!(
            profile.classify_completed_status("30", 404).unwrap(),
            QualificationProbeResult::UnsupportedDefinite
        );
        assert_eq!(
            profile.classify_completed_status("20", 404).unwrap(),
            QualificationProbeResult::FailedDefinite
        );
    }

    #[test]
    fn q1_sources_cover_the_closed_manifest_options() {
        let profile = OpenAiQ1Profile::from_pinned_manifest().unwrap();
        assert!(
            profile.mre_source("80", None).unwrap().message_layout()
                == Q1SafeMessageLayout::MultipartImageMarker
        );
        assert!(
            profile
                .mre_source("60", None)
                .unwrap()
                .parallel_tool_calls()
        );
        assert!(
            profile
                .mre_source("35", None)
                .unwrap()
                .stream_include_usage()
        );
        assert!(profile.mre_source("50", None).is_err());
        let replay = Q1AssistantToolCallReplay::new("call_q1").unwrap();
        assert_eq!(
            profile.mre_source("50", Some(replay)).unwrap().ordinal(),
            "50"
        );
    }
}
