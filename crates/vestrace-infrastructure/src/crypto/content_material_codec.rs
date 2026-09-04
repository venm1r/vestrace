use ring::{
    aead::{AES_256_GCM, Aad, LessSafeKey, NONCE_LEN, Nonce, UnboundKey},
    rand::{SecureRandom, SystemRandom},
};
use vestrace_domain::{
    ConnectionId, ContentMaterialId, CredentialKeyCreationIntentId, CredentialRevisionId,
    CredentialSlotId, IntentNonce, MaterialKeyId, WorkspaceId, ZeroizingDek,
};
use zeroize::{Zeroize as _, Zeroizing};

const FRAME_MAGIC: &[u8; 4] = b"VMRF";
const FRAME_VERSION: u8 = 1;
const HEADER_BYTES: usize = FRAME_MAGIC.len() + 1 + NONCE_LEN;
const LENGTH_BYTES: usize = 8;
const TAG_BYTES: usize = 16;
const MIN_FRAME_BYTES: usize = 4096;
const CODEC_PROFILE: &str = "vestrace-content-material-aead-v1";

const CREDENTIAL_FRAME_MAGIC: &[u8; 4] = b"VCRF";
const CREDENTIAL_FRAME_VERSION: u8 = 2;
const CREDENTIAL_HEADER_BYTES: usize = CREDENTIAL_FRAME_MAGIC.len() + 1 + NONCE_LEN;
const CREDENTIAL_CODEC_PROFILE: &str = "credential_v2";

pub const MAX_FRAMED_CREDENTIAL_BYTES: usize = 64 * 1024;

pub const MAX_FRAMED_MATERIAL_BYTES: usize = 1024 * 1024;

#[derive(Debug, thiserror::Error, Eq, PartialEq)]
pub enum ContentMaterialCodecError {
    #[error("content material frame is malformed")]
    MalformedFrame,
    #[error("content material frame exceeds the fixed ceiling")]
    FrameTooLarge,
    #[error("content material exceeds the fixed plaintext ceiling")]
    ContentTooLarge,
    #[error("content material randomness is unavailable")]
    RandomnessUnavailable,
    #[error("content material encryption failed")]
    EncryptionFailed,
    #[error("content material authentication failed")]
    AuthenticationFailed,
    #[error("content material padding is invalid")]
    InvalidPadding,
}

pub struct ValidatedContentMaterialFrame<'a> {
    bytes: &'a [u8],
    nonce: [u8; NONCE_LEN],
}

impl std::fmt::Debug for ValidatedContentMaterialFrame<'_> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ValidatedContentMaterialFrame")
            .field("framed_size", &self.bytes.len())
            .finish()
    }
}

pub struct ContentMaterialCodec {
    random: SystemRandom,
}

/// Immutable identities authenticated with one `credential_v2` frame.
///
/// The prepared-attachment id is deliberately absent: it is lifecycle
/// metadata, not part of the encrypted credential's semantic identity.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CredentialMaterialContext<'a> {
    pub profile: &'a str,
    pub workspace_id: WorkspaceId,
    pub connection_id: ConnectionId,
    pub credential_slot_id: CredentialSlotId,
    pub credential_revision_id: CredentialRevisionId,
    pub material_key_id: MaterialKeyId,
    pub intent_id: CredentialKeyCreationIntentId,
    pub intent_nonce: IntentNonce,
}

#[derive(Debug, thiserror::Error, Eq, PartialEq)]
pub enum CredentialMaterialCodecError {
    #[error("credential associated-data profile is not dispatchable")]
    UnsupportedProfile,
    #[error("credential material must be nonempty UTF-8")]
    InvalidPlaintext,
    #[error("credential material frame is malformed")]
    MalformedFrame,
    #[error("credential material frame exceeds the fixed ceiling")]
    FrameTooLarge,
    #[error("credential material exceeds the fixed plaintext ceiling")]
    ContentTooLarge,
    #[error("credential material randomness is unavailable")]
    RandomnessUnavailable,
    #[error("credential material encryption failed")]
    EncryptionFailed,
    #[error("credential material authentication failed")]
    AuthenticationFailed,
    #[error("credential material padding is invalid")]
    InvalidPadding,
}

