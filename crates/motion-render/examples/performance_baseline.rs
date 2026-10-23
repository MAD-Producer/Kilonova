//! Explicit offline GPU study; never runs in the normal application/test loop.
//! Each sample waits for GPU completion. It is latency, not display FPS.
use motion_core::{
    compositing::{MatteMode, TrackMatte},
    Composition, CompositionClip, Content, Ease, EffectInstance, Layer, Project, VideoSample,
};
use motion_render::{ChromaLayout, GpuTimer, Presenter, Renderer, Scene, Yuv420Frame};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{fs::File, io::Write, path::Path, time::Instant};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
const SIZE: [u32; 2] = [1280, 720];
const VIDEO_SIZE: [u32; 2] = [3840, 2160];
const WARMUP: u32 = 8;

fn memory_policy() -> motion_render::resource_policy::MemoryPolicy {
    motion_render::resource_policy::MemoryPolicy {
        // Four full-size RGBA outputs plus their YUV planes exceed the normal
        // 128 MiB source allowance. This is an explicit study condition only.
        source_texture_budget_bytes: 256 << 20,
        ..Default::default()
    }
}
const CASES: [&str; 6] = [
    "static",
    "animated",
    "effect-chain",
    "crossing-3d",
    "nested-matte",
    "four-4k-yuv",
];

fn fixture(name: &str, samples: u32) -> Result<Project> {
    let mut p = Project::new(SIZE[0], SIZE[1], 30, WARMUP + samples)?;
    p.background = [0.02, 0.03, 0.05, 1.];
    for id in 1..=4 {
        let mut layer = Layer::solid(
            id,
            "Independently authored benchmark fixture",
            [420., 280.],
            [380. + id as f32 * 100., 250. + id as f32 * 40., 0.],
            [id as f32 / 5., 0.4, 0.7, 0.7],
        );
        if name == "static" && id % 2 == 0 {
            layer.content = Content::Vector {
                vector: motion_core::vector::VectorContent::shape(
                    motion_core::vector::ShapeKind::Ellipse,
                ),
            };
        }
        if matches!(name, "animated" | "crossing-3d") {
            let start = layer.transform.position.value;
            layer.transform.position.upsert(0, start, Ease::Linear)?;
            layer.transform.position.upsert(
                (WARMUP + samples - 1) as i32,
                [1000. - start[0] / 2., start[1], start[2]],
                Ease::Linear,
            )?;
        }
        if name == "crossing-3d" {
            layer.three_d = true;
            layer.transform.rotation.value = [15., if id % 2 == 0 { 50. } else { -50. }, 0.];
        }
        if name == "four-4k-yuv" {
            layer.size = [VIDEO_SIZE[0] as f32, VIDEO_SIZE[1] as f32];
            layer.transform.scale.value = [25., 25., 100.];
        }
        p.layers.push(layer);
    }
    if name == "effect-chain" {
        let package = motion_effects::builtin::package()?;
        for (index, id) in ["glow", "gaussian_blur", "invert", "directional_blur"]
            .repeat(2)
            .iter()
            .enumerate()
        {
            let definition = package
                .manifest
                .effects
                .iter()
                .find(|d| d.id == *id)
                .ok_or_else(|| format!("missing benchmark effect {id}"))?;
            let mut effect = EffectInstance::new(
                index as u64 + 1,
                &package.manifest.id,
                &package.manifest.version,
                &package.hash,
                definition,
                p.layers[0].size,
            );
            let parameter = match *id {
                "gaussian_blur" => Some(("p0001", 8.)),
                "directional_blur" => Some(("p0002", 32.)),
                _ => None,
            };
            if let Some((key, value)) = parameter {
                effect
                    .params
                    .get_mut(key)
                    .ok_or_else(|| format!("missing benchmark parameter {key}"))?
                    .track
                    .value[0] = value;
            }
            p.layers[0].effects.push(effect);
        }
    }
    if name == "nested-matte" {
        p.layers[1].track_matte = Some(TrackMatte {
            source: 1,
            mode: MatteMode::Alpha,
            hide_source: true,
        });
        let layers = std::mem::take(&mut p.layers);
        p.compositions.push(Composition {
            id: "comp-benchmark-child".into(),
            name: "Nested benchmark".into(),
            width: p.width,
            height: p.height,
            fps: p.fps,
            frames: p.frames,
            background: [0.; 4],
            camera: p.camera.clone(),
            layers,
            expressions: Vec::new(),
        });
        let mut reference = Layer::solid(
            1,
            "Nested benchmark reference",
            [SIZE[0] as f32, SIZE[1] as f32],
            [SIZE[0] as f32 / 2., SIZE[1] as f32 / 2., 0.],
            [1.; 4],
        );
        reference.content = Content::Composition {
            clip: CompositionClip::new("comp-benchmark-child".into()),
        };
        p.layers.push(reference);
    }
    p.rebuild_plugin_dependencies();
    p.validate()?;
    Ok(p)
}

