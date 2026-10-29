//! Bounded scalar-only persistence. No inference runtimes or GPU objects.
use super::*;
use std::io::{Read, Write};

const MAGIC: &[u8; 8] = b"MSDPTH3\0";
const MAX_METADATA: usize = 8 << 20;
#[cfg(feature = "ai-depth")]
const CHUNK: usize = 64 << 10;

impl DepthSnapshot {
    pub fn write_to(&self, writer: &mut dyn Write, check: &dyn Fn() -> Result<()>) -> Result<()> {
        check()?;
        #[cfg(feature = "ai-depth")]
        {
            self.validate()?;
            if !self.entries.is_empty() || !self.videos.is_empty() {
                return write_snapshot(self, writer, check);
            }
        }
        writer.write_all(MAGIC)?;
        writer.write_all(&0u32.to_le_bytes())?;
        check()?;
        Ok(())
    }

    pub fn read_from(reader: &mut dyn Read, check: &dyn Fn() -> Result<()>) -> Result<Self> {
        check()?;
        let mut header = [0u8; 12];
        reader.read_exact(&mut header)?;
        if &header[..8] != MAGIC {
            return Err("unsupported frozen depth snapshot version".into());
        }
        let size = u32::from_le_bytes(
            header[8..12]
                .try_into()
                .map_err(|_| "invalid depth header")?,
        ) as usize;
        if size > MAX_METADATA {
            return Err("frozen depth metadata exceeds 8 MiB".into());
        }
        if size == 0 {
            end(reader, check)?;
            return Ok(Self::default());
        }
        #[cfg(feature = "ai-depth")]
        {
            read_snapshot(reader, size, check)
        }
        #[cfg(not(feature = "ai-depth"))]
        {
            Err("frozen calibrated depth requires the ai-depth feature".into())
        }
    }
}

fn end(reader: &mut dyn Read, check: &dyn Fn() -> Result<()>) -> Result<()> {
    check()?;
    let mut extra = [0u8; 1];
    if reader.read(&mut extra)? != 0 {
        return Err("trailing bytes in frozen depth snapshot".into());
    }
    Ok(())
}

#[cfg(feature = "ai-depth")]
use motion_model::depth::{DepthDescriptor, DepthResource};
#[cfg(feature = "ai-depth")]
use serde::{Deserialize, Serialize};

#[cfg(feature = "ai-depth")]
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Metadata {
    composition: String,
    #[serde(deserialize_with = "bounded_entries")]
    entries: Vec<Layer>,
    #[serde(deserialize_with = "bounded_videos")]
    videos: Vec<VideoLayer>,
    #[serde(deserialize_with = "bounded_resources")]
    resources: Vec<Resource>,
}
#[cfg(feature = "ai-depth")]
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Layer {
    object: u64,
    asset: motion_core::Asset,
    key: Key,
    input: PreparedDepthInput,
    calibration: SavedDepthCalibration,
    resource: usize,
    request: u64,
    decoded_budget: u64,
}
#[cfg(feature = "ai-depth")]
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct VideoLayer {
    object: u64,
    asset: motion_core::VideoAsset,
    key: Key,
    input: PreparedDepthInput,
    sample: motion_model::depth::VideoDepthSample,
    resource: usize,
    request: u64,
    decoded_budget: u64,
}
#[cfg(feature = "ai-depth")]
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Resource {
    descriptor: DepthDescriptor,
    provider: String,
    validity: bool,
    confidence: bool,
}

#[cfg(feature = "ai-depth")]
fn bounded<'de, D: serde::Deserializer<'de>, T: Deserialize<'de>, const LIMIT: usize>(
    d: D,
) -> std::result::Result<Vec<T>, D::Error> {
    struct Visitor<T, const LIMIT: usize>(std::marker::PhantomData<T>);
    impl<'de, T: Deserialize<'de>, const LIMIT: usize> serde::de::Visitor<'de> for Visitor<T, LIMIT> {
        type Value = Vec<T>;
        fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
            f.write_str("bounded depth snapshot array")
        }
        fn visit_seq<A: serde::de::SeqAccess<'de>>(
            self,
            mut seq: A,
        ) -> std::result::Result<Self::Value, A::Error> {
            let mut values = Vec::new();
            while let Some(value) = seq.next_element()? {
                if values.len() >= LIMIT {
                    return Err(serde::de::Error::custom("frozen depth count exceeds limit"));
                }
                values.push(value);
            }
            Ok(values)
        }
    }
    d.deserialize_seq(Visitor::<T, LIMIT>(std::marker::PhantomData))
}