pub struct ValidatedCredentialMaterialFrame<'a> {
    bytes: &'a [u8],
    nonce: [u8; NONCE_LEN],
}

impl std::fmt::Debug for ValidatedCredentialMaterialFrame<'_> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ValidatedCredentialMaterialFrame")
            .field("framed_size", &self.bytes.len())
            .finish()
    }
}

/// The only writer/reader for dispatchable credential material.
pub struct CredentialMaterialCodec {
    random: SystemRandom,
}

impl Default for CredentialMaterialCodec {
    fn default() -> Self {
        Self::new()
    }
}

impl CredentialMaterialCodec {
    pub fn new() -> Self {
        Self {
            random: SystemRandom::new(),
        }
    }

    pub fn seal(
        &self,
        context: &CredentialMaterialContext<'_>,
        dek: &ZeroizingDek,
        plaintext: &[u8],
    ) -> Result<Vec<u8>, CredentialMaterialCodecError> {
        validate_credential_context(context)?;
        self.validate_plaintext(plaintext)?;
        let required = CREDENTIAL_HEADER_BYTES
            .checked_add(TAG_BYTES)
            .and_then(|value| value.checked_add(LENGTH_BYTES))
            .and_then(|value| value.checked_add(plaintext.len()))
            .ok_or(CredentialMaterialCodecError::ContentTooLarge)?;
        let frame_size = required
            .max(MIN_FRAME_BYTES)
            .checked_next_power_of_two()
            .ok_or(CredentialMaterialCodecError::ContentTooLarge)?;
        if frame_size > MAX_FRAMED_CREDENTIAL_BYTES {
            return Err(CredentialMaterialCodecError::ContentTooLarge);
        }

        let padded_plaintext_size = frame_size - CREDENTIAL_HEADER_BYTES - TAG_BYTES;
        let mut padded = Zeroizing::new(vec![0_u8; padded_plaintext_size]);
        padded[..LENGTH_BYTES].copy_from_slice(&(plaintext.len() as u64).to_be_bytes());
        padded[LENGTH_BYTES..LENGTH_BYTES + plaintext.len()].copy_from_slice(plaintext);

        let mut nonce = [0_u8; NONCE_LEN];
        self.random
            .fill(&mut nonce)
            .map_err(|_| CredentialMaterialCodecError::RandomnessUnavailable)?;
        let aad = credential_associated_data(context);
        dek.expose(|key| {
            let unbound = UnboundKey::new(&AES_256_GCM, key)
                .map_err(|_| CredentialMaterialCodecError::EncryptionFailed)?;
            LessSafeKey::new(unbound)
                .seal_in_place_append_tag(
                    Nonce::assume_unique_for_key(nonce),
                    Aad::from(aad.as_slice()),
                    &mut *padded,
                )
                .map_err(|_| CredentialMaterialCodecError::EncryptionFailed)
        })?;

        let mut frame = Vec::with_capacity(frame_size);
        frame.extend_from_slice(CREDENTIAL_FRAME_MAGIC);
        frame.push(CREDENTIAL_FRAME_VERSION);
        frame.extend_from_slice(&nonce);
        frame.extend_from_slice(&padded);
        debug_assert_eq!(frame.len(), frame_size);
        padded.zeroize();
        Ok(frame)
    }

