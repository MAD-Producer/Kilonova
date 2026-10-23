use super::*;
use motion_core::{Content, Layer, LayerTimeline, Project, VideoClip};
use motion_host::Platform;
use std::io::{Read, Seek, SeekFrom};

pub(super) fn validate_manifest(manifest: &Value) -> Result<()> {
    require(manifest["schemaVersion"] == 1 && manifest["dimensions"] == json!([3840,2160])
        && manifest["fps"] == FPS && manifest["frames"] == FRAMES,
        "fixture manifest does not match the fixed native study")?;
    let sources = manifest["sources"].as_array().ok_or("missing sources")?;
    require(sources.len() == 4, "four independent files required")?;
    let mut identities = std::collections::HashSet::new();
    for (index, source) in sources.iter().enumerate() {
        require(source["id"] == index + 1
            && source["path"] == format!("source-{}.mp4", index + 1),
            "fixture identity/path mismatch")?;
        let hash = source["sha256"].as_str().ok_or("missing source hash")?;
        require(hash.len() == 64 && hash.bytes().all(|v| v.is_ascii_hexdigit())
            && identities.insert(hash), "source hashes must be distinct SHA-256 identities")?;
    }
    Ok(())
}

pub(super) fn create(fixtures: &Path, manifest: &Value, count: usize,
                      evidence: &mut Evidence) -> Result<PathBuf> {
    require(matches!(count, 1 | 4), "unsupported workload source count")?;
    let directory = evidence.root.join("project");
    std::fs::create_dir(&directory)?;
    std::fs::create_dir(directory.join("assets"))?;
    let mut project = Project::new(COMPOSITION[0], COMPOSITION[1], FPS, FRAMES)?;
    // A missing first source must not accidentally match its black reference.
    // All real video layers are opaque and cover their full authored cells.
    project.background = [1., 0., 1., 1.];
    let platform = motion_desktop_media::platform();
    let mut probes = Vec::new();
    let cells = if count == 1 { 1. } else { 2. };
    for index in 0..count {
        let id = index as u64 + 1;
        let filename = manifest["sources"][index]["path"].as_str().ok_or("missing fixture path")?;
        let relative = format!("assets/{filename}");
        let path = directory.join(&relative);
        evidence.begin("source_copy", json!({"source":id,"path":filename}))?;
        let copied = std::fs::copy(fixtures.join(filename), &path)?;
        require(manifest["sources"][index]["bytes"] == copied, "actual file copy length changed")?;
        evidence.begin("native_probe", json!({"source":id,"path":relative}))?;
        let mut probe = platform.probe_video(&path, None, Some(u32::MAX), &|| Ok(()))?;
        require(probe.asset.width == 3840 && probe.asset.height == 2160
            && probe.asset.frame_count == FRAMES && probe.asset.mime == "video/avc"
            && (probe.asset.nominal_frame_rate - f64::from(FPS)).abs() < 0.001
            && probe.timestamps.len() == FRAMES as usize,
            "actual application probe differs from the encoded 4K600frame source")?;
        require(probe.timestamps.windows(2).all(|p| p[1] > p[0]), "native PTS index did not advance")?;
        probe.asset.id = id;
        probe.asset.path = relative;
        let cache = motion_media::video_cache_path(&directory, &probe.asset)?;
        std::fs::create_dir_all(cache.parent().ok_or("missing video index directory")?)?;
        motion_media::save_index(&cache, &probe.asset, &probe.timestamps)?;
        probes.push(json!({"asset":probe.asset,"timestampsUs":probe.timestamps}));
        project.video_assets.push(probe.asset);
        let cell = [COMPOSITION[0] as f32 / cells, COMPOSITION[1] as f32 / cells];
        let position = if count == 1 { [cell[0] * 0.5, cell[1] * 0.5, 0.] }
            else { [cell[0] * (index % 2) as f32 + cell[0] * 0.5,
                    cell[1] * (index / 2) as f32 + cell[1] * 0.5, 0.] };
        let mut layer = Layer::solid(id, &format!("Independent video {id}"),
            [3840.,2160.], position, [1.;4]);
        layer.content = Content::Video { video: VideoClip::new(id) };
        layer.timeline = Some(LayerTimeline::full(FRAMES));
        layer.transform.scale.value = [100. * cell[0] / 3840., 100. * cell[1] / 2160., 100.];
        project.layers.push(layer);
    }
    project.validate()?;
    motion_core::storage::save(&directory, &project)?;
    write_json(&evidence.root.join("native-probes.json"), &json!(probes))?;
    write_json(&evidence.root.join("authored-project.json"), &json!(project))?;
    Ok(directory)
}