fn distribution(values: impl Iterator<Item = f64>) -> Value {
    let mut values: Vec<_> = values.collect();
    if values.is_empty() {
        return Value::Null;
    }
    values.sort_by(f64::total_cmp);
    let percentile = |fraction: f64| {
        values[((values.len() as f64 * fraction).ceil() as usize).saturating_sub(1)]
    };
    json!({"count": values.len(), "p50": percentile(0.5), "p95": percentile(0.95), "p99": percentile(0.99), "max": values.last()})
}

fn video_fixture() -> Yuv420Frame {
    let [width, height] = VIDEO_SIZE;
    Yuv420Frame {
        width,
        height,
        rotation: 0,
        standard: 1,
        range: 2,
        phase: [0, 0],
        chroma_layout: ChromaLayout::Uv,
        y: vec![128; (width * height) as usize],
        uv: [100, 160].repeat((width * height / 4) as usize),
    }
}

fn measure(name: &str, samples: u32, root: &Path) -> Result<Value> {
    let dir = root.join(name);
    std::fs::create_dir(&dir)?;
    let project = fixture(name, samples)?;
    let project_bytes = serde_json::to_vec_pretty(&project)?;
    std::fs::write(dir.join("project.json"), &project_bytes)?;
    let instance =
        wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
    let started = Instant::now();
    let mut renderer = pollster::block_on(Renderer::new_profiled(
        &instance,
        None,
        wgpu::TextureFormat::Rgba8UnormSrgb,
        true,
    ))?;
    let renderer_create_us = started.elapsed().as_micros();
    renderer.set_memory_policy(memory_policy())?;
    let source = renderer.render_target(SIZE[0], SIZE[1])?;
    let target = renderer.capture_target(SIZE[0], SIZE[1])?;
    // This standalone path excludes the Presenter's two raw uniform buffers
    // from Session admission, explicitly recorded in coverageGaps below.
    let presenter = Presenter::new(&renderer, &source.view, renderer.target_format);
    let mut timer = GpuTimer::new(&renderer)?;
    let mut scene = Scene::new(&project)?;
    let mut video = (name == "four-4k-yuv").then(video_fixture);
    let mut records = Vec::new();
    for index in 0..WARMUP + samples {
        let began = Instant::now();
        let composition_frame = if name == "static" { 0 } else { index };
        scene.sample(
            &project,
            f64::from(composition_frame),
            None,
            &motion_core::ExpressionEvaluator,
        )?;
        let sample_us = began.elapsed().as_micros();
        let previous_conversions = renderer.video_gpu_conversions;
        let previous_submissions = renderer.video_conversion_submissions;
        let previous_uploads = renderer.video_upload_bytes;
        let previous_image_uploads = renderer.image_upload_bytes;
        let previous_pipelines = renderer.effect_pipeline_builds();
        let media_began = Instant::now();
        if let Some(video) = &mut video {
            for layer in &mut scene.layers {
                let pts = u64::from(index) * 1_000_000 / 30;
                video.y[0] = 32 + ((u64::from(index) + layer.id) % 190) as u8;
                layer.video = Some(VideoSample {
                    asset: layer.id,
                    source_time_us: pts,
                });
                renderer.upload_video_yuv(layer.id, layer.id, pts, video)?;
            }
        }
        let media_us = media_began.elapsed().as_micros();
        let slot = timer
            .as_mut()
            .map(|timer| {
                timer
                    .begin(u64::from(index))
                    .ok_or("benchmark timing slot unavailable")
            })
            .transpose()?;
        let mut encoder = renderer.allocation_command_encoder(&Default::default())?;
        let encode_began = Instant::now();
        let stats = renderer.encode(
            &scene,
            &source.view,
            SIZE[0],
            SIZE[1],
            &mut encoder,
            timer
                .as_ref()
                .zip(slot)
                .map(|(timer, slot)| timer.writes(slot, 0)),
        )?;
        if !renderer.effect_diagnostics.is_empty() {
            return Err(renderer.effect_diagnostics.join("; ").into());
        }
        presenter.encode(
            &mut encoder,
            &target.view,
            timer
                .as_ref()
                .zip(slot)
                .map(|(timer, slot)| timer.writes(slot, 1)),
            None,
            project.background,
        );
        if let (Some(timer), Some(slot)) = (&timer, slot) {
            timer.resolve(slot, &mut encoder);
        }
        let encode_us = encode_began.elapsed().as_micros();
        let submit_began = Instant::now();
        renderer.submit_allocated(encoder.finish(), |ledger| {
            source.collect_texture_resources(
                ledger,
                "benchmark/source",
                motion_render::TextureState::Retained,
            );
            ledger.merge(target.texture_resource_ledger());
        })?;
        if let (Some(timer), Some(slot)) = (&timer, slot) {
            timer.map(slot);
        }
        let submit_us = submit_began.elapsed().as_micros();
        let wait_began = Instant::now();
        // Deliberate offline latency measurement; never used by application playback.
        renderer.device.poll(wgpu::PollType::wait_indefinitely())?;
        let gpu = timer
            .as_mut()
            .and_then(|timer| timer.collect().into_iter().flatten().next());
        if timer.is_some() && gpu.is_none() {
            return Err("GPU timing was enabled but its actual sample is missing".into());
        }
        let wait_us = wait_began.elapsed().as_micros();
        renderer.check_health()?;
        let waited_wall_us = began.elapsed().as_micros();
        // Explicit metadata sampling is outside the timed interval. No handles are retained.
        let mut ledger = renderer.texture_resource_ledger();
        source.collect_texture_resources(
            &mut ledger,
            "benchmark/source",
            motion_render::TextureState::Retained,
        );
        ledger.merge(target.texture_resource_ledger());
        records.push(json!({"index":index,"phase":if index==0 {"cold"} else if index<WARMUP {"warmup"} else {"measured"},
            "compositionFrame":composition_frame,"sceneSampleUs":sample_us,"cpuPrepareUs":stats.cpu_prepare_us,
            "mediaUploadWallUs":media_us,"encodeWallUs":encode_us,"submitWallUs":submit_us,"waitUs":wait_us,"waitedWallUs":waited_wall_us,
            "gpuCompositionUs":gpu.map(|value|value.composition_us),"gpuPresentationUs":gpu.map(|value|value.presentation_us),"gpuTotalUs":gpu.map(|value|value.total_us),
            "mainSubmissions":1,"videoConversionSubmissions":renderer.video_conversion_submissions-previous_submissions,
            "videoConversions":renderer.video_gpu_conversions-previous_conversions,"videoUploadBytes":renderer.video_upload_bytes-previous_uploads,
            "imageUploadBytes":renderer.image_upload_bytes-previous_image_uploads,"vectorUpload":renderer.vector_upload_info()?,
            "parameterUploadBytes":stats.parameter_upload_bytes,"parameterResourceUploadBytes":stats.parameter_resource_upload_bytes,
            "instanceUploadBytes":stats.instance_upload_bytes,"effectPipelineBuilds":renderer.effect_pipeline_builds()-previous_pipelines,
            "drawCalls":stats.draw_calls,"textureLedger":{"knownBytes":ledger.total_known_bytes(),"unknownCapacityCount":ledger.unknown_capacity_count(),"resources":ledger.resources().collect::<Vec<_>>()},
            "logicalAdmission":renderer.allocation_budget().info()?,"pixelReadbackBytes":0,"timestampReadbackBytes":if gpu.is_some(){32}else{0}}));
    }
    // One evidence image after all samples, explicitly excluded from timed readback.
    let pixels = renderer.read_target(&target)?;
    image::save_buffer(
        dir.join("final.png"),
        &pixels,
        SIZE[0],
        SIZE[1],
        image::ColorType::Rgba8,
    )?;
    let measured: Vec<_> = records
        .iter()
        .filter(|row| row["phase"] == "measured")
        .collect();
    let metrics = [
        "sceneSampleUs",
        "cpuPrepareUs",
        "mediaUploadWallUs",
        "encodeWallUs",
        "submitWallUs",
        "waitUs",
        "waitedWallUs",
        "gpuCompositionUs",
        "gpuPresentationUs",
        "gpuTotalUs",
    ];
    let distributions = metrics
        .into_iter()
        .map(|key| {
            (
                key.to_owned(),
                distribution(measured.iter().filter_map(|row| row[key].as_f64())),
            )
        })
        .collect::<serde_json::Map<_, _>>();
    let pressure = renderer.memory_pressure();
    let info = &renderer.adapter_info;
    let report = json!({"schemaVersion":1,"case":name,"adapter":{"name":info.name,"driver":info.driver,"driverInfo":info.driver_info,
        "backend":format!("{:?}",info.backend),"deviceType":format!("{:?}",info.device_type)},
        "sourceRevision":option_env!("MOTION_BENCHMARK_REVISION"),"debugAssertions":cfg!(debug_assertions),
        "projectSha256":format!("{:x}",Sha256::digest(&project_bytes)),"finalRgbaSha256":format!("{:x}",Sha256::digest(&pixels)),
        "dimensions":SIZE,"fps":30,"renderMode":"fixed-full-resolution","memoryPolicy":renderer.memory_policy(),"warmupFrames":WARMUP,"measuredFrames":samples,"rendererCreateUs":renderer_create_us,
        "submissionScope":"Explicit main and YUV conversion queue submissions; excludes implicit driver/upload staging submissions",
        "gpuTimestampSupported":timer.is_some(),"nativeMemoryPressure":pressure,
        "syntheticYuv":video.as_ref().map(|frame|json!({"dimensions":VIDEO_SIZE,"sources":4,"oneReusableCpuFixtureBytes":frame.y.len()+frame.uv.len(),
            "decoderMeasured":false,"changingPixel":"Y[0] = 32 + (frame + layer_id) % 190","remainingY":128,"uv":[100,160]})),
        "scope":"Offline serialized GPU latency; fixed full-resolution encode, new renderer per case, cold and warmup retained. GPU timestamps surround root composition and offscreen presentation; earlier YUV conversions and nested/vector/MSAA preparation can fall outside them. CPU wall includes uploads and deliberate per-frame completion wait. Metadata sampling and final PNG readback are outside timing. No native window, UI, file decoder, display deadline, physical residency or process-RAM claim.",
        "coverageGaps":["CPU image/decode cache ownership","raw Presenter uniform buffers","driver and codec external allocations","inference weights/activations","physical memory pressure","Android Adreno/Mali execution"],
        "distributions":distributions,"frames":records});
    std::fs::write(dir.join("report.json"), serde_json::to_vec_pretty(&report)?)?;
    Ok(report)
}

fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let root = args
        .first()
        .ok_or("usage: performance_baseline <new-artifact-directory> [samples:1..600]")?;
    let samples: u32 = args
        .get(1)
        .map(|value| value.parse())
        .transpose()?
        .unwrap_or(60);
    if args.len() > 2 || !(1..=600).contains(&samples) {
        return Err("sample count must be 1..600".into());
    }
    let root = Path::new(root);
    std::fs::create_dir(root)?;
    let mut events = File::create(root.join("events.jsonl"))?;
    let mut cases = Vec::new();
    for name in CASES {
        let began = Instant::now();
        writeln!(
            events,
            "{}",
            json!({"event":"start","operation":name,"samples":samples})
        )?;
        events.flush()?;
        match measure(name, samples, root) {
            Ok(report) => {
                cases.push(json!({"case":name,"report":format!("{name}/report.json"),"projectSha256":report["projectSha256"]}));
            }
            Err(error) => {
                let evidence = writeln!(
                    events,
                    "{}",
                    json!({"event":"error","operation":name,"elapsedMs":began.elapsed().as_millis(),"cause":error.to_string()})
                ).and_then(|_| events.flush());
                if let Err(archive_error) = evidence {
                    eprintln!("benchmark {name}: primary={error}; failure evidence also failed: {archive_error}");
                }
                return Err(error);
            }
        }
        writeln!(
            events,
            "{}",
            json!({"event":"done","operation":name,"elapsedMs":began.elapsed().as_millis()})
        )?;
        events.flush()?;
    }
    std::fs::write(
        root.join("report.json"),
        serde_json::to_vec_pretty(&json!({"schemaVersion":1,"cases":cases}))?,
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn benchmark_fixtures_are_valid_fixed_and_exercise_the_declared_features() -> Result<()> {
        memory_policy().validate()?;
        assert_eq!(memory_policy().source_texture_budget_bytes, 256 << 20);
        assert!(memory_policy().source_texture_budget_bytes > 4 * 3840 * 2160 * 11 / 2);
        for name in CASES {
            let project = fixture(name, 60)?;
            project.validate()?;
            assert_eq!(
                serde_json::to_vec(&project)?,
                serde_json::to_vec(&fixture(name, 60)?)?
            );
            assert_eq!(
                (project.width, project.height, project.fps, project.frames),
                (1280, 720, 30, 68)
            );
            match name {
                "static" => assert_eq!(
                    project
                        .layers
                        .iter()
                        .filter(|layer| matches!(layer.content, Content::Vector { .. }))
                        .count(),
                    2
                ),
                "animated" => {
                    assert!(project
                        .layers
                        .iter()
                        .all(|layer| layer.transform.position.keys.len() == 2))
                }
                "effect-chain" => assert_eq!(project.layers[0].effects.len(), 8),
                "crossing-3d" => assert!(project.layers.iter().all(|layer| layer.three_d)),
                "nested-matte" => assert!(project.compositions[0].layers[1].track_matte.is_some()),
                "four-4k-yuv" => assert!(project
                    .layers
                    .iter()
                    .all(|layer| layer.size == [3840., 2160.])),
                _ => (),
            }
        }
        Ok(())
    }
    #[test]
    fn raw_sample_percentiles_use_nearest_rank_and_preserve_missing_gpu_data() {
        assert!(distribution(std::iter::empty()).is_null());
        assert_eq!(
            distribution((1..=100).rev().map(f64::from)),
            json!({"count":100,"p50":50.,"p95":95.,"p99":99.,"max":100.})
        );
    }
}