    pub fn validate_plaintext(&self, plaintext: &[u8]) -> Result<(), CredentialMaterialCodecError> {
        if plaintext.is_empty() || std::str::from_utf8(plaintext).is_err() {
            return Err(CredentialMaterialCodecError::InvalidPlaintext);
        }
        let required = CREDENTIAL_HEADER_BYTES
            .checked_add(TAG_BYTES)
            .and_then(|value| value.checked_add(LENGTH_BYTES))
            .and_then(|value| value.checked_add(plaintext.len()))
            .ok_or(CredentialMaterialCodecError::ContentTooLarge)?;
        let frame_size = required
            .max(MIN_FRAME_BYTES)
            .checked_next_power_of_two()
            .ok_or(CredentialMaterialCodecError::ContentTooLarge)?;
        if frame_size > MAX_FRAMED_CREDENTIAL_BYTES {
            return Err(CredentialMaterialCodecError::ContentTooLarge);
        }
        Ok(())
    }

    pub fn validate_frame<'a>(
        &self,
        frame: &'a [u8],
    ) -> Result<ValidatedCredentialMaterialFrame<'a>, CredentialMaterialCodecError> {
        if frame.len() > MAX_FRAMED_CREDENTIAL_BYTES {
            return Err(CredentialMaterialCodecError::FrameTooLarge);
        }
        if frame.len() < MIN_FRAME_BYTES
            || !frame.len().is_power_of_two()
            || frame.get(..CREDENTIAL_FRAME_MAGIC.len()) != Some(CREDENTIAL_FRAME_MAGIC.as_slice())
            || frame.get(CREDENTIAL_FRAME_MAGIC.len()).copied() != Some(CREDENTIAL_FRAME_VERSION)
            || frame.len() < CREDENTIAL_HEADER_BYTES + TAG_BYTES + LENGTH_BYTES
        {
            return Err(CredentialMaterialCodecError::MalformedFrame);
        }
        let mut nonce = [0_u8; NONCE_LEN];
        nonce.copy_from_slice(&frame[CREDENTIAL_FRAME_MAGIC.len() + 1..CREDENTIAL_HEADER_BYTES]);
        Ok(ValidatedCredentialMaterialFrame {
            bytes: frame,
            nonce,
        })
    }

    pub fn open(
        &self,
        context: &CredentialMaterialContext<'_>,
        dek: &ZeroizingDek,
        frame: &[u8],
    ) -> Result<Zeroizing<Vec<u8>>, CredentialMaterialCodecError> {
        let validated = self.validate_frame(frame)?;
        self.open_validated(context, dek, validated)
    }

    pub fn open_validated(
        &self,
        context: &CredentialMaterialContext<'_>,
        dek: &ZeroizingDek,
        frame: ValidatedCredentialMaterialFrame<'_>,
    ) -> Result<Zeroizing<Vec<u8>>, CredentialMaterialCodecError> {
        validate_credential_context(context)?;
        let aad = credential_associated_data(context);
        let mut encrypted = Zeroizing::new(frame.bytes[CREDENTIAL_HEADER_BYTES..].to_vec());
        let plaintext_len = dek.expose(|key| {
            let unbound = UnboundKey::new(&AES_256_GCM, key)
                .map_err(|_| CredentialMaterialCodecError::AuthenticationFailed)?;
            LessSafeKey::new(unbound)
                .open_in_place(
                    Nonce::assume_unique_for_key(frame.nonce),
                    Aad::from(aad.as_slice()),
                    &mut encrypted,
                )
                .map(|opened| opened.len())
                .map_err(|_| CredentialMaterialCodecError::AuthenticationFailed)
        })?;
        encrypted.truncate(plaintext_len);
        let declared = encrypted
            .get(..LENGTH_BYTES)
            .and_then(|bytes| <[u8; LENGTH_BYTES]>::try_from(bytes).ok())
            .map(u64::from_be_bytes)
            .and_then(|value| usize::try_from(value).ok())
            .filter(|value| *value > 0)
            .ok_or(CredentialMaterialCodecError::InvalidPadding)?;
        let content_end = LENGTH_BYTES
            .checked_add(declared)
            .filter(|end| *end <= encrypted.len())
            .ok_or(CredentialMaterialCodecError::InvalidPadding)?;
        if encrypted[content_end..].iter().any(|byte| *byte != 0) {
            return Err(CredentialMaterialCodecError::InvalidPadding);
        }
        let plaintext = &encrypted[LENGTH_BYTES..content_end];
        std::str::from_utf8(plaintext)
            .map_err(|_| CredentialMaterialCodecError::InvalidPlaintext)?;
        Ok(Zeroizing::new(plaintext.to_vec()))
    }
}

