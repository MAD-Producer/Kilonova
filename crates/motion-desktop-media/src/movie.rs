//! Independent export snapshots. Source bytes and effect packages stay fixed
//! while the live editor changes, seeks, switches composition or closes.
use motion_core::Project;
use motion_effects::Registry;
use motion_render::resource_policy::MemoryPolicy;
use motion_host::session::Session;
// Export providers retain their existing textual API independently of the host.
type Result<T> = std::result::Result<T, String>;
use serde::Serialize;
use std::{
    collections::BTreeSet,
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex,
    },
};

pub mod frame;

#[cfg_attr(not(feature = "ffmpeg"), allow(dead_code))]
pub struct FrozenMovie {
    project: Project,
    registry: Registry,
    assets: tempfile::TempDir,
    memory_policy: MemoryPolicy,
    allocation_budget: motion_render::AllocationBudget,
    source_paths: BTreeSet<PathBuf>,
    pub revision: u64,
}

#[cfg(all(test, feature = "ffmpeg"))]
mod camera_tests;
#[cfg(all(test, feature = "ffmpeg"))]
mod session_budget_tests;

#[cfg(all(test, feature = "ffmpeg"))]
mod tests {
    use super::*;
    use motion_core::{
        AudioAsset, AudioClip, Command, CompositionAction, Content, EffectInstance, Engine,
        ExpressionTarget, Layer, Property, PropertyExpression, VideoClip, EXPRESSION_PROFILE,
    };
    use std::io::Write;

