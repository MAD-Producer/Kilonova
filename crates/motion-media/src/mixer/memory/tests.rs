use super::*;
use motion_model::{AudioClip, Content, Layer, LayerTimeline};

fn fixture(root: &Path, rates: &[u32]) -> Project {
    let mut project = Project::new(64, 64, 30, 3).unwrap();
    for (index, rate) in rates.iter().enumerate() {
        let id = index as u64 + 1;
        let asset = AudioAsset {
            id,
            path: format!("assets/memory-{id}.wav"),
            mime: "audio/wav".into(),
            bytes: 8192,
            track: 0,
            sample_rate: *rate,
            channels: 1,
            sample_frames: 2048,
            duration_us: 2048 * 1_000_000 / u64::from(*rate),
        };
        let cache = cache_path(root, &asset).unwrap();
        std::fs::create_dir_all(cache.parent().unwrap()).unwrap();
        std::fs::write(cache, 0.125f32.to_le_bytes().repeat(2048)).unwrap();
        project.audio_assets.push(asset);
        let mut layer = Layer::solid(id, "Original PCM fixture", [0.; 2], [0.; 3], [1.; 4]);
        layer.content = Content::Audio {
            audio: AudioClip::new(id),
        };
        layer.timeline = Some(LayerTimeline {
            in_frame: 0,
            out_frame: 1,
            offset_frame: 0,
        });
        project.layers.push(layer);
    }
    project.validate().unwrap();
    project
}

#[test]
fn real_mixer_shared_tables_count_once_and_distinct_rates_keep_their_own_capacity() {
    let root = tempfile::tempdir().unwrap();
    let project = fixture(root.path(), &[48000, 48000, 44100]);
    let mixer = AudioMixer::new(project, root.path()).unwrap();
    assert!(Arc::ptr_eq(
        &mixer.sources[0].weights,
        &mixer.sources[1].weights
    ));
    assert!(!Arc::ptr_eq(
        &mixer.sources[0].weights,
        &mixer.sources[2].weights
    ));
    let strong = Arc::strong_count(&mixer.sources[0].weights);
    let info = mixer.memory_info();
    let tables: Vec<_> = info
        .resources
        .iter()
        .filter(|r| r.category == "audio_sinc_table")
        .collect();
    assert_eq!(tables.len(), 2);
    assert_eq!(tables[0].references.len(), 2);
    assert_eq!(tables[1].references.len(), 1);
    assert_eq!(info.details["uniqueResamplingTables"], 2);
    let bytes = std::mem::size_of::<[f32; 64]>() as u64;
    assert_eq!(
        tables[0].payload_bytes,
        mixer.sources[0].weights.len() as u64 * bytes
    );
    assert_eq!(
        tables[0].capacity_bytes,
        mixer.sources[0].weights.capacity() as u64 * bytes
    );
    assert_eq!(
        info.known_capacity_bytes,
        (mixer.bytes.capacity() + mixer.samples.capacity() * 4) as u64
            + tables.iter().map(|r| r.capacity_bytes).sum::<u64>()
    );
    assert_eq!(Arc::strong_count(&mixer.sources[0].weights), strong);
    assert!(info.details["fileBackedPcmResidentBytes"].is_null());
}

#[test]
fn explicit_mixer_observation_preserves_exact_pcm_and_independent_file_cursors() {
    let root = tempfile::tempdir().unwrap();
    let project = fixture(root.path(), &[48000, 44100]);
    let mut observed = AudioMixer::new(project.clone(), root.path()).unwrap();
    let mut control = AudioMixer::new(project, root.path()).unwrap();
    let mut actual = vec![f32::NAN; 256];
    let mut expected = vec![f32::NAN; 256];
    for start in [0, 128, 256, 32] {
        let info = observed.memory_info();
        assert!(info.known_capacity_bytes > info.known_payload_bytes);
        assert_eq!(
            observed.mix(start, &mut actual).unwrap(),
            control.mix(start, &mut expected).unwrap()
        );
        assert_eq!(
            actual, expected,
            "diagnostic query must not alter samples or file positions"
        );
    }
}

#[test]
fn scratch_lengths_can_clear_while_real_capacity_remains_and_empty_mixer_is_partial() {
    let root = tempfile::tempdir().unwrap();
    let mut mixer = AudioMixer::new(fixture(root.path(), &[]), root.path()).unwrap();
    let initial = mixer.memory_info();
    assert_eq!(initial.known_payload_bytes, 0);
    assert!(initial.known_capacity_bytes > 0);
    mixer.bytes.extend([1, 2, 3, 4]);
    mixer.samples.extend([0.5, 0.25]);
    assert_eq!(mixer.memory_info().known_payload_bytes, 12);
    mixer.bytes.clear();
    mixer.samples.clear();
    assert_eq!(
        mixer.memory_info().known_capacity_bytes,
        initial.known_capacity_bytes
    );
    mixer.bytes = Vec::new();
    mixer.samples = Vec::new();
    let released = mixer.memory_info();
    assert_eq!(released.known_capacity_bytes, 0);
    assert!(!released.complete_coverage);
    assert!(released.details["unknownBytes"].is_null());
}
