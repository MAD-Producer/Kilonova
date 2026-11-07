//! Content-bound frozen alignment. Digests detect corruption, not an adversary
//! who can rewrite both payloads and checksums. The actual planes are re-fitted
//! after reading; serialized coefficients never become an instance proof.
use super::*;
use sha2::{Digest, Sha256};

#[derive(Default)]
pub(super) struct Restored {
    // Bounded by the already checked layer count; never process-global. Aliases
    // of identical immutable planes/policy must not repeat a full-image fit.
    proofs: std::collections::HashMap<
        (usize, usize, [u8; 32]),
        crate::depth_snapshot::alignment::Frozen,
    >,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Aligned {
    pub reference: usize,
    spec: crate::depth_snapshot::alignment::Spec,
    reference_digest: [u8; 32],
    candidate_digest: [u8; 32],
    metadata_digest: [u8; 32],
}

impl Aligned {
    fn seal(&self, layer: &Layer) -> Result<[u8; 32]> {
        let bytes = serde_json::to_vec(&(
            "motion-frozen-alignment-v1",
            layer.object,
            &layer.asset,
            &layer.key,
            layer.input,
            &layer.calibration,
            layer.resource,
            layer.request,
            layer.decoded_budget,
            self.reference,
            &self.spec,
            self.reference_digest,
            self.candidate_digest,
        ))
        .map_err(|cause| crate::Error::with_context("encode frozen alignment integrity", cause))?;
        Ok(Sha256::digest(bytes).into())
    }

    pub(super) fn validate_metadata(&self, layer: &Layer) -> Result<()> {
        self.spec.validate(&layer.key)?;
        if self.metadata_digest != self.seal(layer)? {
            return Err("frozen alignment metadata digest differs".into());
        }
        Ok(())
    }

    pub(super) fn validate_reference(&self, resources: &[Resource]) -> Result<()> {
        let descriptor = &resources
            .get(self.reference)
            .ok_or("invalid frozen alignment resource index")?
            .descriptor;
        let interactive = &self.spec.choices.interactive;
        if interactive.source != descriptor.source
            || interactive.geometry != descriptor.geometry
            || interactive.model != descriptor.model
        {
            return Err("frozen alignment reference differs from the interactive request".into());
        }
        Ok(())
    }

    pub(super) fn capture(
        layer: &Layer,
        reference: usize,
        frozen: &crate::depth_snapshot::alignment::Frozen,
        resources: &[Owned],
        hashes: &mut Vec<Option<[u8; 32]>>,
        check: &dyn Fn() -> Result<()>,
    ) -> Result<Self> {
        let mut value = Self {
            reference,
            spec: frozen.spec().clone(),
            reference_digest: digest(reference, layer, resources, hashes, check)?,
            candidate_digest: digest(layer.resource, layer, resources, hashes, check)?,
            metadata_digest: [0; 32],
        };
        value.metadata_digest = value.seal(layer)?;
        Ok(value)
    }

    pub(super) fn restore(
        &self,
        layer: &Layer,
        resources: &[Owned],
        hashes: &mut Vec<Option<[u8; 32]>>,
        restored: &mut Restored,
        check: &dyn Fn() -> Result<()>,
    ) -> Result<crate::depth_snapshot::alignment::Frozen> {
        self.validate_metadata(layer)?;
        if self.reference_digest != digest(self.reference, layer, resources, hashes, check)?
            || self.candidate_digest != digest(layer.resource, layer, resources, hashes, check)?
        {
            return Err(format!("frozen alignment source_request={} object={} source_id={} phase=verify_planes: content digest differs", layer.request, layer.object, layer.asset.id).into());
        }
        let Owned::Memory(reference) = &resources[self.reference] else {
            return Err("frozen alignment reference was not retained".into());
        };
        let Owned::Memory(candidate) = &resources[layer.resource] else {
            return Err("frozen alignment candidate was not retained".into());
        };
        let policy = serde_json::to_vec(&self.spec)
            .map_err(|cause| crate::Error::with_context("encode frozen fit identity", cause))?;
        let identity = (
            self.reference,
            layer.resource,
            Sha256::digest(policy).into(),
        );
        if let Some(proof) = restored.proofs.get(&identity) {
            return Ok(proof.clone());
        }
        let proof = crate::depth_snapshot::alignment::Frozen::restore(
            reference.clone(),
            candidate,
            self.spec.clone(),
            &layer.key,
            layer.request,
            layer.object,
            check,
        )?;
        restored.proofs.insert(identity, proof.clone());
        Ok(proof)
    }
}

fn digest(
    index: usize,
    layer: &Layer,
    resources: &[Owned],
    hashes: &mut Vec<Option<[u8; 32]>>,
    check: &dyn Fn() -> Result<()>,
) -> Result<[u8; 32]> {
    check()?;
    hashes.resize(resources.len(), None);
    if let Some(hash) = hashes.get(index).and_then(|value| *value) {
        return Ok(hash);
    }
    let Some(Owned::Memory(estimate)) = resources.get(index) else {
        return Err("frozen alignment resource index or residency differs".into());
    };
    log::info!("start frozen_depth operation=hash_alignment source_request={} object={} source_id={} resource={} phase=hash_scalar_planes", layer.request, layer.object, layer.asset.id, index);
    log::logger().flush();
    let began = std::time::Instant::now();
    let resource = &estimate.resource;
    let mut hash = Sha256::new();
    hash.update(b"motion-frozen-scalar-v1\0");
    let metadata = serde_json::to_vec(&(
        resource.descriptor(),
        &estimate.provider,
        resource.validity().is_some(),
        resource.confidence().is_some(),
    ))
    .map_err(|cause| crate::Error::with_context("encode frozen scalar integrity", cause))?;
    hash.update((metadata.len() as u64).to_le_bytes());
    hash.update(metadata);
    fn floats(hash: &mut Sha256, values: &[f32], check: &dyn Fn() -> Result<()>) -> Result<()> {
        for chunk in values.chunks(CHUNK / 4) {
            check()?;
            for value in chunk {
                hash.update(value.to_le_bytes());
            }
        }
        Ok(())
    }
    floats(&mut hash, resource.values(), check)?;
    if let Some(mask) = resource.validity() {
        for chunk in mask.chunks(CHUNK) {
            check()?;
            hash.update(chunk);
        }
    }
    if let Some(scores) = resource.confidence() {
        floats(&mut hash, scores, check)?;
    }
    check()?;
    let value = hash.finalize().into();
    hashes[index] = Some(value);
    log::debug!("done frozen_depth operation=hash_alignment source_request={} object={} resource={} elapsed_ms={}", layer.request, layer.object, index, began.elapsed().as_millis());
    Ok(value)
}