    fn tone(count: usize, offset: u64) -> Vec<f32> {
        (0..count)
            .flat_map(|i| {
                let value = ((offset + i as u64) as f32 * 440.0 * std::f32::consts::TAU / 48_000.0)
                    .sin()
                    * 0.5;
                [value, value]
            })
            .collect()
    }
    #[test]
    fn export_snapshot_freezes_source_and_scratch_limits() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().join("project");
        fs::create_dir(&root).unwrap();
        let mut session = Session::new(
            Project::new(64, 64, 30, 6).unwrap(),
            root,
            crate::platform(),
        )
        .unwrap();
        let policy = MemoryPolicy {
            session_allocation_budget_bytes: 512 << 20,
            source_texture_budget_bytes: 256 << 20,
            scratch_budget_bytes: 96 << 20,
            preview_target_budget_bytes: 4 << 20,
        };
        session.configure_memory_policy(policy).unwrap();
        let plan = FrozenMovie::capture_plan(&session, &ExportControl::default()).unwrap();
        session
            .configure_memory_policy(MemoryPolicy::default())
            .unwrap();
        let frozen = plan.freeze(&ExportControl::default()).unwrap();
        assert_eq!(frozen.memory_policy, policy);
        assert_eq!(session.effects.memory_policy(), MemoryPolicy::default());
    }
    #[test]
    fn frozen_movie_samples_animated_layer_anchor_after_live_edits() {
        let dir=tempfile::tempdir().unwrap();
        let root=dir.path().join("project");fs::create_dir_all(&root).unwrap();
        let mut p=Project::new(64,64,30,6).unwrap();
        p.layers.push(Layer::solid(7,"Anchor card",[16.;2],[32.,32.,0.],[1.;4]));
        let mut session=Session::new(p,root,crate::platform()).unwrap();
        session.engine.apply(Command::Animate{object:7,property:Property::Anchor,axis:None,frame:0,enabled:true}).unwrap();
        for (frame,value) in [(0,[0.,0.,0.]),(5,[1.,1.,0.])] {
            session.engine.apply(Command::SetVector{object:7,property:Property::Anchor,frame,value}).unwrap();
        }
        let frozen=FrozenMovie::from_session(&session).unwrap();
        let track=frozen.project.layers[0].transform.anchor.clone();
        session.engine.apply(Command::Anchor{object:7,anchor:[0.5;2]}).unwrap();
        assert!(!session.engine.project().layers[0].transform.anchor.is_animated());
        assert_eq!(frozen.project.layers[0].transform.anchor,track);
        let out=dir.path().join("anchor.mp4");
        let result=frozen.export(&out,&ExportControl::default(),|_|{}).unwrap();
        assert!(result.bytes>0);
        let (first,last)=decoded_pixels(&out);
        assert!(pixel(&first,44,44)[0]>220 && pixel(&last,44,44)[0]<40);
        assert!(pixel(&first,20,20)[0]<40 && pixel(&last,20,20)[0]>220);
    }
    #[test]
    fn frozen_movie_renders_receiver_exposure_and_keeps_animated_upper_sharp_after_live_edits() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("project");
        fs::create_dir_all(&root).unwrap();
        let mut p = Project::new(64,64,30,6).unwrap();
        p.version = motion_core::CURRENT_PROJECT_VERSION;
        p.background = [0.,0.,0.,1.];
        for (id,color,y) in [(1,[1.,0.,0.,1.],16.),(2,[0.,1.,0.,1.],48.)] {
            let mut l = Layer::solid(id,"independently animated",[8.;2],[10.,y,0.],color);
            for (f,x) in [(0,10.),(4,10.),(5,54.)] {
                l.transform.position.upsert(f,[x,y,0.],motion_core::Ease::Linear).unwrap();
            }
            if id == 1 { l.sampling.motion_blur = Some(motion_core::render_sampling::Shutter {frames:2.,samples:8}); }
            p.layers.push(l);
        }
        let mut session = Session::new(p,root,crate::platform()).unwrap();
        let frozen = FrozenMovie::from_session(&session).unwrap();
        session.engine.apply(Command::SetLayerSampling {object:1,sampling:Default::default()}).unwrap();
        let output = dir.path().join("receiver-exposure.mp4");
        let started = std::time::Instant::now();
        let result = frozen.export(&output,&ExportControl::default(),|_|{}).unwrap();
        let export_us = started.elapsed().as_micros();
        assert!(result.bytes>0);
        let (first,last) = decoded_pixels(&output);
        assert!((150..230).contains(&pixel(&first,10,16)[0]),"transparent shutter boundary: {:?}",pixel(&first,10,16));
        assert!(pixel(&first,10,48)[1]>230,"unblurred boundary reference");
        let support = (10..50).filter(|x|pixel(&last,*x,16)[0]>30).count();
        assert!(support>12,"moving red exposure had only {support} pixels before center");
        assert!(pixel(&last,54,48)[1]>230);
        assert!(pixel(&last,44,48)[1]<20,"upper animation was blurred with lower receiver");
        retain_exposure_artifacts("receiver-exposure", &output, &result, export_us,
            &[("first",&first),("last",&last)]);
    }
    #[test]
    fn frozen_movie_integrates_real_adjacent_video_samples_in_linear_exposure() {
        assert_real_video_exposure(false);
    }
    #[test]
    fn frozen_movie_integrates_parent_and_child_shutters_at_different_rates_with_real_video() {
        assert_real_video_exposure(true);
    }
    fn assert_real_video_exposure(nested:bool) {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("project");
        fs::create_dir_all(root.join("assets")).unwrap();
        let source = root.join("assets/source.mp4");
        source_movie(&source);
        let (red,blue) = decoded_pixels(&source);
        let probe = crate::av::probe(&source,None,None,&||Ok(())).unwrap();
        let mut asset = probe.asset;
        asset.id = 1;
        asset.path = "assets/source.mp4".into();
        let index = motion_media::video_cache_path(&root,&asset).unwrap();
        fs::create_dir_all(index.parent().unwrap()).unwrap();
        motion_media::save_index(&index,&asset,&probe.timestamps).unwrap();
        let mut p = Project::new(64,64,30,6).unwrap();
        p.version = motion_core::CURRENT_PROJECT_VERSION;
        p.background = [0.,0.,0.,1.];
        let mut l = Layer::solid(1,"required neighboring video samples",[64.;2],[32.,32.,0.],[1.;4]);
        l.content = Content::Video {video:VideoClip::new(1)};
        l.sampling.frame_blend = motion_core::render_sampling::FrameBlend::Linear;
        l.sampling.motion_blur = Some(motion_core::render_sampling::Shutter {frames:2.,samples:8});
        if nested {
            p.compositions.push(motion_core::Composition {id:"comp-movie-shutter".into(),name:"60 fps child".into(),
                width:64,height:64,fps:60,frames:12,background:[0.;4],camera:motion_core::Camera::new(64,64),
                layers:vec![l],expressions:vec![]});
            let mut parent=Layer::solid(1,"parent shutter",[64.;2],[32.,32.,0.],[1.;4]);
            parent.content=Content::Composition {clip:motion_core::CompositionClip::new("comp-movie-shutter".into())};
            parent.sampling.motion_blur=Some(motion_core::render_sampling::Shutter {frames:1.,samples:2});
            p.layers.push(parent);
        } else {p.layers.push(l);}
        p.video_assets.push(asset);
        let mut session = Session::new(p,root,crate::platform()).unwrap();
        session.frame = 2.;
        let png_plan = frame::FrameFreezePlan::capture(&session, None, &ExportControl::default()).unwrap();
        let frozen = FrozenMovie::from_session(&session).unwrap();
        let name=if nested {"nested-video-exposure"} else {"linear-video-exposure"};
        let output = dir.path().join(format!("{name}.mp4"));
        let started = std::time::Instant::now();
        let result = frozen.export(&output,&ExportControl::default(),|_|{}).unwrap();
        let export_us = started.elapsed().as_micros();
        let probe = crate::av::probe(&output,None,None,&||Ok(())).unwrap();
        let pts = probe.timestamps[2];
        let mut decoder = crate::av::Decoder::open(&output,probe.asset,probe.timestamps).unwrap();
        let decoded = decoder.frame(pts,&||Ok(())).unwrap();
        let pixels = decoded.rgba().unwrap();
        let actual = pixel(&pixels,32,32);
        // At frame 2 the latter four shutter midpoints blend the red source
        // into blue with weights .125/.375/.625/.875: integrated blue is .25.
        let linear = |v:u8| {let s=f32::from(v)/255.;if s<=0.04045{s/12.92}else{((s+0.055)/1.055).powf(2.4)}};
        let srgb = |v:f32| {let s=if v<=0.0031308{v*12.92}else{1.055*v.powf(1./2.4)-0.055};(s*255.).round() as u8};
        // Parent midpoints 1.75/2.25 map to child centers 3.5/4.5.
        // Averaging eight child bins at each center integrates blue to .15625.
        let blue_weight=if nested {0.15625} else {0.25};
        let expected:Vec<_> = (0..3).map(|c|srgb(linear(pixel(&red,32,32)[c])*(1.-blue_weight)+linear(pixel(&blue,32,32)[c])*blue_weight)).collect();
        for c in 0..3 {assert!(actual[c].abs_diff(expected[c])<=12,"channel={c} decoded={actual:?} linear expected={expected:?}");}
        let png_started = std::time::Instant::now();
        let png = png_plan.export(&ExportControl::default()).unwrap();
        let png_us = png_started.elapsed().as_micros();
        let png_pixels = image::open(png["path"].as_str().unwrap()).unwrap().into_rgba8();
        let png_actual = png_pixels.get_pixel(32,32).0;
        for c in 0..3 { assert!(png_actual[c].abs_diff(expected[c]) <= 2, "PNG channel={c} actual={png_actual:?} expected={expected:?}"); }
        if let Some(directory) = std::env::var_os("MOTION_FRAME_EXPORT_ARTIFACTS") {
            let directory = PathBuf::from(directory);
            fs::create_dir_all(&directory).unwrap();
            fs::copy(png["path"].as_str().unwrap(),directory.join(format!("{name}.png"))).unwrap();
            fs::write(directory.join(format!("{name}.json")),serde_json::to_vec_pretty(&serde_json::json!({
                "result":png,"exportUs":png_us,"expected":expected,"actual":png_actual,
                "scope":"frozen PNG export including source copy, GPU initialization, decode, all samples, readback, encode, verification and publication"
            })).unwrap()).unwrap();
        }
        retain_exposure_artifacts(name, &output, &result, export_us,
            &[("frame-2",&pixels)]);
    }
    fn retain_exposure_artifacts(name:&str, output:&Path, result:&MovieResult, export_us:u128,
        frames:&[(&str,&[u8])]) {
        if let Some(directory) = std::env::var_os("MOTION_SAMPLING_ARTIFACT_DIR") {
            let directory = PathBuf::from(directory);
            fs::create_dir_all(&directory).unwrap();
            fs::copy(output,directory.join(format!("{name}.mp4"))).unwrap();
            for (label,pixels) in frames {
                fs::write(directory.join(format!("{name}-{label}.rgba")),pixels).unwrap();
            }
            let cost = serde_json::json!({"movie":result,"retained_movie":format!("{name}.mp4"),"export_us":export_us,
                "scope":"FrozenMovie::export including renderer initialization, media, GPU, audio, encoding, verification and publish; excludes freeze, decode-back and artifact writes",
                "decoded_frames":"64x64 RGBA8; independent tests authored the source"});
            fs::write(directory.join(format!("{name}-cost.json")),serde_json::to_vec_pretty(&cost).unwrap()).unwrap();
        }
    }
    fn source_movie(path: &Path) {
        let mut encode = crate::encode::Mp4Encoder::new(path, 64, 64, 30).unwrap();
        for frame in 0..6 {
            let color = if frame < 3 {
                [255, 0, 0, 255]
            } else {
                [0, 0, 255, 255]
            };
            encode.video_frame(&color.repeat(64 * 64), frame).unwrap();
        }
        let mut sample = 0;
        while sample < 9600 {
            let count = encode.audio_frame_size().min((9600 - sample) as usize);
            encode.audio_frame(&tone(count, sample), sample).unwrap();
            sample += count as u64;
        }
        encode.finish().unwrap();
    }
    fn decoded_audio_rms(path: &Path) -> (usize, f64) {
        use ffmpeg_next::{codec, format, frame, media};
        let mut input = format::input(path).unwrap();
        let stream = input.streams().best(media::Type::Audio).unwrap();
        assert_eq!(stream.parameters().id(), codec::Id::AAC);
        let track = stream.index();
        let mut decoder = codec::context::Context::from_parameters(stream.parameters())
            .unwrap()
            .decoder()
            .audio()
            .unwrap();
        let mut count = 0;
        let mut power = 0.0;
        let mut read = |decoder: &mut codec::decoder::Audio| {
            let mut decoded = frame::Audio::empty();
            loop {
                match decoder.receive_frame(&mut decoded) {
                    Ok(()) => {}
                    Err(ffmpeg_next::Error::Eof) => break,
                    Err(ffmpeg_next::Error::Other { errno })
                        if errno == ffmpeg_next::util::error::EAGAIN => break,
                    Err(error) => panic!("decode exported AAC: {error}"),
                }
                assert_eq!(decoded.rate(), 48_000);
                assert_eq!(decoded.channels(), 2);
                for sample in decoded.plane::<f32>(0) {
                    power += f64::from(*sample).powi(2);
                    count += 1;
                }
            }
        };
        for (stream, packet) in input.packets() {
            if stream.index() == track {
                decoder.send_packet(&packet).unwrap();
                read(&mut decoder);
            }
        }
        decoder.send_eof().unwrap();
        read(&mut decoder);
        (count, (power / count as f64).sqrt())
    }
    fn decoded_pixels(path: &Path) -> (Vec<u8>, Vec<u8>) {
        let probe = crate::av::probe(path, None, None, &|| Ok(())).unwrap();
        assert_eq!(probe.asset.frame_count, 6);
        assert_eq!(probe.asset.video_end_us, 200_000, "six 30 fps frames must include the final sample duration");
        assert_eq!(
            probe.asset.color_standard, 1,
            "MP4 must signal the BT.709 conversion matrix"
        );
        assert_eq!(
            probe.asset.color_range, 2,
            "MP4 must signal limited-range YUV"
        );
        let first = probe.first_rgba;
        let last_pts = *probe.timestamps.last().unwrap();
        let mut decoder = crate::av::Decoder::open(path, probe.asset, probe.timestamps).unwrap();
        let last = decoder
            .frame(last_pts, &|| Ok(()))
            .unwrap()
            .rgba()
            .unwrap()
            .into_owned();
        (first, last)
    }
    fn pixel(pixels: &[u8], x: usize, y: usize) -> &[u8] {
        &pixels[(y * 64 + x) * 4..(y * 64 + x + 1) * 4]
    }

    #[test]
    fn native_mp4_roundtrip_contains_all_video_frames_and_audible_aac() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("roundtrip.mp4");
        source_movie(&file);
        let (first, last) = decoded_pixels(&file);
        assert!(pixel(&first, 32, 32)[0] > 220 && pixel(&first, 32, 32)[2] < 25);
        assert!(pixel(&last, 32, 32)[2] > 220 && pixel(&last, 32, 32)[0] < 25);
        let (samples, rms) = decoded_audio_rms(&file);
        assert!((9600..=10_240).contains(&samples), "AAC samples={samples}");
        assert!(rms > 0.3 && rms < 0.4, "AAC RMS={rms}");
    }

    #[test]
    fn frozen_movie_evaluates_nested_video_effects_expressions_and_original_pcm() {
        let root = tempfile::tempdir().unwrap();
        fs::create_dir_all(root.path().join("assets")).unwrap();
        fs::create_dir_all(root.path().join("cache/audio-v1")).unwrap();
        fs::create_dir_all(root.path().join("cache/video-v1")).unwrap();
        let source = root.path().join("assets/source.mp4");
        source_movie(&source);
        let probe = crate::av::probe(&source, None, Some(u32::MAX), &|| Ok(())).unwrap();
        let mut asset = probe.asset;
        asset.id = 1;
        asset.path = "assets/source.mp4".into();
        motion_media::save_index(
            &motion_media::video_cache_path(root.path(), &asset).unwrap(),
            &asset,
            &probe.timestamps,
        )
        .unwrap();
        let audio = AudioAsset {
            id: 2,
            path: "assets/tone.wav".into(),
            mime: "audio/wav".into(),
            bytes: 1,
            track: 0,
            sample_rate: 48_000,
            channels: 2,
            sample_frames: 9600,
            duration_us: 200_000,
        };
        fs::write(root.path().join(&audio.path), [0]).unwrap();
        let pcm_path = motion_media::cache_path(root.path(), &audio).unwrap();
        fs::create_dir_all(pcm_path.parent().unwrap()).unwrap();
        let mut pcm = fs::File::create(&pcm_path).unwrap();
        for value in tone(9600, 0) {
            pcm.write_all(&value.to_le_bytes()).unwrap();
        }
        drop(pcm);
        let mut project = Project::new(64, 64, 30, 6).unwrap();
        project.video_assets.push(asset);
        project.audio_assets.push(audio);
        let mut video = Layer::solid(1, "video", [64.0; 2], [32.0, 32.0, 0.0], [1.0; 4]);
        video.content = Content::Video {
            video: VideoClip::new(1),
        };
        let package = motion_effects::builtin::package().unwrap();
        let tint = package
            .manifest
            .effects
            .iter()
            .find(|effect| effect.id == "tint")
            .unwrap();
        video.effects.push(EffectInstance::new(
            1,
            &package.manifest.id,
            &package.manifest.version,
            &package.hash,
            tint,
            [64.0; 2],
        ));
        let moving = Layer::solid(2, "animated", [12.0; 2], [8.0, 16.0, 0.0], [1.0; 4]);
        let mut sound = Layer::solid(3, "audio", [0.0; 2], [0.0; 3], [1.0; 4]);
        sound.content = Content::Audio {
            audio: AudioClip::new(2),
        };
        project.layers = vec![video, moving, sound];
        project.rebuild_plugin_dependencies();
        let mut engine = Engine::new(project).unwrap();
        engine
            .apply(Command::SetExpression {
                expression: PropertyExpression {
                    target: ExpressionTarget::Property {
                        object: 2,
                        property: Property::Position,
                        axis: None,
                    },
                    source: "value+[time*240,0,0]".into(),
                    enabled: true,
                    seed: 0,
                    profile: EXPRESSION_PROFILE.into(),
                },
                frame: 0,
            })
            .unwrap();
        engine
            .apply(Command::Composition {
                action: CompositionAction::Precompose {
                    objects: vec![1, 2, 3],
                    name: "nested".into(),
                    range: "composition".into(),
                },
            })
            .unwrap();
        let mut registry = Registry::new_with_builtins().unwrap();
        let snapshot = FrozenMovie::new(
            engine.snapshot(),
            root.path(),
            registry.clone(),
            motion_render::resource_policy::MAX_SCRATCH_BUDGET,
            7,
        )
        .unwrap();
        // Mutate all three live inputs after freezing. Export must retain the old bytes.
        fs::write(&source, b"deleted video source").unwrap();
        fs::write(&pcm_path, vec![0; 9600 * 8]).unwrap();
        registry.disabled.extend(registry.packages.keys().cloned());
        let output = root.path().join("movie.mp4");
        let result = snapshot
            .export(&output, &ExportControl::default(), |_| {})
            .unwrap();
        assert_eq!(result.revision, 7);
        assert!(result.bytes > 0);
        let (first, last) = decoded_pixels(&output);
        let a = pixel(&first, 32, 48);
        let b = pixel(&last, 32, 48);
        assert!(
            a[0].abs_diff(a[1]) < 8 && a[1].abs_diff(a[2]) < 8,
            "tint pixel={a:?}"
        );
        assert!(
            a[0] > b[0] + 20,
            "video sequence must advance: {a:?} -> {b:?}"
        );
        assert!(pixel(&first, 8, 16)[0] > 220 && pixel(&last, 8, 16)[0] < 150, "expression must move the white card: first-left={:?}, last-left={:?}, first-right={:?}, last-right={:?}", pixel(&first, 8, 16),pixel(&last, 8, 16),pixel(&first, 48, 16),pixel(&last, 48, 16));
        assert!(
            pixel(&last, 48, 16)[0] > 220,
            "last card pixel={:?}",
            pixel(&last, 48, 16)
        );
        assert!(
            decoded_audio_rms(&output).1 > 0.3,
            "the frozen PCM must remain audible"
        );
    }

    #[test]
    fn cancellation_keeps_existing_output_and_removes_temporary_movie() {
        let dir = tempfile::tempdir().unwrap();
        let output = dir.path().join("movie.mp4");
        fs::write(&output, b"existing movie").unwrap();
        let snapshot = FrozenMovie::new(
            Project::new(64, 64, 30, 6).unwrap(),
            dir.path(),
            Registry::new_with_builtins().unwrap(),
            motion_render::resource_policy::MAX_SCRATCH_BUDGET,
            0,
        )
        .unwrap();
        let control = ExportControl::default();
        let cancel = control.clone();
        let result = snapshot.export(&output, &control, move |p| {
            if p > 0.1 {
                cancel.cancel();
            }
        });
        assert_eq!(result.err().as_deref(), Some("movie export cancelled"));
        assert_eq!(fs::read(output).unwrap(), b"existing movie");
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
    }

    #[test]
    fn exposure_cancellation_after_encoded_frame_preserves_destination_and_removes_temporary_movie() {
        let dir = tempfile::tempdir().unwrap();
        let output = dir.path().join("movie.mp4");
        fs::write(&output,b"existing movie").unwrap();
        let mut project = Project::new(64,64,30,6).unwrap();
        project.version = motion_core::CURRENT_PROJECT_VERSION;
        let mut layer = Layer::solid(1,"cancelled receiver",[16.;2],[32.,32.,0.],[1.;4]);
        layer.sampling.motion_blur = Some(motion_core::render_sampling::Shutter {frames:1.,samples:4});
        project.layers.push(layer);
        let snapshot = FrozenMovie::new(project,dir.path(),Registry::new_with_builtins().unwrap(),
            motion_render::resource_policy::MAX_SCRATCH_BUDGET,0).unwrap();
        let control = ExportControl::default();
        let cancel = control.clone();
        let mut reports = Vec::new();
        let result = snapshot.export(&output,&control,|progress| {
            reports.push(progress);
            if progress>0.1 { assert!(cancel.cancel()); }
        });
        assert_eq!(result.err().as_deref(),Some("movie export cancelled"));
        assert_eq!(reports.len(),2);
        assert!(reports[1]>0.1 && reports[1]<1.);
        assert_eq!(fs::read(output).unwrap(),b"existing movie");
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(),1);
    }

    #[test]
    fn cancelled_freeze_plan_does_not_copy_sources() {
        let dir = tempfile::tempdir().unwrap();
        let control = ExportControl::default();
        let plan = MovieFreezePlan::new(
            Project::new(64, 64, 30, 6).unwrap(),
            dir.path(),
            Registry::new_with_builtins().unwrap(),
            MemoryPolicy { scratch_budget_bytes: motion_render::resource_policy::MAX_SCRATCH_BUDGET, ..Default::default() },
            0,
            &control,
        )
        .unwrap();
        control.cancel();
        assert_eq!(
            plan.freeze(&control).err().as_deref(),
            Some("movie export cancelled")
        );
        assert_eq!(control.progress(), 0.0);
    }

    #[test]
    fn changed_source_between_plan_capture_and_freeze_is_rejected() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir(dir.path().join("assets")).unwrap();
        let source = dir.path().join("assets/image.png");
        fs::write(&source, [1]).unwrap();
        let mut project = Project::new(64, 64, 30, 6).unwrap();
        project.assets.push(motion_core::Asset {
            id: 1,
            path: "assets/image.png".into(),
            width: 1,
            height: 1,
        });
        let control = ExportControl::default();
        let plan = MovieFreezePlan::new(
            project,
            dir.path(),
            Registry::new_with_builtins().unwrap(),
            MemoryPolicy { scratch_budget_bytes: motion_render::resource_policy::MAX_SCRATCH_BUDGET, ..Default::default() },
            0,
            &control,
        )
        .unwrap();
        fs::write(source, [1, 2]).unwrap();
        assert_eq!(
            plan.freeze(&control).err().as_deref(),
            Some("source changed before freezing movie; retry export")
        );
    }
}