impl Default for ContentMaterialCodec {
    fn default() -> Self {
        Self::new()
    }
}

impl ContentMaterialCodec {
    pub fn new() -> Self {
        Self {
            random: SystemRandom::new(),
        }
    }

    pub fn seal(
        &self,
        workspace_id: WorkspaceId,
        material_id: ContentMaterialId,
        material_key_id: MaterialKeyId,
        dek: &ZeroizingDek,
        plaintext: &[u8],
    ) -> Result<Vec<u8>, ContentMaterialCodecError> {
        let required = HEADER_BYTES
            .checked_add(TAG_BYTES)
            .and_then(|value| value.checked_add(LENGTH_BYTES))
            .and_then(|value| value.checked_add(plaintext.len()))
            .ok_or(ContentMaterialCodecError::ContentTooLarge)?;
        let frame_size = required
            .max(MIN_FRAME_BYTES)
            .checked_next_power_of_two()
            .ok_or(ContentMaterialCodecError::ContentTooLarge)?;
        if frame_size > MAX_FRAMED_MATERIAL_BYTES {
            return Err(ContentMaterialCodecError::ContentTooLarge);
        }

        let padded_plaintext_size = frame_size - HEADER_BYTES - TAG_BYTES;
        let mut padded = Zeroizing::new(vec![0_u8; padded_plaintext_size]);
        padded[..LENGTH_BYTES].copy_from_slice(&(plaintext.len() as u64).to_be_bytes());
        padded[LENGTH_BYTES..LENGTH_BYTES + plaintext.len()].copy_from_slice(plaintext);

        let mut nonce = [0_u8; NONCE_LEN];
        self.random
            .fill(&mut nonce)
            .map_err(|_| ContentMaterialCodecError::RandomnessUnavailable)?;
        let aad = associated_data(workspace_id, material_id, material_key_id);
        let seal_result = dek.expose(|key| {
            let unbound = UnboundKey::new(&AES_256_GCM, key)
                .map_err(|_| ContentMaterialCodecError::EncryptionFailed)?;
            LessSafeKey::new(unbound)
                .seal_in_place_append_tag(
                    Nonce::assume_unique_for_key(nonce),
                    Aad::from(aad.as_slice()),
                    &mut *padded,
                )
                .map_err(|_| ContentMaterialCodecError::EncryptionFailed)
        });
        seal_result?;

        let mut frame = Vec::with_capacity(frame_size);
        frame.extend_from_slice(FRAME_MAGIC);
        frame.push(FRAME_VERSION);
        frame.extend_from_slice(&nonce);
        frame.extend_from_slice(&padded);
        debug_assert_eq!(frame.len(), frame_size);
        padded.zeroize();
        Ok(frame)
    }