#[cfg(feature = "ai-depth")]
fn bounded_entries<'de, D: serde::Deserializer<'de>, T: Deserialize<'de>>(
    d: D,
) -> std::result::Result<Vec<T>, D::Error> {
    bounded::<D, T, { motion_model::MAX_LAYERS }>(d)
}
#[cfg(feature = "ai-depth")]
fn bounded_videos<'de, D: serde::Deserializer<'de>, T: Deserialize<'de>>(
    d: D,
) -> std::result::Result<Vec<T>, D::Error> {
    bounded::<D, T, { motion_model::depth::MAX_VIDEO_DEPTH_SAMPLES }>(d)
}
#[cfg(feature = "ai-depth")]
fn bounded_resources<'de, D: serde::Deserializer<'de>, T: Deserialize<'de>>(
    d: D,
) -> std::result::Result<Vec<T>, D::Error> {
    bounded::<D, T, { motion_model::MAX_LAYERS + motion_model::depth::MAX_VIDEO_DEPTH_SAMPLES }>(d)
}

#[cfg(feature = "ai-depth")]
fn write_snapshot(
    snapshot: &DepthSnapshot,
    writer: &mut dyn Write,
    check: &dyn Fn() -> Result<()>,
) -> Result<()> {
    let mut resources: Vec<Owned> = Vec::new();
    let mut layers = Vec::new();
    for entry in &snapshot.entries {
        let plane = Owned::Memory(entry.estimate.clone());
        let index = resources.iter().position(|r| r.same(&plane));
        let resource = index.unwrap_or_else(|| {
            resources.push(plane);
            resources.len() - 1
        });
        layers.push(Layer {
            object: entry.object,
            asset: entry.asset.clone(),
            key: entry.key.clone(),
            input: entry.input,
            calibration: entry.calibration.clone(),
            resource,
            request: entry.request,
            decoded_budget: entry.decoded_budget,
        });
    }
    let mut videos = Vec::new();
    for entry in &snapshot.videos {
        let resource = resources
            .iter()
            .position(|r| r.same(&entry.estimate))
            .unwrap_or_else(|| {
                resources.push(entry.estimate.clone());
                resources.len() - 1
            });
        videos.push(VideoLayer {
            object: entry.object,
            asset: entry.asset.clone(),
            key: entry.key.clone(),
            input: entry.input,
            sample: entry.sample.clone(),
            resource,
            request: entry.request,
            decoded_budget: entry.decoded_budget,
        });
    }
    let metadata = Metadata {
        composition: snapshot.composition.clone(),
        entries: layers,
        videos,
        resources: resources
            .iter()
            .map(|e| Resource {
                descriptor: e.descriptor().clone(),
                provider: e.provider().into(),
                validity: e.flags().0,
                confidence: e.flags().1,
            })
            .collect(),
    };
    struct Buffer(Vec<u8>);
    impl Write for Buffer {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            if bytes.len() > MAX_METADATA - self.0.len() {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "frozen depth metadata exceeds 8 MiB",
                ));
            }
            self.0.extend_from_slice(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut bytes = Buffer(Vec::new());
    serde_json::to_writer(&mut bytes, &metadata)
        .map_err(|e| crate::Error::with_context("serialize frozen depth metadata", e))?;
    writer.write_all(MAGIC)?;
    writer.write_all(&(bytes.0.len() as u32).to_le_bytes())?;
    for chunk in bytes.0.chunks(CHUNK) {
        check()?;
        writer.write_all(chunk)?;
    }
    for (index, plane) in resources.into_iter().enumerate() {
        check()?;
        if let Owned::File(plane) = plane {
            log::info!("start frozen_depth operation=write_snapshot resource={} source_id={} pts={:?} phase=archive_read",
                index, plane.descriptor.source.asset, plane.descriptor.source.pts);
            log::logger().flush();
            plane.write_to(writer, check)?;
            continue;
        }
        let Owned::Memory(estimate) = plane else {
            return Err("invalid frozen memory resource".into());
        };
        floats_out(writer, estimate.resource.values(), check)?;
        if let Some(mask) = estimate.resource.validity() {
            for chunk in mask.chunks(CHUNK) {
                check()?;
                writer.write_all(chunk)?;
            }
        }
        if let Some(scores) = estimate.resource.confidence() {
            floats_out(writer, scores, check)?;
        }
    }
    check()?;
    Ok(())
}