/// Cancellation and the final rename share one lock. A successful cancellation
/// can never race publication and leave a movie reported as cancelled.
#[derive(Clone, Default)]
pub struct ExportControl(Arc<Mutex<ControlState>>, Arc<AtomicU64>);
#[derive(Default)]
struct ControlState {
    cancelled: bool,
    published: bool,
}
impl ExportControl {
    pub fn progress(&self) -> f64 {
        f64::from_bits(self.1.load(Ordering::Acquire))
    }
    fn set_progress(&self, progress: f64) {
        self.1.store(progress.to_bits(), Ordering::Release);
    }
    pub fn cancel(&self) -> bool {
        let mut state = self.0.lock().unwrap_or_else(|e| e.into_inner());
        if state.published {
            return false;
        }
        state.cancelled = true;
        true
    }
    pub fn check(&self) -> Result<()> {
        if self
            .0
            .lock()
            .map_err(|_| "export cancellation state poisoned")?
            .cancelled
        {
            Err("movie export cancelled".into())
        } else {
            Ok(())
        }
    }
}

#[derive(Clone, Serialize)]
pub struct MovieResult {
    pub path: PathBuf,
    pub bytes: u64,
    pub width: u32,
    pub height: u32,
    pub frames: u32,
    pub fps: u32,
    pub revision: u64,
    pub video_encoder: String,
    pub audio_codec: &'static str,
}

