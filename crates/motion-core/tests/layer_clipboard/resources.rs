use super::*;
use motion_core::{
    compositing::{MatteMode, TrackMatte},
    AudioAsset, EffectImageInput, EffectImageStage, EffectInstance, FontAsset, VideoAsset,
    VideoClip,
};

fn effect() -> EffectInstance {
    let package = motion_effects::builtin::package().unwrap();
    EffectInstance::new(
        1,
        &package.manifest.id,
        &package.manifest.version,
        &package.hash,
        package
            .manifest
            .effects
            .iter()
            .find(|e| e.id == "displacement_map")
            .unwrap(),
        [80.; 2],
    )
}

#[test]
fn internal_mattes_and_effect_inputs_remap_and_cut_cannot_break_an_external_matte() {
    let mut project = source().snapshot();
    project.layers[1].track_matte = Some(TrackMatte {
        source: 1,
        mode: MatteMode::Alpha,
        hide_source: true,
    });
    let mut effect = effect();
    effect.image_input = Some(EffectImageInput::Layer {
        layer: 1,
        stage: EffectImageStage::Source,
    });
    project.layers[1].effects.push(effect.clone());
    project.rebuild_plugin_dependencies();
    project.validate().unwrap();
    let clipboard = LayerClipboard::capture(&project, vec![1, 9]).unwrap();
    let mut target = Project::new(640, 360, 30, 120).unwrap();
    target
        .layers
        .push(Layer::solid(90, "Existing", [80.; 2], [0.; 3], [1.; 4]));
    let mut target = Engine::new(target).unwrap();
    target.apply(paste(clipboard, None, false)).unwrap();
    let pasted = &target.project().layers[2];
    assert_eq!(pasted.track_matte.unwrap().source, 91);
    assert_eq!(pasted.parent.as_ref().unwrap().object, Some(91));
    effect.image_input = Some(EffectImageInput::Layer {
        layer: 91,
        stage: EffectImageStage::Source,
    });
    assert_eq!(pasted.effects, [effect]);
    assert_eq!(
        target.project().plugin_dependencies,
        project.plugin_dependencies
    );

    let mut source = Engine::new(project.clone()).unwrap();
    let error = source
        .apply(Command::RemoveLayers {
            objects: vec![1],
            frame: 30,
        })
        .unwrap_err();
    assert!(error
        .to_string()
        .contains("unselected track matte: object=9"));
    assert_eq!(source.project(), &project);
    assert_eq!(source.revision(), 0);
    assert!(!source.can_undo());
    source
        .apply(Command::RemoveLayers {
            objects: vec![1, 9],
            frame: 30,
        })
        .unwrap();
    assert!(source.project().layers.is_empty());
    source.undo().unwrap();
    assert_eq!(source.project(), &project);
}

#[test]
fn text_fonts_rasters_and_effect_images_follow_only_the_selected_resource_closure() {
    let mut project = Project::new(640, 360, 30, 120).unwrap();
    project.assets = vec![
        Asset {
            id: 10,
            path: "assets/input.png".into(),
            width: 80,
            height: 80,
        },
        Asset {
            id: 20,
            path: "assets/text.png".into(),
            width: 80,
            height: 80,
        },
        Asset {
            id: 30,
            path: "assets/unused.png".into(),
            width: 80,
            height: 80,
        },
    ];
    project.fonts.push(FontAsset {
        id: "fixture-font".into(),
        path: "assets/fonts/source.ttf".into(),
        name: "Metadata fixture".into(),
        face_index: 0,
        license: "No font bytes in this test".into(),
    });
    let mut layer = Layer::solid(1, "Title", [80.; 2], [0.; 3], [1.; 4]);
    layer.content = Content::Text {
        text: "Clipboard text".into(),
        font: "fixture-font".into(),
        color: [1.; 4],
        raster_asset: 20,
    };
    let mut effect = effect();
    effect.image_input = Some(EffectImageInput::Asset { asset: 10 });
    layer.effects.push(effect);
    project.layers.push(layer);
    project.rebuild_plugin_dependencies();
    let clipboard = LayerClipboard::capture(&project, vec![1]).unwrap();
    assert_eq!(
        clipboard
            .resolve()
            .unwrap()
            .assets
            .iter()
            .map(|a| a.id)
            .collect::<Vec<_>>(),
        [10, 20]
    );
    let mut target = Project::new(640, 360, 30, 120).unwrap();
    target.assets.push(Asset {
        id: 50,
        path: "assets/existing.png".into(),
        width: 80,
        height: 80,
    });
    let mut existing_font = project.fonts[0].clone();
    existing_font.path = "assets/fonts/existing.ttf".into();
    target.fonts.push(existing_font);
    let mut target = Engine::new(target).unwrap();
    target.apply(paste(clipboard.clone(), None, false)).unwrap();
    assert_eq!(target.project().assets.len(), 3);
    assert_eq!(target.project().fonts.len(), 2);
    assert_eq!(target.project().fonts[1].id, "clipboard-font-1");
    assert_eq!(
        target.project().layers[0].content,
        Content::Text {
            text: "Clipboard text".into(),
            font: "clipboard-font-1".into(),
            color: [1.; 4],
            raster_asset: 52,
        }
    );
    assert_eq!(
        target.project().layers[0].effects[0].image_input,
        Some(EffectImageInput::Asset { asset: 51 })
    );
    target.apply(paste(clipboard, None, false)).unwrap();
    assert_eq!(
        target.project().assets.len(),
        3,
        "identical registered resources are reused"
    );
    assert_eq!(target.project().fonts.len(), 2);
    assert_eq!(
        target.project().layers[1].content,
        target.project().layers[0].content
    );
}