    pub fn validate_frame<'a>(
        &self,
        frame: &'a [u8],
    ) -> Result<ValidatedContentMaterialFrame<'a>, ContentMaterialCodecError> {
        if frame.len() > MAX_FRAMED_MATERIAL_BYTES {
            return Err(ContentMaterialCodecError::FrameTooLarge);
        }
        if frame.len() < MIN_FRAME_BYTES
            || !frame.len().is_power_of_two()
            || frame.get(..FRAME_MAGIC.len()) != Some(FRAME_MAGIC.as_slice())
            || frame.get(FRAME_MAGIC.len()).copied() != Some(FRAME_VERSION)
            || frame.len() < HEADER_BYTES + TAG_BYTES + LENGTH_BYTES
        {
            return Err(ContentMaterialCodecError::MalformedFrame);
        }
        let mut nonce = [0_u8; NONCE_LEN];
        nonce.copy_from_slice(&frame[FRAME_MAGIC.len() + 1..HEADER_BYTES]);
        Ok(ValidatedContentMaterialFrame {
            bytes: frame,
            nonce,
        })
    }

    pub fn open(
        &self,
        workspace_id: WorkspaceId,
        material_id: ContentMaterialId,
        material_key_id: MaterialKeyId,
        dek: &ZeroizingDek,
        frame: &[u8],
    ) -> Result<Zeroizing<Vec<u8>>, ContentMaterialCodecError> {
        let validated = self.validate_frame(frame)?;
        self.open_validated(workspace_id, material_id, material_key_id, dek, validated)
    }

    pub fn open_validated(
        &self,
        workspace_id: WorkspaceId,
        material_id: ContentMaterialId,
        material_key_id: MaterialKeyId,
        dek: &ZeroizingDek,
        frame: ValidatedContentMaterialFrame<'_>,
    ) -> Result<Zeroizing<Vec<u8>>, ContentMaterialCodecError> {
        let aad = associated_data(workspace_id, material_id, material_key_id);
        let mut encrypted = Zeroizing::new(frame.bytes[HEADER_BYTES..].to_vec());
        let plaintext_len = dek.expose(|key| {
            let unbound = UnboundKey::new(&AES_256_GCM, key)
                .map_err(|_| ContentMaterialCodecError::AuthenticationFailed)?;
            LessSafeKey::new(unbound)
                .open_in_place(
                    Nonce::assume_unique_for_key(frame.nonce),
                    Aad::from(aad.as_slice()),
                    &mut encrypted,
                )
                .map(|opened| opened.len())
                .map_err(|_| ContentMaterialCodecError::AuthenticationFailed)
        })?;
        encrypted.truncate(plaintext_len);
        let declared = encrypted
            .get(..LENGTH_BYTES)
            .and_then(|bytes| <[u8; LENGTH_BYTES]>::try_from(bytes).ok())
            .map(u64::from_be_bytes)
            .and_then(|value| usize::try_from(value).ok())
            .ok_or(ContentMaterialCodecError::InvalidPadding)?;
        let content_end = LENGTH_BYTES
            .checked_add(declared)
            .filter(|end| *end <= encrypted.len())
            .ok_or(ContentMaterialCodecError::InvalidPadding)?;
        if encrypted[content_end..].iter().any(|byte| *byte != 0) {
            return Err(ContentMaterialCodecError::InvalidPadding);
        }
        Ok(Zeroizing::new(
            encrypted[LENGTH_BYTES..content_end].to_vec(),
        ))
    }
}

fn associated_data(
    workspace_id: WorkspaceId,
    material_id: ContentMaterialId,
    material_key_id: MaterialKeyId,
) -> Vec<u8> {
    format!(
        "{CODEC_PROFILE}|{}|{}|{}",
        workspace_id.as_uuid(),
        material_id.as_uuid(),
        material_key_id.as_uuid()
    )
    .into_bytes()
}

fn validate_credential_context(
    context: &CredentialMaterialContext<'_>,
) -> Result<(), CredentialMaterialCodecError> {
    if context.profile != CREDENTIAL_CODEC_PROFILE {
        return Err(CredentialMaterialCodecError::UnsupportedProfile);
    }
    Ok(())
}

fn credential_associated_data(context: &CredentialMaterialContext<'_>) -> Vec<u8> {
    format!(
        "{}|{}|{}|{}|{}|{}|{}|{}",
        context.profile,
        context.workspace_id.as_uuid(),
        context.connection_id.as_uuid(),
        context.credential_slot_id.as_uuid(),
        context.credential_revision_id.as_uuid(),
        context.material_key_id.as_uuid(),
        context.intent_id.as_uuid(),
        context.intent_nonce.as_uuid()
    )
    .into_bytes()
}