/// A cheap, transferable source plan. Materialize it on an export worker.
/// Open file handles preserve replaced/deleted sources; in-place changes are
/// rejected before or during copying, never silently exported as another revision.
pub struct MovieFreezePlan {
    project: Project,
    registry: Registry,
    root: PathBuf,
    memory_policy: MemoryPolicy,
    allocation_budget: motion_render::AllocationBudget,
    revision: u64,
    sources: Vec<(PathBuf, PathBuf, fs::File, fs::Metadata)>,
    source_paths: BTreeSet<PathBuf>,
    total_bytes: u64,
    include_audio: bool,
}
impl MovieFreezePlan {
    fn new(
        project: Project,
        root: &Path,
        registry: Registry,
        memory_policy: MemoryPolicy,
        revision: u64,
        control: &ExportControl,
    ) -> Result<Self> {
        Self::new_with_audio(project, root, registry, memory_policy, revision, control, true)
    }

    fn new_with_audio(
        project: Project, root: &Path, registry: Registry, memory_policy: MemoryPolicy,
        revision: u64, control: &ExportControl, include_audio: bool,
    ) -> Result<Self> {
        control.check()?;
        project.validate().map_err(|e| e.to_string())?;
        let root = root.canonicalize().map_err(|e| e.to_string())?;
        // Frozen-movie APIs currently return wire diagnostics. Preserve native
        // render errors until this existing, explicitly text-only boundary.
        let mut plans = motion_render::effect_plan::PlanBuilder::new(registry.clone())
            .map_err(|error| error.to_string())?;
        plans.set_memory_policy(memory_policy).map_err(|error| error.to_string())?;
        let allocation_budget = motion_render::AllocationBudget::new(memory_policy.session_allocation_budget_bytes)
            .map_err(|error| error.to_string())?;
        for id in project
            .reachable_compositions()
            .map_err(|e| e.to_string())?
        {
            control.check()?;
            plans.preflight_project(&project.composition(&id).map_err(|e| e.to_string())?)
                .map_err(|error| error.to_string())?;
        }
        let mut paths: BTreeSet<PathBuf> = project
            .assets
            .iter()
            .map(|a| PathBuf::from(&a.path))
            .chain(project.fonts.iter().map(|a| PathBuf::from(&a.path)))
            .chain(project.audio_assets.iter().filter(|_| include_audio).map(|a| PathBuf::from(&a.path)))
            .chain(project.video_assets.iter().map(|a| PathBuf::from(&a.path)))
            .collect();
        let voices = if include_audio { project.audio_voices().map_err(|e| e.to_string())? } else { Vec::new() };
        for asset in &project.audio_assets {
            if voices.iter().any(|v| v.asset == asset.id) {
                let pcm = motion_media::cache_path(&root, asset)?;
                paths.insert(
                    pcm.strip_prefix(&root)
                        .map_err(|e| e.to_string())?
                        .to_path_buf(),
                );
            }
        }
        for asset in &project.video_assets {
            let pts = motion_media::video_cache_path(&root, asset)?;
            paths.insert(
                pts.strip_prefix(&root)
                    .map_err(|e| e.to_string())?
                    .to_path_buf(),
            );
        }
        let mut source_paths = BTreeSet::new();
        // Visual exports need no PCM, but must still protect audio source files.
        for asset in &project.audio_assets {
            let path = root.join(&asset.path);
            let protected = match path.canonicalize() {
                Ok(path) => path,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => path,
                Err(error) => return Err(format!("resolve protected source {}: {error}", path.display())),
            };
            source_paths.insert(protected);
        }
        let mut sources = Vec::new();
        let mut total_bytes = 0u64;
        for relative in paths {
            control.check()?;
            if relative.as_os_str().is_empty()
                || !relative
                    .components()
                    .all(|component| matches!(component, std::path::Component::Normal(_)))
            {
                return Err("invalid relative export source path".into());
            }
            let source = root
                .join(&relative)
                .canonicalize()
                .map_err(|e| format!("freeze {}: {e}", relative.display()))?;
            if !source.starts_with(&root) {
                return Err("export source resolves outside project".into());
            }
            let file = fs::File::open(&source).map_err(|e| e.to_string())?;
            let before = file.metadata().map_err(|e| e.to_string())?;
            if !before.is_file() {
                return Err("export source is not a file".into());
            }
            total_bytes = total_bytes
                .checked_add(before.len())
                .ok_or("frozen asset size overflow")?;
            source_paths.insert(source.clone());
            sources.push((relative, source, file, before));
        }
        Ok(Self {
            project,
            registry,
            root,
            memory_policy,
            allocation_budget,
            revision,
            sources,
            source_paths,
            total_bytes,
            include_audio,
        })
    }