#[cfg(feature = "ai-depth")]
fn read_snapshot(
    reader: &mut dyn Read,
    size: usize,
    check: &dyn Fn() -> Result<()>,
) -> Result<DepthSnapshot> {
    let mut bytes = vec![0; size];
    for chunk in bytes.chunks_mut(CHUNK) {
        check()?;
        reader.read_exact(chunk)?;
    }
    let metadata: Metadata = serde_json::from_slice(&bytes)
        .map_err(|e| crate::Error::with_context("deserialize frozen depth metadata", e))?;
    let count = metadata.entries.len() + metadata.videos.len();
    if count == 0
        || metadata.entries.len() > motion_model::MAX_LAYERS
        || metadata.videos.len() > motion_model::depth::MAX_VIDEO_DEPTH_SAMPLES
        || metadata.resources.is_empty()
        || metadata.resources.len() > count
    {
        return Err("invalid frozen depth resource count".into());
    }
    // Validate indices and classify resident resources before any plane allocation.
    let mut used = vec![false; metadata.resources.len()];
    let mut resident = vec![false; metadata.resources.len()];
    for layer in &metadata.entries {
        *used
            .get_mut(layer.resource)
            .ok_or("invalid frozen depth resource index")? = true;
        resident[layer.resource] = true;
    }
    for layer in &metadata.videos {
        *used
            .get_mut(layer.resource)
            .ok_or("invalid frozen depth resource index")? = true;
    }
    if used.contains(&false) {
        return Err("unreferenced frozen depth resource".into());
    }
    let mut memory = 0u64;
    let mut disk = 0u64;
    for (index, resource) in metadata.resources.iter().enumerate() {
        let pixels = resource.descriptor.validate()?;
        let bytes = pixels
            .checked_mul(4 + usize::from(resource.validity) + 4 * usize::from(resource.confidence))
            .ok_or("frozen depth plane size overflow")?;
        if bytes > MAX_DEPTH_RESOURCE_BYTES {
            return Err("frozen depth plane exceeds 192 MiB".into());
        }
        let total = if resident[index] {
            &mut memory
        } else {
            &mut disk
        };
        *total = total
            .checked_add(bytes as u64)
            .ok_or("frozen depth plane total overflow")?;
        if memory > MAX_DEPTH_RESOURCE_BYTES as u64 {
            return Err("frozen depth retained planes exceed 192 MiB".into());
        }
        if disk > crate::depth::storage::MAX_STORED_BYTES {
            return Err("frozen depth file planes exceed 2 GiB".into());
        }
        if resource.provider.is_empty()
            || resource.provider.len() > 256
            || resource.provider.chars().any(char::is_control)
        {
            return Err("invalid bounded frozen depth provider".into());
        }
    }
    let mut resources = Vec::new();
    let mut spool = crate::depth::storage::Spool::default();
    for (index, resource) in metadata.resources.into_iter().enumerate() {
        let pixels = resource.descriptor.validate()?;
        let values = floats_in(reader, pixels, check)?;
        let validity = if resource.validity {
            let mut mask = vec![0; pixels];
            for chunk in mask.chunks_mut(CHUNK) {
                check()?;
                reader.read_exact(chunk)?;
            }
            Some(mask)
        } else {
            None
        };
        let confidence = if resource.confidence {
            Some(floats_in(reader, pixels, check)?)
        } else {
            None
        };
        let estimate = Arc::new(Estimate {
            resource: DepthResource::new(resource.descriptor, values, validity, confidence)?,
            provider: resource.provider,
            costs: crate::depth::Costs::default(),
        });
        if resident[index] {
            resources.push(Owned::Memory(estimate));
        } else {
            log::info!("start frozen_depth operation=read_snapshot resource={} source_id={} pts={:?} phase=archive_write",
                index, estimate.resource.descriptor().source.asset, estimate.resource.descriptor().source.pts);
            log::logger().flush();
            resources.push(Owned::File(spool.append_for_export(&estimate, check)?));
        }
    }
    end(reader, check)?;
    let snapshot = DepthSnapshot {
        composition: metadata.composition,
        entries: metadata
            .entries
            .into_iter()
            .map(|e| {
                let Owned::Memory(estimate) = &resources[e.resource] else {
                    return Err(crate::Error::from(
                        "frozen static resource was not retained",
                    ));
                };
                Ok(Entry {
                    object: e.object,
                    asset: e.asset,
                    key: e.key,
                    input: e.input,
                    calibration: e.calibration,
                    estimate: estimate.clone(),
                    request: e.request,
                    decoded_budget: e.decoded_budget,
                })
            })
            .collect::<Result<Vec<_>>>()?,
        videos: metadata
            .videos
            .into_iter()
            .map(|e| VideoEntry {
                object: e.object,
                asset: e.asset,
                key: e.key,
                input: e.input,
                sample: e.sample,
                estimate: resources[e.resource].clone(),
                request: e.request,
                decoded_budget: e.decoded_budget,
            })
            .collect(),
    };
    snapshot.validate()?;
    Ok(snapshot)
}