fn reference_pixel(fixtures: &Path, source: &Value) -> Result<[u8;4]> {
    let samples = source["independentDecode"].as_array().ok_or("missing native decoded references")?;
    let mut pixels = Vec::new();
    for sample in samples {
        let name = sample["path"].as_str().ok_or("missing reference pixel file")?;
        require(Path::new(name).components().count() == 1, "reference must be a fixture basename")?;
        let mut file = File::open(fixtures.join(name))?;
        require(file.metadata()?.len() == 3840 * 2160 * 4, "reference is not full-resolution RGBA")?;
        file.seek(SeekFrom::Start((1080 * 3840 + 1920) * 4))?;
        let mut pixel = [0;4];
        file.read_exact(&mut pixel)?;
        pixels.push(pixel);
    }
    require(pixels.len() == 3 && pixels.iter().all(|p| *p == pixels[0]),
        "authored rotation changed the reference center; do not compare a different source time")?;
    Ok(pixels[0])
}

pub(super) fn verify_pixels(fixtures: &Path, manifest: &Value, count: usize,
                            image_rect: [f32;4], viewport: [f32;4], evidence: &Evidence) -> Result<Value> {
    let image = image::open(evidence.root.join("window.png"))?.to_rgba8();
    require(image.dimensions() == (WINDOW[0],WINDOW[1]), "native screenshot dimensions changed")?;
    let mut samples = Vec::new();
    let mut all_match = true;
    for index in 0..count {
        let uv = if count == 1 { [0.5,0.5] }
            else { [0.25 + (index % 2) as f32 * 0.5, 0.25 + (index / 2) as f32 * 0.5] };
        let screen = [image_rect[0]+image_rect[2]*uv[0], image_rect[1]+image_rect[3]*uv[1]];
        require(screen[0] >= viewport[0] && screen[1] >= viewport[1]
            && screen[0] < viewport[0]+viewport[2] && screen[1] < viewport[1]+viewport[3]
            && screen[0] >= 0. && screen[0] < WINDOW[0] as f32
            && screen[1] >= 0. && screen[1] < WINDOW[1] as f32,
            "reference center is clipped from the actual window")?;
        let screen = [screen[0] as u32,screen[1] as u32];
        let actual = image.get_pixel(screen[0],screen[1]).0;
        let expected = reference_pixel(fixtures, &manifest["sources"][index])?;
        let matches = (0..3).all(|c| actual[c].abs_diff(expected[c]) <= 8) && actual[3] == 255;
        all_match &= matches;
        samples.push(json!({"source":index+1,"screen":screen,"actualRgba":actual,
            "referenceRgba":expected,"rgbTolerance":8,"matches":matches}));
    }
    let proof = json!({"scope":"Actual native UI capture; each video center compared with independently decoded file RGBA. Authored centers are stable at all three retained reference times.",
        "imageRect":image_rect,"viewport":viewport,"matches":all_match,"samples":samples});
    write_json(&evidence.root.join("pixel-check.json"), &proof)?;
    require(all_match, "native window video colors differ from actual independent file pixels")?;
    Ok(proof)
}

#[test]
fn fixed_study_rejects_duplicate_or_non_native_source_manifests() {
    let mut value = json!({"schemaVersion":1,"dimensions":[3840,2160],"fps":30,"frames":600,
        "sources":(1..=4).map(|id|json!({"id":id,"path":format!("source-{id}.mp4"),
            "sha256":format!("{id:064x}")})).collect::<Vec<_>>()});
    assert!(validate_manifest(&value).is_ok());
    value["sources"][3]["sha256"] = value["sources"][0]["sha256"].clone();
    assert!(validate_manifest(&value).is_err());
    value["sources"][3]["sha256"] = json!(format!("{:064x}",4));
    value["dimensions"] = json!([640,360]);
    assert!(validate_manifest(&value).is_err());
}