    pub fn freeze(self, control: &ExportControl) -> Result<FrozenMovie> {
        control.check()?;
        let Self {
            project,
            registry,
            root,
            memory_policy,
            allocation_budget,
            revision,
            sources,
            mut source_paths,
            total_bytes,
            include_audio,
        } = self;
        let assets = tempfile::Builder::new()
            .prefix("motion-movie-snapshot-")
            .tempdir()
            .map_err(|e| e.to_string())?;
        let mut buffer = vec![0; 1024 * 1024];
        let mut done_bytes = 0u64;
        for (relative, _source, mut file, before) in sources {
            control.check()?;
            let before_modified = before.modified().map_err(|e| {
                format!("freeze {} capture modification time: {e}", relative.display())
            })?;
            let current = file.metadata().map_err(|e| e.to_string())?;
            let current_modified = current.modified().map_err(|e| {
                format!("freeze {} read modification time before copy: {e}", relative.display())
            })?;
            if current.len() != before.len() || current_modified != before_modified {
                return Err("source changed before freezing movie; retry export".into());
            }
            let destination = assets.path().join(&relative);
            fs::create_dir_all(destination.parent().ok_or("invalid frozen source")?)
                .map_err(|e| e.to_string())?;
            let mut output = fs::File::create(&destination).map_err(|e| e.to_string())?;
            let mut copied = 0u64;
            loop {
                control.check()?;
                let count = file
                    .read(&mut buffer)
                    .map_err(|e| format!("freeze {}: {e}", relative.display()))?;
                if count == 0 {
                    break;
                }
                copied += count as u64;
                if copied > before.len() {
                    return Err("source grew while freezing movie; retry export".into());
                }
                output
                    .write_all(&buffer[..count])
                    .map_err(|e| e.to_string())?;
                done_bytes += count as u64;
                control.set_progress(done_bytes as f64 / total_bytes.max(1) as f64 * 0.1);
            }
            let after = file.metadata().map_err(|e| e.to_string())?;
            let after_modified = after.modified().map_err(|e| {
                format!("freeze {} read modification time after copy: {e}", relative.display())
            })?;
            if copied != before.len()
                || before.len() != after.len()
                || before_modified != after_modified
            {
                return Err("source changed while freezing movie; retry export".into());
            }
        }
        control.check()?;
        // Validate every referenced PCM and PTS cache before accepting the task.
        if include_audio { motion_media::AudioMixer::new(project.clone(), assets.path())?; }
        for asset in &project.video_assets {
            motion_media::load_video_index(assets.path(), asset)?;
        }
        source_paths.insert(root.join("project.json"));
        control.set_progress(0.1);
        Ok(FrozenMovie {
            project,
            registry,
            assets,
            memory_policy,
            allocation_budget,
            source_paths,
            revision,
        })
    }
}