#[test]
fn muted_video_keeps_its_recoverable_audio_and_remaps_both_shared_source_ids() {
    let mut project = Project::new(640, 360, 30, 120).unwrap();
    project.audio_assets.push(AudioAsset {
        id: 1,
        path: "assets/clip.mp4".into(),
        bytes: 64,
        mime: "audio/mp4".into(),
        track: 0,
        sample_rate: 48_000,
        channels: 2,
        sample_frames: 192_000,
        duration_us: 4_000_000,
    });
    project.video_assets.push(VideoAsset {
        id: 2,
        path: "assets/clip.mp4".into(),
        bytes: 64,
        mime: "video/avc".into(),
        track: 1,
        width: 80,
        height: 80,
        rotation: 0,
        display_width: 80,
        display_height: 80,
        video_start_us: 0,
        video_end_us: 4_000_000,
        duration_us: 4_000_000,
        frame_count: 120,
        variable_frame_rate: false,
        nominal_frame_rate: 30.,
        color_standard: 1,
        color_range: 1,
        audio_asset: Some(1),
    });
    let mut layer = Layer::solid(7, "Muted picture", [80.; 2], [0.; 3], [1.; 4]);
    let mut video = VideoClip::new(2);
    video.muted = true;
    video.source_offset_us = 250_000;
    video.volume = 0.75;
    layer.content = Content::Video { video };
    layer.timeline = Some(LayerTimeline {
        in_frame: 0,
        out_frame: 90,
        offset_frame: 0,
    });
    project.layers.push(layer);
    let clipboard = LayerClipboard::capture(&project, vec![7]).unwrap();
    assert_eq!(clipboard.resolve().unwrap().audio_assets.len(), 1);
    let mut target = Project::new(640, 360, 30, 120).unwrap();
    target.assets.push(Asset {
        id: 50,
        path: "assets/existing.png".into(),
        width: 80,
        height: 80,
    });
    let mut target = Engine::new(target).unwrap();
    target.apply(paste(clipboard, None, false)).unwrap();
    assert_eq!(target.project().audio_assets[0].id, 51);
    assert_eq!(target.project().video_assets[0].id, 52);
    assert_eq!(target.project().video_assets[0].audio_asset, Some(51));
    assert_eq!(
        target.project().audio_assets[0].path,
        target.project().video_assets[0].path
    );
    let Content::Video { video } = &target.project().layers[0].content else {
        panic!("video lost")
    };
    assert_eq!(video.asset, 52);
    assert!(video.muted && video.volume == 0.75 && video.source_offset_us == 250_000);
    assert!(target
        .project()
        .assets
        .iter()
        .all(|a| a.path != "assets/clip.mp4"));
}

#[test]
fn exhausted_ids_and_stale_or_outside_targets_are_atomic() {
    let clipboard = LayerClipboard::capture(source().project(), vec![1, 9]).unwrap();
    let mut project = Project::new(640, 360, 30, 120).unwrap();
    project.layers.push(Layer::solid(
        u64::MAX,
        "Last ID",
        [80.; 2],
        [0.; 3],
        [1.; 4],
    ));
    let mut target = Engine::new(project.clone()).unwrap();
    assert!(target
        .apply(paste(clipboard.clone(), None, false))
        .unwrap_err()
        .to_string()
        .contains("ID space exhausted"));
    assert_eq!(target.project(), &project);
    assert_eq!(target.revision(), 0);
    assert!(!target.can_undo());
    assert!(target
        .apply(paste(clipboard, Some(120), false))
        .unwrap_err()
        .to_string()
        .contains("outside composition"));
    assert_eq!(target.project(), &project);
    assert_eq!(target.revision(), 0);
    assert!(target
        .apply(Command::RemoveLayers {
            objects: vec![42],
            frame: 0
        })
        .is_err());
    assert_eq!(target.project(), &project);
    assert!(!target.can_undo());
}
