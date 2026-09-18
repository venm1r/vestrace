pub mod flat;
pub mod registry;
pub use flat::{
    FlatEmbeddingIndex, FlatEmbeddingIndexBuilder, FlatEmbeddingIndexFactory, IndexMemoryBudget,
};
pub use registry::EmbeddingIndexRegistry;
pub use vestrace_application::embedding::index::{IndexHit, IndexLimits, IndexVector};

use std::sync::Arc;
use vestrace_application::{
    ApplicationError, RequestContext,
    embedding::index::{EmbeddingIndexDecoder, EncryptedIndexProjection},
};
use vestrace_domain::ZeroizingDek;
use zeroize::Zeroizing;
/// Temporary AEAD plaintext and decoded components share the index memory budget.
pub struct ContentMaterialIndexDecoder {
    codec: crate::crypto::ContentMaterialCodec,
    budget: Arc<IndexMemoryBudget>,
}
impl ContentMaterialIndexDecoder {
    pub fn new(budget: Arc<IndexMemoryBudget>) -> Self {
        Self {
            codec: crate::crypto::ContentMaterialCodec::new(),
            budget,
        }
    }
}
impl EmbeddingIndexDecoder for ContentMaterialIndexDecoder {
    fn decode_into(
        &self,
        context: &RequestContext,
        projection: &EncryptedIndexProjection,
        dek: &ZeroizingDek,
        dimensions: u32,
        consume: &mut dyn FnMut(Zeroizing<Vec<f32>>) -> Result<(), ApplicationError>,
    ) -> Result<(), ApplicationError> {
        let invalid = || ApplicationError::Policy("embedding-index-invalid-vector".into());
        let limit = || ApplicationError::Unavailable("embedding-index-memory-limit".into());
        if projection.binding.workspace_id != context.workspace_id || dimensions == 0 {
            return Err(invalid());
        }
        let length = usize::try_from(dimensions)
            .map_err(|_| limit())?
            .checked_mul(4)
            .ok_or_else(limit)?;
        let temporary = projection
            .ciphertext
            .len()
            .checked_mul(2)
            .and_then(|n| n.checked_add(length.checked_mul(2)?))
            .and_then(|n| n.checked_add(512))
            .ok_or_else(limit)?;
        let reservation = self.budget.reserve(temporary)?;
        let plaintext = self
            .codec
            .open(
                context.workspace_id,
                projection.binding.material_id,
                projection.binding.key_id,
                dek,
                &projection.ciphertext,
            )
            .map_err(|_| invalid())?;
        if plaintext.len() != length {
            return Err(invalid());
        }
        let mut values = Zeroizing::new(Vec::new());
        values
            .try_reserve_exact(dimensions as usize)
            .map_err(|_| limit())?;
        for bytes in plaintext.chunks_exact(4) {
            let value =
                f32::from_bits(u32::from_be_bytes(bytes.try_into().map_err(|_| invalid())?));
            if !value.is_finite() {
                return Err(invalid());
            }
            values.push(value);
        }
        let result = consume(values);
        drop(plaintext);
        drop(reservation);
        result
    }
}