impl FrozenMovie {
    /// Call on the session's owning worker. The private directory contains actual
    /// copies, including PCM and PTS caches; retained original file handles alone
    /// would not freeze an in-place external source edit.
    pub fn from_session(session: &Session) -> Result<Self> {
        Self::from_session_cancellable(session, &ExportControl::default())
    }

    pub fn from_session_cancellable(session: &Session, control: &ExportControl) -> Result<Self> {
        Self::capture_plan(session, control)?.freeze(control)
    }

    pub fn capture_plan(session: &Session, control: &ExportControl) -> Result<MovieFreezePlan> {
        session.check_thread()?;
        if !cfg!(feature = "ffmpeg") {
            return Err("MP4 export requires a build with --ffmpeg".into());
        }
        let mut plan = MovieFreezePlan::new(
            session.engine.snapshot(),
            &session.root,
            session.effects.registry.clone(),
            session.effects.memory_policy(),
            session.engine.revision(),
            control,
        )?;
        plan.allocation_budget = session.allocation_budget().clone();
        Ok(plan)
    }

    pub fn new(
        project: Project,
        root: &Path,
        registry: Registry,
        scratch_budget: u64,
        revision: u64,
    ) -> Result<Self> {
        Self::new_cancellable(
            project,
            root,
            registry,
            scratch_budget,
            revision,
            &ExportControl::default(),
        )
    }

    pub fn new_cancellable(
        project: Project,
        root: &Path,
        registry: Registry,
        scratch_budget: u64,
        revision: u64,
        control: &ExportControl,
    ) -> Result<Self> {
        MovieFreezePlan::new(project, root, registry, MemoryPolicy { scratch_budget_bytes: scratch_budget, ..Default::default() }, revision, control)?
            .freeze(control)
    }

    pub fn frames(&self) -> u32 {
        self.project.frames
    }