/// The one production [`GovernedInputSealer`].
///
/// It adds no cryptography of its own: it is the existing content-material
/// framing, exposed through an application port so the governed Run-step
/// acceptance service can seal without depending on this crate.
impl vestrace_application::GovernedInputSealer for ContentMaterialCodec {
    fn seal(
        &self,
        workspace_id: vestrace_domain::WorkspaceId,
        material_id: vestrace_domain::ContentMaterialId,
        material_key_id: vestrace_domain::MaterialKeyId,
        dek: &vestrace_domain::ZeroizingDek,
        plaintext: &[u8],
    ) -> Result<Vec<u8>, vestrace_application::ApplicationError> {
        ContentMaterialCodec::seal(
            self,
            workspace_id,
            material_id,
            material_key_id,
            dek,
            plaintext,
        )
        .map_err(|error| vestrace_application::ApplicationError::Storage(error.to_string()))
    }
}

#[cfg(test)]
mod credential_tests {
    use super::*;

    fn context() -> CredentialMaterialContext<'static> {
        CredentialMaterialContext {
            profile: "credential_v2",
            workspace_id: WorkspaceId::new(),
            connection_id: ConnectionId::new(),
            credential_slot_id: CredentialSlotId::new(),
            credential_revision_id: CredentialRevisionId::new(),
            material_key_id: MaterialKeyId::new(),
            intent_id: CredentialKeyCreationIntentId::new(),
            intent_nonce: IntentNonce::new(),
        }
    }

    fn authenticated_frame(
        context: &CredentialMaterialContext<'_>,
        dek: &ZeroizingDek,
        mutate: impl FnOnce(&mut [u8]),
    ) -> Vec<u8> {
        let mut payload = vec![0_u8; MIN_FRAME_BYTES - CREDENTIAL_HEADER_BYTES - TAG_BYTES];
        mutate(&mut payload);
        let nonce = [0x91_u8; NONCE_LEN];
        let aad = credential_associated_data(context);
        dek.expose(|key| {
            let unbound = UnboundKey::new(&AES_256_GCM, key).unwrap();
            LessSafeKey::new(unbound)
                .seal_in_place_append_tag(
                    Nonce::assume_unique_for_key(nonce),
                    Aad::from(aad.as_slice()),
                    &mut payload,
                )
                .unwrap();
        });
        let mut frame = Vec::with_capacity(MIN_FRAME_BYTES);
        frame.extend_from_slice(CREDENTIAL_FRAME_MAGIC);
        frame.push(CREDENTIAL_FRAME_VERSION);
        frame.extend_from_slice(&nonce);
        frame.extend_from_slice(&payload);
        frame
    }

    #[test]
    fn credential_open_rejects_authenticated_padding_and_utf8_failures() {
        let codec = CredentialMaterialCodec::new();
        let context = context();
        let dek = ZeroizingDek::new([0x31; 32]);

        let invalid_padding = authenticated_frame(&context, &dek, |payload| {
            payload[..LENGTH_BYTES].copy_from_slice(&1_u64.to_be_bytes());
            payload[LENGTH_BYTES] = b'x';
            payload[LENGTH_BYTES + 1] = 1;
        });
        assert_eq!(
            codec.open(&context, &dek, &invalid_padding),
            Err(CredentialMaterialCodecError::InvalidPadding)
        );

        let invalid_utf8 = authenticated_frame(&context, &dek, |payload| {
            payload[..LENGTH_BYTES].copy_from_slice(&1_u64.to_be_bytes());
            payload[LENGTH_BYTES] = 0xff;
        });
        assert_eq!(
            codec.open(&context, &dek, &invalid_utf8),
            Err(CredentialMaterialCodecError::InvalidPlaintext)
        );
    }
}
