//! Bounded contiguous exact cosine index; all vector-derived storage zeroizes.
use std::{
    mem::size_of,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};
use uuid::Uuid;
use vestrace_application::{ApplicationError, embedding::index::*};
use vestrace_domain::{ContentMaterialId, embedding::CanonicalGenerationSnapshot};
use zeroize::{Zeroize, Zeroizing};

fn limit_error() -> ApplicationError {
    ApplicationError::Unavailable("embedding-index-memory-limit".into())
}
fn invalid() -> ApplicationError {
    ApplicationError::Policy("embedding-index-invalid-vector".into())
}
#[derive(Debug)]
pub struct IndexMemoryBudget {
    maximum: usize,
    used: AtomicUsize,
}
impl IndexMemoryBudget {
    pub fn new(maximum: usize) -> Arc<Self> {
        Arc::new(Self {
            maximum,
            used: AtomicUsize::new(0),
        })
    }
    pub fn used_bytes(&self) -> usize {
        self.used.load(Ordering::Acquire)
    }
    pub(super) fn reserve(self: &Arc<Self>, bytes: usize) -> Result<Reservation, ApplicationError> {
        self.used
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |used| {
                used.checked_add(bytes).filter(|next| *next <= self.maximum)
            })
            .map_err(|_| limit_error())?;
        Ok(Reservation {
            budget: self.clone(),
            bytes,
        })
    }
}
pub(super) struct Reservation {
    budget: Arc<IndexMemoryBudget>,
    bytes: usize,
}
impl Drop for Reservation {
    fn drop(&mut self) {
        self.budget.used.fetch_sub(self.bytes, Ordering::AcqRel);
    }
}
struct Member {
    projection_id: Uuid,
    projection_ordinal: u64,
    material_id: ContentMaterialId,
}
pub struct FlatEmbeddingIndex {
    snapshot: CanonicalGenerationSnapshot,
    registration: Uuid,
    dimensions: usize,
    vectors: Zeroizing<Vec<f32>>,
    norms: Zeroizing<Vec<f64>>,
    members: Vec<Member>,
    reservation: Reservation,
    #[cfg(test)]
    witness: Option<Arc<std::sync::atomic::AtomicBool>>,
}
impl Drop for FlatEmbeddingIndex {
    fn drop(&mut self) {
        self.vectors.as_mut_slice().zeroize();
        self.norms.as_mut_slice().zeroize();
        #[cfg(test)]
        if let Some(witness) = &self.witness {
            witness.store(
                self.vectors.len() == self.members.len() * self.dimensions
                    && self.norms.len() == self.members.len()
                    && self.vectors.iter().all(|v| *v == 0.0)
                    && self.norms.iter().all(|v| *v == 0.0),
                Ordering::Release,
            );
        }
    }
}
fn required_bytes(
    snapshot: &CanonicalGenerationSnapshot,
    count: usize,
    dimensions: usize,
) -> Result<usize, ApplicationError> {
    let pins = snapshot.space.canonical_identity().ok_or_else(invalid)?;
    let strings = [
        snapshot.space.name(),
        pins.adapter_profile_revision.as_str(),
        pins.returned_model.as_str(),
        pins.encoding_format.as_str(),
    ];
    let mut bytes = count
        .checked_mul(dimensions)
        .and_then(|n| n.checked_mul(size_of::<f32>()))
        .and_then(|n| n.checked_add(count.checked_mul(size_of::<f64>() + size_of::<Member>())?))
        .and_then(|n| n.checked_add(size_of::<FlatEmbeddingIndex>()))
        .ok_or_else(limit_error)?;
    for string in strings {
        bytes = bytes.checked_add(string.len()).ok_or_else(limit_error)?;
    }
    Ok(bytes)
}
impl FlatEmbeddingIndex {
    pub fn build(
        snapshot: CanonicalGenerationSnapshot,
        registration: Uuid,
        mut vectors: Vec<IndexVector>,
        limits: IndexLimits,
    ) -> Result<Self, ApplicationError> {
        let factory = FlatEmbeddingIndexFactory::new(IndexMemoryBudget::new(limits.max_bytes));
        let mut builder = factory.begin(snapshot, registration, limits)?;
        vectors.sort_by_key(|row| row.projection_ordinal);
        for vector in vectors {
            builder.push(vector)?;
        }
        builder.finish()
    }
    pub fn snapshot(&self) -> &CanonicalGenerationSnapshot {
        &self.snapshot
    }
    pub fn allocated_bytes(&self) -> usize {
        self.reservation.bytes
    }
    pub fn search(&self, query: &[f32], limit: usize) -> Result<Vec<IndexHit>, ApplicationError> {
        if query.len() != self.dimensions {
            return Err(invalid());
        }
        let query_norm = norm(query)?;
        if limit == 0 {
            return Ok(vec![]);
        }
        // Selection is bounded by the smaller of the requested limit and members.
        // No plaintext corpus copy or per-member score buffer is created.
        let capacity = limit.min(self.members.len());
        let mut hits: Vec<IndexHit> = Vec::new();
        hits.try_reserve_exact(capacity)
            .map_err(|_| limit_error())?;
        for (ordinal, member) in self.members.iter().enumerate() {
            let start = ordinal * self.dimensions;
            let dot = self.vectors[start..start + self.dimensions]
                .iter()
                .zip(query)
                .map(|(a, b)| f64::from(*a) * f64::from(*b))
                .sum::<f64>();
            let cosine = (dot / (self.norms[ordinal] * query_norm)).clamp(-1.0, 1.0);
            let hit = IndexHit {
                projection_id: member.projection_id,
                projection_ordinal: member.projection_ordinal,
                material_id: member.material_id,
                distance: 1.0 - cosine,
            };
            let position = hits
                .binary_search_by(|other| {
                    other
                        .distance
                        .total_cmp(&hit.distance)
                        .then_with(|| other.projection_ordinal.cmp(&hit.projection_ordinal))
                })
                .unwrap_or_else(|position| position);
            if position < capacity {
                if hits.len() == capacity {
                    hits.pop();
                }
                hits.insert(position, hit);
            }
        }
        Ok(hits)
    }
}
impl LocalEmbeddingIndex for FlatEmbeddingIndex {
    fn snapshot(&self) -> &CanonicalGenerationSnapshot {
        &self.snapshot
    }
    fn space_registration_id(&self) -> Uuid {
        self.registration
    }
    fn allocated_bytes(&self) -> usize {
        self.allocated_bytes()
    }
    fn search(&self, query: &[f32], limit: usize) -> Result<Vec<IndexHit>, ApplicationError> {
        self.search(query, limit)
    }
}
fn norm(values: &[f32]) -> Result<f64, ApplicationError> {
    if values.iter().any(|value| !value.is_finite()) {
        return Err(invalid());
    }
    let square = values
        .iter()
        .map(|v| f64::from(*v) * f64::from(*v))
        .sum::<f64>();
    if !square.is_finite() || square <= 0.0 {
        return Err(invalid());
    }
    Ok(square.sqrt())
}
pub struct FlatEmbeddingIndexFactory {
    budget: Arc<IndexMemoryBudget>,
}
impl FlatEmbeddingIndexFactory {
    pub fn new(budget: Arc<IndexMemoryBudget>) -> Self {
        Self { budget }
    }
}
pub struct FlatEmbeddingIndexBuilder {
    index: FlatEmbeddingIndex,
    expected: usize,
}
impl EmbeddingIndexFactory for FlatEmbeddingIndexFactory {
    type Index = FlatEmbeddingIndex;
    type Builder = FlatEmbeddingIndexBuilder;
    fn begin(
        &self,
        snapshot: CanonicalGenerationSnapshot,
        registration: Uuid,
        limits: IndexLimits,
    ) -> Result<Self::Builder, ApplicationError> {
        snapshot.validate().map_err(|_| invalid())?;
        if registration.is_nil() {
            return Err(invalid());
        }
        let count = usize::try_from(snapshot.member_count).map_err(|_| limit_error())?;
        if count > limits.max_members {
            return Err(limit_error());
        }
        let dimensions = usize::try_from(snapshot.space.dimensions()).map_err(|_| limit_error())?;
        let bytes = required_bytes(&snapshot, count, dimensions)?;
        if bytes > limits.max_bytes {
            return Err(limit_error());
        }
        let reservation = self.budget.reserve(bytes)?;
        let mut vectors = Zeroizing::new(Vec::new());
        let mut norms = Zeroizing::new(Vec::new());
        let mut members = Vec::new();
        vectors
            .try_reserve_exact(count.checked_mul(dimensions).ok_or_else(limit_error)?)
            .map_err(|_| limit_error())?;
        norms.try_reserve_exact(count).map_err(|_| limit_error())?;
        members
            .try_reserve_exact(count)
            .map_err(|_| limit_error())?;
        // try_reserve_exact may legally overallocate; refuse unaccounted capacity.
        if vectors.capacity() != count * dimensions
            || norms.capacity() != count
            || members.capacity() != count
        {
            return Err(limit_error());
        }
        Ok(FlatEmbeddingIndexBuilder {
            index: FlatEmbeddingIndex {
                snapshot,
                registration,
                dimensions,
                vectors,
                norms,
                members,
                reservation,
                #[cfg(test)]
                witness: None,
            },
            expected: count,
        })
    }
}
impl EmbeddingIndexBuilder for FlatEmbeddingIndexBuilder {
    type Index = FlatEmbeddingIndex;
    fn push(&mut self, vector: IndexVector) -> Result<(), ApplicationError> {
        if self.index.members.len() >= self.expected
            || vector.values.len() != self.index.dimensions
            || vector.projection_id.is_nil()
            || vector.material_id.as_uuid().is_nil()
            || vector.projection_ordinal == 0
            || vector.projection_ordinal > self.index.snapshot.built_through_projection_ordinal
            || self
                .index
                .members
                .last()
                .is_some_and(|last| last.projection_ordinal >= vector.projection_ordinal)
            || self.index.members.iter().any(|member| {
                member.projection_id == vector.projection_id
                    || member.material_id == vector.material_id
            })
        {
            return Err(invalid());
        }
        let norm = norm(&vector.values)?;
        self.index.vectors.extend_from_slice(&vector.values);
        self.index.norms.push(norm);
        self.index.members.push(Member {
            projection_id: vector.projection_id,
            projection_ordinal: vector.projection_ordinal,
            material_id: vector.material_id,
        });
        Ok(())
    }
    fn finish(self) -> Result<Self::Index, ApplicationError> {
        if self.index.members.len() != self.expected {
            return Err(invalid());
        }
        Ok(self.index)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::embedding_index::EmbeddingIndexRegistry;
    use std::sync::atomic::AtomicBool;
    use vestrace_domain::{
        CorpusGenerationId, ModelQualificationRevisionId, ModelRevisionId, WorkspaceId,
        embedding::{CanonicalEmbeddingSpace, EmbeddingSpaceKey},
    };
    fn snapshot() -> CanonicalGenerationSnapshot {
        CanonicalGenerationSnapshot::new(
            EmbeddingSpaceKey::canonical(
                WorkspaceId::new(),
                "unit",
                CanonicalEmbeddingSpace {
                    model_revision_id: ModelRevisionId::new(),
                    model_qualification_revision_id: ModelQualificationRevisionId::new(),
                    adapter_profile_revision: "q1".into(),
                    request_shape_revision_id: Uuid::now_v7(),
                    returned_model: "model".into(),
                    encoding_format: "float".into(),
                    dimensions: 2,
                },
            )
            .unwrap(),
            CorpusGenerationId::new(),
            1,
            2,
            1,
            1,
            1,
        )
        .unwrap()
    }
    #[test]
    fn embedding_index_registry_retains_budget_until_final_arc_and_zeroizes_before_release() {
        let first = snapshot();
        let bytes = required_bytes(&first, 1, 2).unwrap();
        let budget = IndexMemoryBudget::new(bytes * 2);
        let factory = FlatEmbeddingIndexFactory::new(budget.clone());
        let registry = EmbeddingIndexRegistry::new();
        let registration = Uuid::now_v7();
        let witness = Arc::new(AtomicBool::new(false));
        let mut builder = factory
            .begin(
                first.clone(),
                registration,
                IndexLimits {
                    max_members: 1,
                    max_bytes: bytes,
                },
            )
            .unwrap();
        builder.index.witness = Some(witness.clone());
        builder
            .push(IndexVector {
                projection_id: Uuid::now_v7(),
                projection_ordinal: 1,
                material_id: ContentMaterialId::new(),
                values: Zeroizing::new(vec![4.0, 3.0]),
            })
            .unwrap();
        let retained = Arc::new(builder.finish().unwrap());
        registry.install(retained.clone()).unwrap();
        let mut second = first.clone();
        second.generation_id = CorpusGenerationId::new();
        second.generation_epoch = 2;
        second.guard_version = 3;
        let mut builder = factory
            .begin(
                second.clone(),
                registration,
                IndexLimits {
                    max_members: 1,
                    max_bytes: bytes,
                },
            )
            .unwrap();
        builder
            .push(IndexVector {
                projection_id: Uuid::now_v7(),
                projection_ordinal: 1,
                material_id: ContentMaterialId::new(),
                values: Zeroizing::new(vec![1.0, 2.0]),
            })
            .unwrap();
        registry
            .install(Arc::new(builder.finish().unwrap()))
            .unwrap();
        assert_eq!(budget.used_bytes(), bytes * 2);
        assert!(!witness.load(Ordering::Acquire));
        assert!(
            factory
                .begin(
                    second.clone(),
                    registration,
                    IndexLimits {
                        max_members: 1,
                        max_bytes: bytes
                    }
                )
                .is_err()
        );
        assert!(!retained.vectors.is_empty());
        drop(retained);
        assert!(witness.load(Ordering::Acquire));
        assert_eq!(budget.used_bytes(), bytes);
        registry.remove_exact(&second, registration);
        assert_eq!(budget.used_bytes(), 0);
    }
    #[test]
    fn embedding_index_registry_rejects_equal_epoch_mismatch_and_zeroizes_rejected_candidate() {
        let first = snapshot();
        let budget = IndexMemoryBudget::new(4096);
        let factory = FlatEmbeddingIndexFactory::new(budget.clone());
        let registry = EmbeddingIndexRegistry::new();
        let registration = Uuid::now_v7();
        let mut builder = factory
            .begin(
                first.clone(),
                registration,
                IndexLimits {
                    max_members: 1,
                    max_bytes: 4096,
                },
            )
            .unwrap();
        builder
            .push(IndexVector {
                projection_id: Uuid::now_v7(),
                projection_ordinal: 1,
                material_id: ContentMaterialId::new(),
                values: Zeroizing::new(vec![1.0, 2.0]),
            })
            .unwrap();
        let original = Arc::new(builder.finish().unwrap());
        let original_bytes = original.allocated_bytes();
        registry.install(original.clone()).unwrap();
        let mut forged = first.clone();
        forged.generation_id = CorpusGenerationId::new();
        let witness = Arc::new(AtomicBool::new(false));
        let mut builder = factory
            .begin(
                forged,
                registration,
                IndexLimits {
                    max_members: 1,
                    max_bytes: 4096,
                },
            )
            .unwrap();
        builder.index.witness = Some(witness.clone());
        builder
            .push(IndexVector {
                projection_id: Uuid::now_v7(),
                projection_ordinal: 1,
                material_id: ContentMaterialId::new(),
                values: Zeroizing::new(vec![3.0, 4.0]),
            })
            .unwrap();
        let candidate = builder.finish().unwrap();
        assert!(!candidate.vectors.is_empty());
        let result = registry.install(Arc::new(candidate));
        assert!(
            registry.get(&first, registration).is_some(),
            "equal epoch cannot replace a different immutable snapshot"
        );
        assert!(result.is_err());
        assert!(witness.load(Ordering::Acquire));
        assert_eq!(budget.used_bytes(), original_bytes);
    }
    #[test]
    fn embedding_index_committed_epoch_purge_is_strict_and_retained_arc_keeps_budget() {
        let snap = snapshot();
        let registration = Uuid::now_v7();
        let budget = IndexMemoryBudget::new(4096);
        let factory = FlatEmbeddingIndexFactory::new(budget.clone());
        let registry = EmbeddingIndexRegistry::new();
        let witness = Arc::new(AtomicBool::new(false));
        let mut builder = factory
            .begin(
                snap.clone(),
                registration,
                IndexLimits {
                    max_members: 1,
                    max_bytes: 4096,
                },
            )
            .unwrap();
        builder.index.witness = Some(witness.clone());
        builder
            .push(IndexVector {
                projection_id: Uuid::now_v7(),
                projection_ordinal: 1,
                material_id: ContentMaterialId::new(),
                values: Zeroizing::new(vec![3.0, 4.0]),
            })
            .unwrap();
        let retained = Arc::new(builder.finish().unwrap());
        let bytes = retained.allocated_bytes();
        registry.install(retained.clone()).unwrap();
        registry.remove_space_before_epoch(
            snap.workspace_id,
            registration,
            snap.generation_epoch - 1,
        );
        assert!(registry.get(&snap, registration).is_some());
        registry.remove_space_before_epoch(snap.workspace_id, registration, snap.generation_epoch);
        assert!(registry.get(&snap, registration).is_some());
        registry.remove_space_before_epoch(
            snap.workspace_id,
            Uuid::now_v7(),
            snap.generation_epoch + 1,
        );
        assert!(registry.get(&snap, registration).is_some());
        registry.remove_space_before_epoch(
            snap.workspace_id,
            registration,
            snap.generation_epoch + 1,
        );
        assert!(registry.get(&snap, registration).is_none());
        assert_eq!(budget.used_bytes(), bytes);
        assert!(!retained.vectors.is_empty());
        assert!(!witness.load(Ordering::Acquire));
        drop(retained);
        assert!(witness.load(Ordering::Acquire));
        assert_eq!(budget.used_bytes(), 0);
    }
}