    #[cfg(not(feature = "ffmpeg"))]
    pub fn export(
        self,
        _output: &Path,
        _control: &ExportControl,
        _progress: impl FnMut(f64),
    ) -> Result<MovieResult> {
        Err("MP4 export requires a build with --ffmpeg".into())
    }

    #[cfg(feature = "ffmpeg")]
    pub fn export(
        self,
        output: &Path,
        control: &ExportControl,
        mut progress: impl FnMut(f64),
    ) -> Result<MovieResult> {
        use motion_render::Scene;
        use motion_render::{image_resources::Resolution, Renderer};
        use std::time::{Duration, Instant};
        control.check()?;
        let mut report = |value: f64| {
            control.set_progress(value);
            progress(value);
        };
        let output = export_destination(output, &self.source_paths)?;
        let temporary = tempfile::Builder::new()
            .prefix(".motion-movie-")
            .suffix(".mp4")
            .tempfile_in(output.parent().unwrap())
            .map_err(|e| e.to_string())?;
        let p = &self.project;
        let mut encoder =
            crate::encode::Mp4Encoder::new(temporary.path(), p.width, p.height, p.fps)?;
        let encoder_name = encoder.video_encoder.clone();
        let mut renderer =
            pollster::block_on(Renderer::headless_with_allocation_budget(self.allocation_budget.clone())).map_err(|e| format!("export GPU: {e}"))?;
        renderer.set_effect_registry(self.registry.clone());
        renderer
            .set_memory_policy(self.memory_policy)
            .map_err(|e| e.to_string())?;
        renderer
            .configure_assets(p, self.assets.path())
            .map_err(|e| e.to_string())?;
        let mut scene = Scene::new(p);
        let document = if motion_host::render_sampling::needs_exposure(p)? {
            Some(motion_render::exposure_scene::ExposureDocument::new(Arc::new(p.clone()))
                .map_err(|error| error.to_string())?)
        } else { None };
        let target = if document.is_none() {
            Some(renderer.capture_target(p.width, p.height).map_err(|error| error.to_string())?)
        } else { None };
        let mut proxies = motion_host::preview_video::PreviewVideoCache::default();
        let mut videos = motion_host::video_frames::VideoFrames::new(crate::platform());
        let mut mixer = motion_media::AudioMixer::new(p.clone(), self.assets.path())?;
        let total_samples = mixer.total_frames();
        let block_size = encoder.audio_frame_size();
        if block_size == 0 || block_size > motion_media::MAX_BLOCK_FRAMES {
            return Err("unsupported AAC frame size".into());
        }
        let mut pcm = vec![0f32; block_size * 2];
        let mut sample = 0;
        report(0.1);
        for frame in 0..p.frames {
            control.check()?;
            let pixels = if let Some(document) = &document {
                render_exposure_frame(&mut renderer, &mut videos, &mut proxies, p,
                    document.clone(), self.assets.path(), f64::from(frame), control)?
            } else {
                // Scene::sample performs the same animated, expression and recursive
                // composition evaluation used by the editing host.
                scene
                    .sample(p, f64::from(frame), None, &motion_core::ExpressionEvaluator)
                    .map_err(|e| e.to_string())?;
                renderer.retain_video_instances(&scene);
                renderer
                    .prepare_scene_assets(&scene, Resolution::Full, false)
                    .map_err(|e| e.to_string())?;
                let began = Instant::now();
                let decoded = loop {
                    control.check()?;
                    if let Some(frames) = videos.prepare_scene_exact(p, self.assets.path(), &scene)? {
                        break frames;
                    }
                    if began.elapsed() > Duration::from_secs(30) {
                        return Err("export video frame decode timed out".into());
                    }
                    std::thread::sleep(Duration::from_millis(2));
                };
                for (object, decoded) in decoded {
                    let source = scene
                        .video_layers()
                        .into_iter()
                        .find(|l| l.id == object)
                        .and_then(|l| l.video.as_ref())
                        .ok_or("export video instance missing")?
                        .asset;
                    decoded.upload(&mut renderer, object, source)?;
                }
                renderer
                    .capture(&scene, target.as_ref().ok_or("ordinary capture target missing")?)
                    .map_err(|e| format!("render export frame {frame}: {e}"))?.0
            };
            control.check()?;
            encoder.video_frame(&pixels, frame)?;
            let until = (u64::from(frame + 1) * motion_media::OUTPUT_RATE / u64::from(p.fps))
                .min(total_samples);
            while sample < until {
                control.check()?;
                let count = block_size.min((total_samples - sample) as usize);
                let actual = mixer.mix(sample, &mut pcm[..count * 2])?;
                if actual != count {
                    return Err("audio mixer ended before the frozen composition".into());
                }
                encoder.audio_frame(&pcm[..count * 2], sample)?;
                sample += count as u64;
            }
            report(0.1 + f64::from(frame + 1) / f64::from(p.frames) * 0.88);
        }
        if sample != total_samples {
            return Err("movie mix did not cover the complete composition".into());
        }
        control.check()?;
        encoder.finish()?;
        temporary.as_file().sync_all().map_err(|e| e.to_string())?;
        // Parse the completed container before making it visible at its destination.
        let input =
            ffmpeg_next::format::input(temporary.path()).map_err(|e| format!("verify MP4: {e}"))?;
        let video = input
            .streams()
            .best(ffmpeg_next::media::Type::Video)
            .ok_or("completed MP4 has no video")?;
        let audio = input
            .streams()
            .best(ffmpeg_next::media::Type::Audio)
            .ok_or("completed MP4 has no audio")?;
        if video.parameters().id() != ffmpeg_next::codec::Id::H264
            || audio.parameters().id() != ffmpeg_next::codec::Id::AAC
            || video.frames() != i64::from(p.frames)
        {
            return Err("completed MP4 stream verification failed".into());
        }
        drop(input);
        let bytes = temporary
            .as_file()
            .metadata()
            .map_err(|e| e.to_string())?
            .len();
        if bytes == 0 {
            return Err("completed MP4 is empty".into());
        }
        {
            let mut state = control
                .0
                .lock()
                .map_err(|_| "export cancellation state poisoned")?;
            if state.cancelled {
                return Err("movie export cancelled".into());
            }
            temporary
                .persist(&output)
                .map_err(|e| format!("publish MP4: {}", e.error))?;
            state.published = true;
        }
        report(1.0);
        Ok(MovieResult {
            path: output,
            bytes,
            width: p.width,
            height: p.height,
            frames: p.frames,
            fps: p.fps,
            revision: self.revision,
            video_encoder: encoder_name,
            audio_codec: "aac",
        })
    }
}

