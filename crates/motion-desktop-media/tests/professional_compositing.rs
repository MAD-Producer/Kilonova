#![cfg(feature = "ffmpeg")]
//! Repository-authored motion-graphics shot through the public host and native
//! frozen exporters. This is headless output acceptance, not desktop UI capture.
use motion_core::{
    Axis, Command, EffectAction, EffectInstance, FrameRate, Layer, Project, Property,
};
use motion_desktop_media::movie::{frame::FrameFreezePlan, ExportControl, FrozenMovie};
use motion_host::Session;
use motion_render::{image_resources::Resolution, OutputTransform, Renderer};
use serde_json::json;
use std::{fs, path::Path};

const WIDTH: u32 = 320;
const HEIGHT: u32 = 180;
const FRAMES: u32 = 36;

fn commands() -> Vec<Command> {
    let package = motion_effects::builtin::package().unwrap();
    let definition = package
        .manifest
        .effects
        .iter()
        .find(|e| e.id == "brightness_contrast")
        .unwrap();
    let effect = EffectInstance::new(
        1,
        &package.manifest.id,
        &package.manifest.version,
        &package.hash,
        definition,
        [135., 92.],
    );
    let mut commands = vec![
        Command::Add {
            layer: Layer::camera(10, "Authored camera", WIDTH, HEIGHT),
        },
        Command::Add {
            layer: Layer::solid(
                8,
                "Far blue card",
                [220., 130.],
                [160., 90., 60.],
                [0.05, 0.22, 0.7, 1.],
            ),
        },
        Command::Add {
            layer: Layer::solid(
                7,
                "Near coral card",
                [135., 92.],
                [160., 90., -20.],
                [0.8, 0.1, 0.04, 1.],
            ),
        },
        Command::Add {
            layer: Layer::solid(
                9,
                "Gold depth marker",
                [40., 40.],
                [250., 120., -30.],
                [0.9, 0.6, 0.04, 1.],
            ),
        },
    ];
    for object in [7, 8, 9] {
        commands.push(Command::SetLayer3d {
            object,
            enabled: true,
        });
    }
    // Add appends to the painter-order stack. Put blue last so the flat
    // control occludes coral; actual spatial depth must reverse that result.
    commands.push(Command::Reorder {
        object: 8,
        index: 3,
    });
    commands.extend([
        Command::Effect {
            object: 7,
            action: EffectAction::Insert { instance: effect },
        },
        Command::Effect {
            object: 7,
            action: EffectAction::Set {
                effect: 1,
                param: "p0001".into(),
                frame: 0,
                value: [-10., 0., 0., 0.],
            },
        },
        Command::Effect {
            object: 7,
            action: EffectAction::Animate {
                effect: 1,
                param: "p0001".into(),
                frame: 0,
                enabled: true,
            },
        },
        Command::Effect {
            object: 7,
            action: EffectAction::Set {
                effect: 1,
                param: "p0001".into(),
                frame: 35,
                value: [25., 0., 0., 0.],
            },
        },
        Command::Animate {
            object: 10,
            property: Property::Position,
            axis: None,
            frame: 0,
            enabled: true,
        },
        Command::SetVector {
            object: 10,
            property: Property::Position,
            frame: 35,
            value: [200., 90., -217.27922],
        },
        Command::Animate {
            object: 7,
            property: Property::Rotation,
            axis: None,
            frame: 0,
            enabled: true,
        },
        Command::SetVector {
            object: 7,
            property: Property::Rotation,
            frame: 0,
            value: [0., -20., 0.],
        },
        Command::SetVector {
            object: 7,
            property: Property::Rotation,
            frame: 35,
            value: [0., 20., 0.],
        },
        Command::SeparateDimensions {
            object: 7,
            property: Property::Position,
        },
        Command::Animate {
            object: 7,
            property: Property::Position,
            axis: Some(Axis::Y),
            frame: 0,
            enabled: true,
        },
        Command::SetComponent {
            object: 7,
            property: Property::Position,
            axis: Axis::Y,
            frame: 35,
            value: 80.,
        },
    ]);
    commands
}