#[cfg(feature = "ai-depth")]
fn floats_out(
    writer: &mut dyn Write,
    values: &[f32],
    check: &dyn Fn() -> Result<()>,
) -> Result<()> {
    let mut bytes = [0u8; CHUNK];
    for chunk in values.chunks(CHUNK / 4) {
        check()?;
        for (value, out) in chunk.iter().zip(bytes.chunks_exact_mut(4)) {
            out.copy_from_slice(&value.to_le_bytes());
        }
        writer.write_all(&bytes[..chunk.len() * 4])?;
    }
    Ok(())
}

#[cfg(feature = "ai-depth")]
fn floats_in(
    reader: &mut dyn Read,
    count: usize,
    check: &dyn Fn() -> Result<()>,
) -> Result<Vec<f32>> {
    let mut values = vec![0.; count];
    let mut bytes = [0u8; CHUNK];
    for chunk in values.chunks_mut(CHUNK / 4) {
        check()?;
        reader.read_exact(&mut bytes[..chunk.len() * 4])?;
        for (value, input) in chunk.iter_mut().zip(bytes.chunks_exact(4)) {
            *value = f32::from_le_bytes([input[0], input[1], input[2], input[3]]);
        }
    }
    Ok(values)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn empty_wire_checks_version_truncation_trailing_data_and_primary_cancellation() {
        let mut bytes = Vec::new();
        DepthSnapshot::default()
            .write_to(&mut bytes, &|| Ok(()))
            .unwrap();
        assert_eq!(
            DepthSnapshot::read_from(&mut &bytes[..], &|| Ok(()))
                .unwrap()
                .layer_count(),
            0
        );
        for end in 0..bytes.len() {
            assert!(DepthSnapshot::read_from(&mut &bytes[..end], &|| Ok(())).is_err());
        }
        bytes.push(1);
        assert!(DepthSnapshot::read_from(&mut &bytes[..], &|| Ok(()))
            .unwrap_err()
            .to_string()
            .contains("trailing"));
        bytes[0] = b'!';
        assert!(DepthSnapshot::read_from(&mut &bytes[..], &|| Ok(()))
            .unwrap_err()
            .to_string()
            .contains("version"));
        let cause = DepthSnapshot::read_from(&mut &[][..], &|| Err("primary cancellation".into()))
            .unwrap_err();
        assert_eq!(cause.to_string(), "primary cancellation");
    }

    #[cfg(feature = "ai-depth")]
    fn fixture() -> DepthSnapshot {
        use motion_model::depth::*;
        let descriptor = DepthDescriptor {
            size: [2, 2],
            source: DepthSource {
                asset: 7,
                revision: 3,
                pts: DepthTime {
                    ticks: 0,
                    timescale: 1,
                },
            },
            geometry: DepthGeometry {
                source_size: [2, 2],
                crop: [0, 0, 2, 2],
                quarter_turns: 0,
                mirror_x: false,
            },
            model: DepthModel {
                id: "owned-test".into(),
                revision: "1".into(),
                processing: "rgb-test".into(),
                sha256: [7; 32],
            },
            measure: DepthMeasure::Relative {
                near: NearDirection::Increasing,
            },
            scale: 1.,
            offset: 0.,
            valid_range: None,
        };
        let key = Key {
            source: descriptor.source.clone(),
            geometry: descriptor.geometry.clone(),
            model: descriptor.model.clone(),
        };
        let resource = DepthResource::new(
            descriptor,
            vec![10., f32::from_bits(0x7fc00001), 90., 50.],
            Some(vec![1, 0, 1, 1]),
            Some(vec![1., 0., 0.8, 0.5]),
        )
        .unwrap();
        let input = PreparedDepthInput {
            rgb_size: [2, 2],
            rgb_sha256: [9; 32],
        };
        let calibration = SavedDepthCalibration::new(
            &resource,
            input,
            [
                DepthAnchor {
                    value: 10.,
                    camera_z_meters: 8.,
                },
                DepthAnchor {
                    value: 90.,
                    camera_z_meters: 4.,
                },
            ],
            CalibrationCurve::CameraZ,
            CalibrationExtrapolation::Reject,
            20.,
        )
        .unwrap();
        let entry = Entry {
            object: 1,
            asset: motion_core::Asset {
                id: 7,
                path: "assets/owned.png".into(),
                width: 2,
                height: 2,
            },
            key,
            input,
            calibration,
            estimate: Arc::new(Estimate {
                resource,
                provider: "controlled_cpu".into(),
                costs: Default::default(),
            }),
            request: 42,
            decoded_budget: 1 << 20,
        };
        let mut alias = entry.clone();
        alias.object = 2;
        DepthSnapshot {
            composition: "comp-main".into(),
            entries: vec![entry, alias],
            videos: Vec::new(),
        }
    }

    #[test]
    #[cfg(feature = "ai-depth")]
    fn scalar_wire_roundtrip_preserves_masked_nan_confidence_and_shared_ownership() {
        let original = fixture();
        let mut bytes = Vec::new();
        original.write_to(&mut bytes, &|| Ok(())).unwrap();
        let restored = DepthSnapshot::read_from(&mut &bytes[..], &|| Ok(())).unwrap();
        assert_eq!(restored.layer_count(), 2);
        assert!(Arc::ptr_eq(
            &restored.entries[0].estimate,
            &restored.entries[1].estimate
        ));
        let a = &original.entries[0].estimate.resource;
        let b = &restored.entries[0].estimate.resource;
        assert_ne!(a.cache_identity(), b.cache_identity());
        assert_eq!(a.descriptor(), b.descriptor());
        assert_eq!(a.validity(), b.validity());
        assert_eq!(a.confidence(), b.confidence());
        assert_eq!(
            a.values().iter().map(|v| v.to_bits()).collect::<Vec<_>>(),
            b.values().iter().map(|v| v.to_bits()).collect::<Vec<_>>()
        );
        assert_eq!(restored.entries[0].request, 42);
        assert_eq!(restored.entries[0].input, original.entries[0].input);
        let mut repeated = Vec::new();
        restored.write_to(&mut repeated, &|| Ok(())).unwrap();
        assert_eq!(bytes, repeated);
    }

    #[cfg(feature = "ai-depth")]
    fn temporal_fixture() -> (DepthSnapshot, Entry) {
        use motion_model::depth::*;
        let mut snapshot = fixture();
        let old = snapshot.entries.remove(0);
        snapshot.entries.clear();
        let mut descriptor = old.estimate.resource.descriptor().clone();
        descriptor.source.pts.timescale = 1_000_000;
        let resource =
            DepthResource::new(descriptor.clone(), vec![10., 30., 90., 50.], None, None).unwrap();
        let calibration = SavedDepthCalibration::new(
            &resource,
            old.input,
            [
                DepthAnchor {
                    value: 10.,
                    camera_z_meters: 8.,
                },
                DepthAnchor {
                    value: 90.,
                    camera_z_meters: 4.,
                },
            ],
            CalibrationCurve::CameraZ,
            CalibrationExtrapolation::Reject,
            20.,
        )
        .unwrap();
        snapshot.videos.push(VideoEntry {
            object: 1,
            asset: motion_core::VideoAsset {
                id: 7,
                path: "assets/source.mp4".into(),
                bytes: 100,
                mime: "video/avc".into(),
                track: 0,
                width: 2,
                height: 2,
                rotation: 0,
                display_width: 2,
                display_height: 2,
                video_start_us: 0,
                video_end_us: 100_000,
                duration_us: 100_000,
                frame_count: 3,
                variable_frame_rate: true,
                nominal_frame_rate: 30.,
                color_standard: 1,
                color_range: 1,
                audio_asset: None,
            },
            key: Key {
                source: descriptor.source,
                geometry: descriptor.geometry,
                model: descriptor.model,
            },
            input: old.input,
            sample: VideoDepthSample::new(calibration, 33_333).unwrap(),
            estimate: Owned::Memory(Arc::new(Estimate {
                resource,
                provider: "owned-wire-test".into(),
                costs: Default::default(),
            })),
            request: 42,
            decoded_budget: 1 << 20,
        });
        (snapshot, old)
    }

    #[test]
    #[cfg(feature = "ai-depth")]
    fn temporal_wire_rejects_duplicate_intervals_bad_indices_and_mixed_object_ownership() {
        let (mut snapshot, old) = temporal_fixture();
        let mut bytes = Vec::new();
        snapshot.write_to(&mut bytes, &|| Ok(())).unwrap();
        let restored = DepthSnapshot::read_from(&mut &bytes[..], &|| Ok(())).unwrap();
        assert_eq!(restored.layer_count(), 1);
        assert_eq!(restored.videos.len(), 1);
        assert_eq!(restored.videos[0].sample, snapshot.videos[0].sample);
        let mut repeated = Vec::new();
        restored.write_to(&mut repeated, &|| Ok(())).unwrap();
        assert_eq!(bytes, repeated);
        let size = u32::from_le_bytes(bytes[8..12].try_into().unwrap()) as usize;
        let metadata: serde_json::Value = serde_json::from_slice(&bytes[12..12 + size]).unwrap();
        for mutation in 0..4 {
            let mut value = metadata.clone();
            match mutation {
                0 => {
                    let duplicate = value["videos"][0].clone();
                    value["videos"].as_array_mut().unwrap().push(duplicate);
                }
                1 => value["videos"][0]["resource"] = serde_json::json!(99),
                2 => value["videos"][0]["sample"]["end_us"] = serde_json::json!(0),
                _ => {
                    value["videos"] = serde_json::json!(vec![
                        value["videos"][0].clone();
                        motion_model::MAX_LAYERS + 1
                    ])
                }
            }
            let data = serde_json::to_vec(&value).unwrap();
            let mut malformed = MAGIC.to_vec();
            malformed.extend_from_slice(&(data.len() as u32).to_le_bytes());
            malformed.extend(data);
            malformed.extend_from_slice(&bytes[12 + size..]);
            assert!(
                DepthSnapshot::read_from(&mut &malformed[..], &|| Ok(())).is_err(),
                "mutation={mutation}"
            );
        }
        snapshot.entries.push(old);
        assert!(snapshot
            .validate()
            .unwrap_err()
            .to_string()
            .contains("video depth source metadata"));
    }

    #[test]
    #[cfg(feature = "ai-depth")]
    fn temporal_wire_restores_150_planes_as_files_and_streams_identical_bytes() {
        use motion_model::depth::*;
        let (mut snapshot, _) = temporal_fixture();
        let mut base = snapshot.videos.pop().unwrap();
        base.asset.frame_count = 150;
        base.asset.video_end_us = 5_000_000;
        base.asset.duration_us = 5_000_000;
        for i in 0..150 {
            let mut descriptor = base.estimate.descriptor().clone();
            descriptor.source.pts.ticks = i * 33_333;
            let resource =
                DepthResource::new(descriptor.clone(), vec![10., 30., 90., 50.], None, None)
                    .unwrap();
            let calibration = SavedDepthCalibration::new(
                &resource,
                base.input,
                [
                    DepthAnchor {
                        value: 10.,
                        camera_z_meters: 8.,
                    },
                    DepthAnchor {
                        value: 90.,
                        camera_z_meters: 4.,
                    },
                ],
                CalibrationCurve::CameraZ,
                CalibrationExtrapolation::Reject,
                20.,
            )
            .unwrap();
            let mut entry = base.clone();
            entry.key.source = descriptor.source;
            entry.sample = VideoDepthSample::new(calibration, ((i + 1) * 33_333) as u64).unwrap();
            entry.estimate = Owned::Memory(Arc::new(Estimate {
                resource,
                provider: "wire-stream-test".into(),
                costs: Default::default(),
            }));
            snapshot.videos.push(entry);
        }
        let mut bytes = Vec::new();
        snapshot.write_to(&mut bytes, &|| Ok(())).unwrap();
        let restored = DepthSnapshot::read_from(&mut &bytes[..], &|| Ok(())).unwrap();
        assert_eq!((restored.videos.len(), restored.layer_count()), (150, 1));
        assert!(restored
            .videos
            .iter()
            .all(|e| matches!(e.estimate, Owned::File(_))));
        let owned = restored.clone();
        drop(restored);
        drop(snapshot);
        for (i, entry) in owned.videos.iter().enumerate() {
            let actual = entry.estimate.load(&|| Ok(())).unwrap();
            assert_eq!(actual.resource.values(), &[10., 30., 90., 50.]);
            assert_eq!(
                actual.resource.descriptor().source.pts.ticks,
                i as i64 * 33_333
            );
        }
        let mut repeated = Vec::new();
        owned.write_to(&mut repeated, &|| Ok(())).unwrap();
        assert_eq!(repeated, bytes);
    }

    #[test]
    #[cfg(feature = "ai-depth")]
    fn temporal_wire_preflights_sample_resource_and_disk_limits_without_plane_payloads() {
        let (snapshot, _) = temporal_fixture();
        let mut bytes = Vec::new();
        snapshot.write_to(&mut bytes, &|| Ok(())).unwrap();
        let size = u32::from_le_bytes(bytes[8..12].try_into().unwrap()) as usize;
        let metadata: serde_json::Value = serde_json::from_slice(&bytes[12..12 + size]).unwrap();
        let mut value = metadata.clone();
        value["videos"] = serde_json::json!(vec![
            metadata["videos"][0].clone();
            motion_model::depth::MAX_VIDEO_DEPTH_SAMPLES + 1
        ]);
        assert!(serde_json::from_value::<Metadata>(value)
            .err()
            .unwrap()
            .to_string()
            .contains("count exceeds"));
        let mut value = metadata.clone();
        value["resources"] = serde_json::json!(vec![
            metadata["resources"][0].clone();
            motion_model::MAX_LAYERS + motion_model::depth::MAX_VIDEO_DEPTH_SAMPLES
                + 1
        ]);
        assert!(serde_json::from_value::<Metadata>(value)
            .err()
            .unwrap()
            .to_string()
            .contains("count exceeds"));
        let mut value = metadata;
        value["resources"][0]["descriptor"]["size"] = serde_json::json!([4096, 4096]);
        value["resources"] = serde_json::json!(vec![value["resources"][0].clone(); 33]);
        value["videos"] = serde_json::json!(vec![value["videos"][0].clone(); 33]);
        for i in 0..33 {
            value["videos"][i]["resource"] = serde_json::json!(i);
        }
        let payload = serde_json::to_vec(&value).unwrap();
        let mut wire = MAGIC.to_vec();
        wire.extend_from_slice(&(payload.len() as u32).to_le_bytes());
        wire.extend_from_slice(&payload);
        assert!(DepthSnapshot::read_from(&mut &wire[..], &|| Ok(()))
            .unwrap_err()
            .to_string()
            .contains("2 GiB"));
    }

    #[test]
    #[cfg(feature = "ai-depth")]
    fn aliased_source_keys_still_verify_each_distinct_expected_rgb_fingerprint() {
        use motion_model::depth::*;
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir(root.path().join("assets")).unwrap();
        image::RgbaImage::from_pixel(2, 2, image::Rgba([10, 2, 3, 255]))
            .save(root.path().join("assets/owned.png"))
            .unwrap();
        let mut snapshot = fixture();
        let actual = ImageInput {
            key: snapshot.entries[0].key.clone(),
            root: root.path().into(),
            relative: "assets/owned.png".into(),
            budget: 1 << 20,
        }
        .decode(&Cancellation::default())
        .unwrap()
        .fingerprint(&Cancellation::default())
        .unwrap();
        for entry in &mut snapshot.entries {
            entry.input = actual;
            if entry.object == 2 {
                entry.input.rgb_sha256[0] ^= 1;
            }
            entry.calibration = SavedDepthCalibration::new(
                &entry.estimate.resource,
                entry.input,
                [
                    DepthAnchor {
                        value: 10.,
                        camera_z_meters: 8.,
                    },
                    DepthAnchor {
                        value: 90.,
                        camera_z_meters: 4.,
                    },
                ],
                CalibrationCurve::CameraZ,
                CalibrationExtrapolation::Reject,
                20.,
            )
            .unwrap();
        }
        snapshot.validate().unwrap();
        let cause = snapshot
            .verify_sources(root.path(), &|| Ok(()))
            .err()
            .expect("same source key does not authorize a different expected RGB fingerprint")
            .to_string();
        assert!(
            cause.contains("object=2") && cause.contains("fingerprint changed"),
            "{cause}"
        );
    }

    #[test]
    #[cfg(feature = "ai-depth")]
    fn malformed_wire_rejects_unbounded_metadata_counts_planes_and_bad_references() {
        let original = fixture();
        let mut bytes = Vec::new();
        original.write_to(&mut bytes, &|| Ok(())).unwrap();
        let size = u32::from_le_bytes(bytes[8..12].try_into().unwrap()) as usize;
        let metadata: serde_json::Value = serde_json::from_slice(&bytes[12..12 + size]).unwrap();
        let mutated = |value: &serde_json::Value| {
            let m = serde_json::to_vec(value).unwrap();
            let mut v = MAGIC.to_vec();
            v.extend_from_slice(&(m.len() as u32).to_le_bytes());
            v.extend_from_slice(&m);
            v
        };
        let mut huge = MAGIC.to_vec();
        huge.extend_from_slice(&((MAX_METADATA + 1) as u32).to_le_bytes());
        assert!(DepthSnapshot::read_from(&mut &huge[..], &|| Ok(()))
            .unwrap_err()
            .to_string()
            .contains("8 MiB"));
        let mut value = metadata.clone();
        value["entries"] = serde_json::Value::Array(vec![
            metadata["entries"][0].clone();
            motion_model::MAX_LAYERS + 1
        ]);
        assert!(
            DepthSnapshot::read_from(&mut &mutated(&value)[..], &|| Ok(()))
                .unwrap_err()
                .to_string()
                .contains("count exceeds")
        );
        let mut value = metadata.clone();
        value["resources"][0]["descriptor"]["size"] = serde_json::json!([4096, 4096]);
        let r = value["resources"][0].clone();
        value["resources"].as_array_mut().unwrap().push(r);
        value["entries"][1]["resource"] = serde_json::json!(1);
        assert!(
            DepthSnapshot::read_from(&mut &mutated(&value)[..], &|| Ok(()))
                .unwrap_err()
                .to_string()
                .contains("192 MiB")
        );
        let mut value = metadata.clone();
        value["entries"][0]["resource"] = serde_json::json!(99);
        assert!(
            DepthSnapshot::read_from(&mut &mutated(&value)[..], &|| Ok(()))
                .unwrap_err()
                .to_string()
                .contains("resource index")
        );
        let mut bad = bytes.clone();
        *bad.last_mut().unwrap() = 0xff;
        assert!(DepthSnapshot::read_from(&mut &bad[..], &|| Ok(())).is_err());
        let count = std::cell::Cell::new(0);
        let error = DepthSnapshot::read_from(&mut &bytes[..], &|| {
            count.set(count.get() + 1);
            if count.get() >= 3 {
                Err("primary plane cancellation".into())
            } else {
                Ok(())
            }
        })
        .unwrap_err();
        assert_eq!(error.to_string(), "primary plane cancellation");
    }
}