/// One frozen request retains its GPU phase while media is pending. The
/// existing decode deadline applies to each unchanged pending stage; no task
/// restart is performed on a poll or observation timeout.
fn render_exposure_frame(renderer: &mut motion_render::Renderer,
    videos: &mut motion_host::video_frames::VideoFrames,
    proxies: &mut motion_host::preview_video::PreviewVideoCache,
    project: &Project, document: motion_render::exposure_scene::ExposureDocument,
    root: &Path, frame: f64, control: &ExportControl) -> Result<Vec<u8>> {
    use motion_host::render_sampling::{ExposureMedia, ExposureMediaPolicy};
    use motion_render::{exposure_scene::ExposureScenes, ExposureFrame, ExposurePoll};
    use std::time::{Duration, Instant};
    let scenes = ExposureScenes::new(document, &project.composition_id, frame,
        false, None, motion_core::ExpressionEvaluator).map_err(|error| error.to_string())?;
    let mut task = ExposureFrame::new(renderer, scenes, project.width, project.height)
        .map_err(|error| format!("export frame={frame} phase=exposure_init: {error}"))?;
    let mut media = ExposureMedia { project, root, videos, proxies, policy: ExposureMediaPolicy::FULL };
    let mut progress = task.progress();
    let mut stage_started = Instant::now();
    loop {
        control.check()?;
        match task.poll(renderer, Duration::from_millis(8), |renderer, scene| media.prepare(renderer, scene))
            .map_err(|error| format!("export frame={frame} phase=exposure_poll: {error}"))? {
            ExposurePoll::Ready(_) => return renderer.capture_exposure(task.target())
                .map_err(|error| format!("export frame={frame} phase=exposure_capture: {error}")),
            ExposurePoll::Pending(current) => {
                if current != progress { progress = current; stage_started = Instant::now(); }
                if stage_started.elapsed() > Duration::from_secs(30) {
                    return Err(format!("export frame={frame} phase=exposure_media progress={progress:?}: video frame decode timed out"));
                }
                std::thread::sleep(Duration::from_millis(2));
            }
        }
    }
}

/// Resolve existing destinations before checking protected sources. An absent
/// destination is expected; a permission/path error must retain its cause.
#[cfg(any(feature = "ffmpeg", test))]
fn export_destination(output: &Path, source_paths: &BTreeSet<PathBuf>) -> Result<PathBuf> {
    export_destination_for(output, source_paths, "MP4")
}

fn export_destination_for(output: &Path, source_paths: &BTreeSet<PathBuf>, format: &str) -> Result<PathBuf> {
    let parent = output
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    let output = parent
        .canonicalize()
        .map_err(|e| e.to_string())?
        .join(output.file_name().ok_or("invalid export destination")?);
    let existing = match output.canonicalize() {
        Ok(existing) => existing,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => output.clone(),
        Err(error) => {
            return Err(format!("resolve {format} export destination {}: {error}", output.display()));
        }
    };
    if source_paths.contains(&existing) {
        return Err(format!("{format} export must not replace a project source"));
    }
    Ok(output)
}

#[cfg(test)]
mod freeze_integrity_tests {
    use super::*;
    use std::time::{Duration, SystemTime};

    fn project() -> Project {
        let mut project = Project::new(64, 64, 30, 6).unwrap();
        project.assets.push(motion_core::Asset {
            id: 1,
            path: "assets/image.png".into(),
            width: 1,
            height: 1,
        });
        project
    }
    fn plan(root: &Path) -> MovieFreezePlan {
        MovieFreezePlan::new(
            project(), root, Registry::new_with_builtins().unwrap(),
            MemoryPolicy::default(), 17, &ExportControl::default(),
        ).unwrap()
    }

    #[test]
    fn frozen_source_preserves_bytes_and_rejects_same_size_modified_source() {
        let root = tempfile::tempdir().unwrap();
        fs::create_dir(root.path().join("assets")).unwrap();
        let source = root.path().join("assets/image.png");
        fs::write(&source, [1, 2, 3, 4]).unwrap();
        let original = SystemTime::UNIX_EPOCH + Duration::from_secs(1_600_000_000);
        fs::File::options().write(true).open(&source).unwrap()
            .set_times(fs::FileTimes::new().set_modified(original)).unwrap();
        let frozen = plan(root.path()).freeze(&ExportControl::default()).unwrap();
        assert_eq!(frozen.revision, 17);
        assert_eq!(fs::read(frozen.assets.path().join("assets/image.png")).unwrap(), [1, 2, 3, 4]);

        let pending = plan(root.path());
        fs::write(&source, [4, 3, 2, 1]).unwrap();
        fs::File::options().write(true).open(&source).unwrap()
            .set_times(fs::FileTimes::new().set_modified(original + Duration::from_secs(60))).unwrap();
        assert_eq!(pending.freeze(&ExportControl::default()).err().as_deref(),
            Some("source changed before freezing movie; retry export"));
        assert_eq!(fs::read(frozen.assets.path().join("assets/image.png")).unwrap(), [1, 2, 3, 4]);
    }

    #[test]
    fn export_path_distinguishes_new_destination_protected_source_and_resolution_error() {
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("source.mp4");
        fs::write(&source, b"original source").unwrap();
        let protected = BTreeSet::from([source.canonicalize().unwrap()]);
        assert_eq!(export_destination(&source, &protected).err().as_deref(),
            Some("MP4 export must not replace a project source"));
        let new = root.path().join("new.mp4");
        assert_eq!(export_destination(&new, &protected).unwrap(),
            root.path().canonicalize().unwrap().join("new.mp4"));
        assert!(!new.exists());
        let invalid = root.path().join("invalid\0.mp4");
        let error = export_destination(&invalid, &protected).unwrap_err();
        assert!(error.contains("resolve MP4 export destination"), "{error}");
        assert_eq!(fs::read(&source).unwrap(), b"original source");
    }
}