fn render(session: &mut Session, renderer: &mut Renderer, frame: f64) -> Vec<u8> {
    session.frame = frame;
    session.sample().unwrap();
    renderer.set_effect_registry(session.effects.registry.clone());
    renderer
        .configure_assets_with_scene(session.engine.project(), &session.root, &session.scene)
        .unwrap();
    assert!(renderer
        .prepare_scene_assets(&session.scene, Resolution::Full, false)
        .unwrap());
    let target = renderer
        .sdr_capture_target(WIDTH, HEIGHT, OutputTransform::SDR_CLIP)
        .unwrap();
    renderer
        .capture_sdr(&session.scene, &target)
        .unwrap()
        .0
        .pixels
}

fn write_png(path: &Path, pixels: &[u8]) {
    image::save_buffer(path, pixels, WIDTH, HEIGHT, image::ColorType::Rgba8).unwrap();
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn host_camera_effect_animation_matches_reopened_and_frozen_outputs_after_live_edits() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().join("project");
    fs::create_dir(&root).unwrap();
    let fps = FrameRate::new(30000, 1001).unwrap();
    let mut project = Project::new_with_frame_rate(WIDTH, HEIGHT, fps, FRAMES).unwrap();
    project.background = [0.01, 0.015, 0.025, 1.];
    let original = project.clone();
    let mut session =
        Session::new(project, root.clone(), motion_desktop_media::platform()).unwrap();
    let commands = commands();
    let command_text = serde_json::to_string_pretty(&commands).unwrap();
    session.apply_commands(&command_text).unwrap();
    let authored = session.engine.snapshot();
    let revision = session.engine.revision();
    assert!(authored
        .layers
        .iter()
        .find(|l| l.id == 7)
        .unwrap()
        .transform
        .position
        .axes
        .is_some());
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    let mut previews = Vec::new();
    let mut plans = Vec::new();
    for frame in [0., 17.5, 18., 35.] {
        previews.push(render(&mut session, &mut renderer, frame));
        plans.push((
            frame,
            FrameFreezePlan::capture(
                &session,
                Some(directory.path().join(format!("frozen-{frame}.png"))),
                &ExportControl::default(),
            )
            .unwrap(),
        ));
    }
    let center = ((HEIGHT / 2 * WIDTH + WIDTH / 2) * 4) as usize;
    let pixel = &previews[0][center..center + 4];
    assert!(
        pixel[0] > 100
            && u16::from(pixel[0]) > u16::from(pixel[1]) * 2
            && u16::from(pixel[0]) > u16::from(pixel[2]) * 2
            && pixel[3] == 255,
        "the actual near coral plane must cover the image center: {pixel:?}"
    );
    let changed_pixels = previews[0]
        .chunks_exact(4)
        .zip(previews[3].chunks_exact(4))
        .filter(|(a, b)| a[..3].iter().zip(&b[..3]).any(|(x, y)| x.abs_diff(*y) > 16))
        .count();
    assert!(
        changed_pixels > 4000,
        "animated camera/rotation/effect must visibly change the shot: {changed_pixels}"
    );
    let mut flat_control = Session::new(
        authored.clone(),
        root.clone(),
        motion_desktop_media::platform(),
    )
    .unwrap();
    flat_control
        .apply_command_batch(
            [7, 8, 9]
                .into_iter()
                .map(|object| Command::SetLayer3d {
                    object,
                    enabled: false,
                })
                .collect(),
        )
        .unwrap();
    let flat_pixels = render(&mut flat_control, &mut renderer, 0.);
    let flat_center = &flat_pixels[center..center + 4];
    assert!(
        flat_center[2] > 100 && u16::from(flat_center[2]) > u16::from(flat_center[0]) * 2,
        "the unchanged layer stack must put blue above coral in the 2D control: {flat_center:?}"
    );
    assert_ne!(
        flat_pixels, previews[0],
        "3D depth, not layer order, determines the coral occluder"
    );
    drop(flat_control);
    session.engine.undo().unwrap();
    assert_eq!(
        session.engine.project(),
        &original,
        "the entire real API batch is one undo step"
    );
    assert_ne!(render(&mut session, &mut renderer, 18.), previews[2]);
    session.engine.redo().unwrap();
    assert_eq!(session.engine.project(), &authored);
    assert_eq!(render(&mut session, &mut renderer, 18.), previews[2]);
    motion_core::storage::save(&root, &authored).unwrap();
    let reopened = motion_core::storage::load(&root).unwrap();
    assert_eq!(reopened, authored);
    let mut reopened =
        Session::new(reopened, root.clone(), motion_desktop_media::platform()).unwrap();
    for (index, frame) in [0., 17.5, 18., 35.].into_iter().enumerate() {
        assert_eq!(
            render(&mut reopened, &mut renderer, frame),
            previews[index],
            "reopen frame {frame}"
        );
    }
    let movie_references: Vec<_> = (0..FRAMES)
        .map(|frame| render(&mut reopened, &mut renderer, f64::from(frame)))
        .collect();
    let movie_revision = session.engine.revision();
    let frozen = FrozenMovie::from_session(&session).unwrap();
    session
        .apply_command_batch(vec![
            Command::SetVector {
                object: 10,
                property: Property::Position,
                frame: 35,
                value: [160., 90., -600.],
            },
            Command::Delete { object: 7 },
        ])
        .unwrap();
    assert_ne!(render(&mut session, &mut renderer, 35.), previews[3]);
    let live_after = session.engine.snapshot();
    drop(reopened);
    drop(session);
    let mut frozen_paths = Vec::new();
    for ((frame, plan), expected) in plans.into_iter().zip(&previews) {
        let result = plan.export(&ExportControl::default()).unwrap();
        assert_eq!(result["revision"], revision);
        let path = std::path::PathBuf::from(result["path"].as_str().unwrap());
        let image = image::open(&path).unwrap().into_rgba8();
        assert_eq!(image.dimensions(), (WIDTH, HEIGHT));
        assert_eq!(image.as_raw(), expected, "frozen PNG frame {frame}");
        frozen_paths.push(path);
    }
    let movie = directory.path().join("camera-effect.mp4");
    let control = ExportControl::default();
    let result = frozen.export(&movie, &control, |_| {}).unwrap();
    assert_eq!(result.frames, FRAMES);
    assert_eq!(result.fps, fps);
    assert_eq!(result.revision, movie_revision);
    assert!(result.bytes > 0);
    assert_eq!(control.progress(), 1.);
    let mut input = ffmpeg_next::format::input(&movie).unwrap();
    let video = input
        .streams()
        .best(ffmpeg_next::media::Type::Video)
        .unwrap();
    let rate = video.avg_frame_rate();
    assert_eq!(
        i64::from(rate.numerator()) * 1001,
        i64::from(rate.denominator()) * 30000
    );
    let video_index = video.index();
    let timebase = video.time_base();
    assert_eq!(video.frames(), i64::from(FRAMES));
    let mut packet_count = 0;
    for (stream, packet) in input.packets() {
        if stream.index() != video_index {
            continue;
        }
        let pts = packet.pts().unwrap();
        assert_eq!(packet.dts(), Some(pts));
        assert_eq!(
            i128::from(pts) * i128::from(timebase.numerator()) * 30000,
            i128::from(packet_count) * 1001 * i128::from(timebase.denominator())
        );
        assert_eq!(
            i128::from(packet.duration()) * i128::from(timebase.numerator()) * 30000,
            1001 * i128::from(timebase.denominator())
        );
        packet_count += 1;
    }
    assert_eq!(packet_count, FRAMES);
    let probe = motion_desktop_media::av::probe(&movie, None, None, &|| Ok(())).unwrap();
    assert_eq!(probe.asset.frame_count, FRAMES);
    assert_eq!(probe.timestamps.len(), FRAMES as usize);
    let timestamps = probe.timestamps.clone();
    let mut decoder =
        motion_desktop_media::av::Decoder::open(&movie, probe.asset, probe.timestamps).unwrap();
    let mut comparisons = Vec::new();
    for (frame, expected) in movie_references.iter().enumerate() {
        let decoded = decoder.frame(timestamps[frame], &|| Ok(())).unwrap();
        let rgba = decoded.rgba().unwrap();
        assert_eq!(rgba.len(), (WIDTH * HEIGHT * 4) as usize);
        let rgb_errors: Vec<_> = rgba
            .chunks_exact(4)
            .zip(expected.chunks_exact(4))
            .flat_map(|(a, b)| (0..3).map(move |c| a[c].abs_diff(b[c])))
            .collect();
        let mean = rgb_errors.iter().map(|&v| f64::from(v)).sum::<f64>() / rgb_errors.len() as f64;
        // Native movie output is lossy YUV420. PNG equality above is exact;
        // this separate, declared RGB mean budget includes codec/chroma error.
        assert!(
            mean <= 3.,
            "decoded movie frame {frame}: RGB mean error {mean}"
        );
        comparisons.push(
            json!({"frame":frame,"rgbMeanAbsoluteError":mean,"maximum":rgb_errors.iter().max()}),
        );
    }
    if let Some(output) = std::env::var_os("MOTION_PROFESSIONAL_API_ARTIFACTS") {
        let output = std::path::PathBuf::from(output);
        fs::create_dir_all(&output).unwrap();
        fs::copy(&movie, output.join("camera-effect.mp4")).unwrap();
        fs::copy(root.join("project.json"), output.join("project.json")).unwrap();
        fs::write(
            output.join("initial-project.json"),
            serde_json::to_vec_pretty(&original).unwrap(),
        )
        .unwrap();
        write_png(&output.join("flat-control.png"), &flat_pixels);
        fs::write(output.join("commands.json"), &command_text).unwrap();
        fs::write(
            output.join("live-after.json"),
            serde_json::to_vec_pretty(&live_after).unwrap(),
        )
        .unwrap();
        for (index, (frame, frozen)) in [0., 17.5, 18., 35.]
            .into_iter()
            .zip(&frozen_paths)
            .enumerate()
        {
            write_png(
                &output.join(format!("headless-{frame}.png")),
                &previews[index],
            );
            fs::copy(frozen, output.join(format!("frozen-{frame}.png"))).unwrap();
        }
        for (frame, pixels) in movie_references.iter().enumerate() {
            write_png(
                &output.join(format!("movie-reference-{frame:03}.png")),
                pixels,
            );
        }
        fs::write(output.join("result.json"), serde_json::to_vec_pretty(&json!({
            "fixture":{"author":"Motion Studio repository contributors","license":"MIT","source":"Procedural solid planes, authored camera and built-in brightness/contrast","externalMedia":false},
            "scope":"Public host API, atomic undo/redo, save/reopen, headless native scene output, independent frozen PNG and MP4 after session drop",
            "limits":["No desktop HWND/UI capture","No actual photographed shot or independent image golden","No masks/mattes/precompose/relink or compiled node graph proof","Default 8-bit working precision only"],
            "width":WIDTH,"height":HEIGHT,"frames":FRAMES,"fps":fps,"revision":revision,"movieRevision":movie_revision,
            "changedPixels":changed_pixels,"depthCenter":pixel,"flatCenter":flat_center,
            "pngExact":true,"movie":result,"exactVideoPackets":packet_count,"decodedComparisons":comparisons
        })).unwrap()).unwrap();
    }
}
