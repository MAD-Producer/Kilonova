use motion_scene::{
    import_frozen,
    obj::{import_obj, Handedness, MapKind, MaterialColorSpace, ObjOptions, UpAxis},
    AssetResolver, Cause, DirectoryResolver, ImportContext, ImportLimits, MaterialModel,
    ResourceKind, Scene, SourceFormat,
};
use std::{collections::BTreeMap, io};

const CTX: ImportContext = ImportContext {
    request: 87,
    source: 9,
};
const TRI: &str = "v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3\n";
struct Assets(BTreeMap<String, Vec<u8>>);
impl AssetResolver for Assets {
    fn read(&mut self, path: &str, _: ResourceKind, limit: usize) -> io::Result<Vec<u8>> {
        let bytes = self
            .0
            .get(path)
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "original fixture missing"))?;
        if bytes.len() > limit {
            return Err(io::Error::other("fixture resource exceeds limit"));
        }
        Ok(bytes.clone())
    }
}
fn load(source: &str) -> motion_scene::Result<Scene> {
    import_obj(
        CTX,
        source.as_bytes(),
        &mut Assets(BTreeMap::new()),
        &ObjOptions::default(),
        &ImportLimits::default(),
    )
}
fn material_assets(mtl: &str) -> Assets {
    Assets(BTreeMap::from([
        ("mats/scene.mtl".into(), mtl.as_bytes().to_vec()),
        ("textures/check%20er.png".into(), png()),
    ]))
}
fn png() -> Vec<u8> {
    // Independently encode an opaque white pixel with an uncompressed DEFLATE block.
    fn chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
        out.extend((data.len() as u32).to_be_bytes());
        out.extend(kind);
        out.extend(data);
        let mut crc = u32::MAX;
        for &byte in kind.iter().chain(data) {
            crc ^= byte as u32;
            for _ in 0..8 {
                crc = (crc >> 1) ^ if crc & 1 != 0 { 0xedb88320 } else { 0 };
            }
        }
        out.extend((!crc).to_be_bytes());
    }
    let mut out = b"\x89PNG\r\n\x1a\n".to_vec();
    chunk(&mut out, b"IHDR", &[0, 0, 0, 1, 0, 0, 0, 1, 8, 6, 0, 0, 0]);
    let pixels = [0u8, 255, 255, 255, 255];
    let mut a = 1u32;
    let mut b = 0u32;
    for p in pixels {
        a = (a + p as u32) % 65521;
        b = (b + a) % 65521;
    }
    let mut zlib = vec![0x78, 0x01, 1, 5, 0, 250, 255];
    zlib.extend(pixels);
    zlib.extend(((b << 16) | a).to_be_bytes());
    chunk(&mut out, b"IDAT", &zlib);
    chunk(&mut out, b"IEND", &[]);
    out
}
fn mtl_source() -> String {
    format!("mtllib mats/scene.mtl\nusemtl White\n{TRI}")
}

#[test]
fn triangle_has_owned_geometry_and_no_invented_normals() {
    let scene = load(TRI).unwrap();
    assert_eq!(scene.source_format(), SourceFormat::Obj);
    let p = &scene.meshes()[0].primitives[0];
    assert_eq!(p.positions, [[0., 0., 0.], [1., 0., 0.], [0., 1., 0.]]);
    assert_eq!(p.indices, [0, 1, 2]);
    assert!(p.normals.is_none());
    assert_eq!(scene.instances(0, 100.).unwrap().len(), 1);
}
#[test]
fn negative_indices_uv_seams_authored_normals_and_continuations() {
    let scene=load("\u{feff}v 0 0 0\r\nv 1 0 0\r\nv 0 1 0\r\nvt 0 .25\nvt 1 .25\nvt 0 .75\nvn 0 0 2\nf -3/-3/-1 \\\n -2/-2/-1 -1/-1/-1 # comment\n").unwrap();
    let p = &scene.meshes()[0].primitives[0];
    assert_eq!(
        p.texcoords[0].as_ref().unwrap(),
        &[[0., 0.75], [1., 0.75], [0., 0.25]]
    );
    assert_eq!(p.normals.as_ref().unwrap(), &[[0., 0., 1.]; 3]);
}
#[test]
fn concave_polygon_triangulation_preserves_area_and_winding() {
    // This concave L cannot be triangulated correctly with an arbitrary fan.
    let scene =
        load("v 0 0 0\nv 2 0 0\nv 2 1 0\nv 1 1 0\nv 1 2 0\nv 0 2 0\nf 1 2 3 4 5 6\n").unwrap();
    let p = &scene.meshes()[0].primitives[0];
    assert_eq!(p.indices.len(), 12);
    let mut area = 0.;
    for t in p.indices.chunks_exact(3) {
        let a = glam::Vec3::from(p.positions[t[0] as usize]);
        let b = glam::Vec3::from(p.positions[t[1] as usize]);
        let c = glam::Vec3::from(p.positions[t[2] as usize]);
        let n = (b - a).cross(c - a);
        assert!(n.z > 0.);
        area += n.z * 0.5;
    }
    assert_eq!(area, 3.);
}
#[test]
fn clockwise_faces_remain_clockwise() {
    let scene = load(&TRI.replace("f 1 2 3", "f 3 2 1")).unwrap();
    let p = &scene.meshes()[0].primitives[0];
    let [a, b, c] = [p.positions[0], p.positions[1], p.positions[2]].map(glam::Vec3::from);
    assert!((b - a).cross(c - a).z < 0.);
}
#[test]
fn objects_groups_material_splits_and_smoothing_survive() {
    let source="v 0 0 0\nv 1 0 0\nv 0 1 0\nv 0 0 1\no Corner\ng surface detail\ns 7\nf 1 2 3\ng other\nf 1 4 2\no Separate\ns off\nf 1 2 3\n";
    let scene = load(source).unwrap();
    assert_eq!(scene.meshes().len(), 2);
    let metadata = scene.obj_metadata().unwrap();
    assert_eq!(metadata.primitives[0].object, "Corner");
    assert_eq!(metadata.primitives[0].groups, ["surface", "detail"]);
    assert_eq!(metadata.primitives[0].smoothing_group, Some(7));
    assert_eq!(metadata.primitives[1].groups, ["other"]);
    let n = scene.meshes()[0].primitives[0].normals.as_ref().unwrap()[0];
    let expected = std::f32::consts::FRAC_1_SQRT_2;
    assert!((n[1] - expected).abs() < 1e-6 && (n[2] - expected).abs() < 1e-6);
    assert!(scene.meshes()[1].primitives[0].normals.is_none());
}
#[test]
fn smoothing_does_not_split_at_uv_seams() {
    let scene = load(
        "v 0 0 0\nv 1 0 0\nv 0 1 0\nv 0 0 1\nvt 0 0\nvt 1 1\ns 1\nf 1/1 2/1 3/1\nf 1/2 4/2 2/2\n",
    )
    .unwrap();
    let p = &scene.meshes()[0].primitives[0];
    let shared: Vec<_> = p
        .positions
        .iter()
        .enumerate()
        .filter(|(_, v)| **v == [0.; 3])
        .map(|(i, _)| p.normals.as_ref().unwrap()[i])
        .collect();
    assert_eq!(shared.len(), 2);
    assert_eq!(shared[0], shared[1]);
}
#[test]
fn explicit_units_axes_and_handedness_transform_normals_with_winding() {
    let options = ObjOptions {
        metres_per_unit: 0.01,
        up_axis: UpAxis::Z,
        handedness: Handedness::Left,
        flip_v: false,
        ..ObjOptions::default()
    };
    let scene = import_obj(
        CTX,
        b"v 0 0 0\nv 100 0 0\nv 0 100 0\nvn 0 0 1\nf 1//1 2//1 3//1",
        &mut Assets(BTreeMap::new()),
        &options,
        &ImportLimits::default(),
    )
    .unwrap();
    let p = &scene.meshes()[0].primitives[0];
    let [a, b, c] = [p.positions[0], p.positions[1], p.positions[2]].map(glam::Vec3::from);
    let n = glam::Vec3::from(p.normals.as_ref().unwrap()[0]);
    assert!(((b - a).cross(c - a).normalize() - n).length() < 1e-6);
    assert_eq!(n.to_array(), [0., -1., 0.]);
    assert!(p.positions.contains(&[1., 0., 0.]));
    assert!(p.positions.contains(&[0., 0., -1.]));
    assert_eq!(scene.obj_metadata().unwrap().options, options);
}
#[test]
fn homogeneous_vertices_and_explicit_rgb_extension() {
    let scene = load("v 0 0 0 2\nv 2 0 0 2\nv 0 2 0 2\nf 1 2 3").unwrap();
    assert_eq!(scene.meshes()[0].primitives[0].positions[1], [1., 0., 0.]);
    let scene = load("v 0 0 0 1 0 0\nv 1 0 0 0 1 0\nv 0 1 0 0 0 1\nf 1 2 3").unwrap();
    assert_eq!(
        scene.meshes()[0].primitives[0].colors.as_ref().unwrap()[2],
        [0., 0., 1., 1.]
    );
}
#[test]
fn classic_mtl_and_map_options_are_preserved_without_fake_pbr() {
    let mtl="newmtl White\nKa .1 .2 .3\nKd .4 .5 .6\nKs 1 1 1\nKe 2 0 0\nTf .2 .4 .6\nNs 120\nNi 1.5\nd -halo .7\nillum 7\nmap_Kd -o .1 .2 -s 2 3 -clamp on ../textures/check%20er.png\nbump -bm .25 -imfchan r ../textures/check%20er.png\n";
    let scene = import_obj(
        CTX,
        mtl_source().as_bytes(),
        &mut material_assets(mtl),
        &ObjOptions::default(),
        &ImportLimits::default(),
    )
    .unwrap();
    assert_eq!(scene.images().len(), 1);
    assert_eq!(scene.textures().len(), 2);
    assert_eq!(scene.extra_resources().len(), 1);
    let mat = &scene.materials()[0];
    assert!(mat.base_color_texture.is_none());
    assert!(mat.normal_texture.is_none());
    let MaterialModel::Wavefront(m) = &mat.model else {
        panic!("classic material silently converted")
    };
    assert_eq!(m.specular_exponent, Some(120.));
    assert_eq!(m.optical_density, Some(1.5));
    assert!(m.dissolve_halo);
    assert_eq!(m.illumination, Some(7));
    let map = &m.maps[&MapKind::Diffuse];
    assert_eq!(map.offset, [0.1, 0.2, 0.]);
    assert_eq!(map.scale, [2., 3., 1.]);
    assert!(map.clamp);
    assert_eq!(m.maps[&MapKind::Bump].bump_multiplier, 0.25);
    assert_eq!(m.maps[&MapKind::Bump].channel, Some('r'));
    assert_eq!(
        scene.images()[0].resource.locator.as_deref(),
        Some("textures/check%20er.png")
    );
}
#[test]
fn material_color_interpretation_is_explicit_and_authoring_values_survive() {
    let options = ObjOptions {
        material_color_space: MaterialColorSpace::Srgb,
        ..ObjOptions::default()
    };
    let scene = import_obj(
        CTX,
        mtl_source().as_bytes(),
        &mut material_assets("newmtl White\nKd .5 .5 .5\nTr .25"),
        &options,
        &ImportLimits::default(),
    )
    .unwrap();
    assert!((scene.materials()[0].base_color[0] - 0.21404114).abs() < 1e-6);
    assert_eq!(scene.materials()[0].base_color[3], 0.75);
    let MaterialModel::Wavefront(m) = &scene.materials()[0].model else {
        panic!()
    };
    assert_eq!(m.diffuse, Some([0.5; 3]));
    let error = import_obj(
        CTX,
        mtl_source().as_bytes(),
        &mut material_assets("newmtl White\nKe 3e38 0 0"),
        &options,
        &ImportLimits::default(),
    )
    .unwrap_err();
    assert!(error
        .to_string()
        .contains("material color conversion overflow"));
}
#[test]
fn real_files_are_owned_and_frozen_scene_survives_deleting_all_originals() {
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(root.path().join("mats")).unwrap();
    std::fs::create_dir_all(root.path().join("textures")).unwrap();
    std::fs::write(
        root.path().join("mats/scene.mtl"),
        "newmtl White\nKd .2 .4 .8\nmap_Kd ../textures/check%20er.png",
    )
    .unwrap();
    std::fs::write(root.path().join("textures/check%20er.png"), png()).unwrap();
    let source = mtl_source();
    let options = ObjOptions {
        metres_per_unit: 2.,
        ..ObjOptions::default()
    };
    let scene = import_obj(
        CTX,
        source.as_bytes(),
        &mut DirectoryResolver::new(root.path()).unwrap(),
        &options,
        &ImportLimits::default(),
    )
    .unwrap();
    let frozen = scene.freeze(source.as_bytes(), 1 << 20).unwrap();
    assert_eq!(scene.freeze(source.as_bytes(), 1 << 20).unwrap(), frozen);
    root.close().unwrap();
    assert_eq!(
        scene,
        import_frozen(CTX, &frozen, 1 << 20, &ImportLimits::default()).unwrap()
    );
}
#[test]
fn frozen_obj_rejects_option_source_resource_damage_and_truncation() {
    let source = mtl_source();
    let scene = import_obj(
        CTX,
        source.as_bytes(),
        &mut material_assets("newmtl White\nmap_Kd ../textures/check%20er.png"),
        &ObjOptions::default(),
        &ImportLimits::default(),
    )
    .unwrap();
    let frozen = scene.freeze(source.as_bytes(), 1 << 20).unwrap();
    let length = u32::from_le_bytes(frozen[8..12].try_into().unwrap()) as usize;
    for offset in [44, 44 + length + 8 + 32, frozen.len() - 1] {
        let mut broken = frozen.clone();
        broken[offset] ^= 1;
        assert!(
            import_frozen(CTX, &broken, 1 << 20, &ImportLimits::default()).is_err(),
            "offset {offset}"
        );
    }
    for length in [0, 7, 12, 40, frozen.len() - 1] {
        assert!(import_frozen(CTX, &frozen[..length], 1 << 20, &ImportLimits::default()).is_err());
    }
    assert!(scene.freeze(b"other", 1 << 20).is_err());
    assert!(scene.freeze(source.as_bytes(), 20).is_err());
}
#[test]
fn malformed_indices_layouts_and_nonfinite_numbers_fail_with_lines() {
    for tail in [
        "f 0 2 3",
        "f 1 2 4",
        "f -4 2 3",
        "f 1/ 2/ 3/",
        "f 1// 2 3",
        "f 1/1 2 3",
        "f 1 2",
        "v NaN 0 0",
        "v 0 0 0 0",
        "vn 0 0 0",
        "vt 0 0 1",
        "s -1",
    ] {
        let source = format!("v 0 0 0\nv 1 0 0\nv 0 1 0\n{tail}\n");
        let error = load(&source).unwrap_err();
        assert_eq!(error.object, "4", "{tail}: {error}");
    }
}
#[test]
fn self_intersections_degeneracy_and_unsupported_directives_are_explicit() {
    for source in [
        "v 0 0 0\nv 1 1 0\nv 0 1 0\nv 1 0 0\nf 1 2 3 4",
        "v 0 0 0\nv 1 0 0\nv 2 0 0\nf 1 2 3",
    ] {
        assert_eq!(load(source).unwrap_err().phase, "obj_polygon");
    }
    for directive in [
        "call other.obj",
        "csh del files",
        "curv 0 1 1 2 3",
        "l 1 2",
        "p 1",
    ] {
        let error = load(&format!("{TRI}{directive}")).unwrap_err();
        assert!(matches!(error.cause, Cause::Unsupported(_)));
    }
}
#[test]
fn nonplanar_faces_keep_positions_and_expose_projection_count() {
    let scene = load("v 0 0 0\nv 1 0 0\nv 1 1 .1\nv 0 1 0\nf 1 2 3 4").unwrap();
    assert_eq!(scene.obj_metadata().unwrap().projected_nonplanar_faces, 1);
    assert_eq!(scene.meshes()[0].primitives[0].indices.len(), 6);
}
#[test]
fn resource_escapes_unknown_mtl_and_missing_materials_do_not_silently_fallback() {
    for path in [
        "../escape.mtl",
        "/abs.mtl",
        "C:/secret.mtl",
        "https://host/x",
        "\\\\server\\x",
    ] {
        assert_eq!(
            load(&format!("mtllib {path}\n{TRI}")).unwrap_err().phase,
            "obj_path"
        );
    }
    for mtl in [
        "newmtl White\nmap_Kd ../../escape.png",
        "newmtl White\nmap_Kd -unknown x.png",
        "newmtl White\nd .1\nTr .1",
        "newmtl White\nnewmtl White",
        "newmtl White\nPr .5",
    ] {
        assert!(
            import_obj(
                CTX,
                mtl_source().as_bytes(),
                &mut material_assets(mtl),
                &ObjOptions::default(),
                &ImportLimits::default()
            )
            .is_err(),
            "{mtl}"
        );
    }
    assert!(load(&format!("usemtl missing\n{TRI}"))
        .unwrap_err()
        .to_string()
        .contains("undefined material"));
    let error = load(&format!("mtllib missing.mtl\n{TRI}")).unwrap_err();
    let Cause::Io(cause) = error.cause else {
        panic!()
    };
    assert_eq!(cause.kind(), io::ErrorKind::NotFound);
    assert_eq!(cause.to_string(), "original fixture missing");
}
#[test]
fn bounded_source_attributes_decoded_payload_and_descriptors() {
    let defaults = ImportLimits::default();
    let options = ObjOptions::default();
    for limits in [
        ImportLimits {
            max_source_bytes: 10,
            ..defaults.clone()
        },
        ImportLimits {
            max_vertices: 2,
            ..defaults.clone()
        },
        ImportLimits {
            max_indices: 2,
            ..defaults.clone()
        },
        ImportLimits {
            max_retained_bytes: 12,
            ..defaults.clone()
        },
        ImportLimits {
            max_items: 0,
            ..defaults.clone()
        },
        ImportLimits {
            max_node_depth: 0,
            ..defaults
        },
    ] {
        assert!(import_obj(
            CTX,
            TRI.as_bytes(),
            &mut Assets(BTreeMap::new()),
            &options,
            &limits
        )
        .is_err());
    }
    let repeated = "v 0 0 0\nv 1 0 0\nv 0 1 0\ng a\nf 1 2 3\ng b\nf 1 2 3";
    let limits = ImportLimits {
        max_vertices: 3,
        ..ImportLimits::default()
    };
    assert!(import_obj(
        CTX,
        repeated.as_bytes(),
        &mut Assets(BTreeMap::new()),
        &options,
        &limits
    )
    .unwrap_err()
    .to_string()
    .contains("decoded vertex"));
    assert!(load(&format!("#{}\n{TRI}", "x".repeat(65537))).is_err());
    let polygon = format!("v 0 0 0\nf {}", vec!["1"; 257].join(" "));
    assert!(load(&polygon).is_err());
}

#[test]
fn info_start_is_published_before_a_blocking_mtl_resolver() {
    use std::sync::{mpsc, Mutex};
    use std::time::Duration;
    struct Capture(Mutex<Vec<String>>);
    impl log::Log for Capture {
        fn enabled(&self, m: &log::Metadata) -> bool {
            m.level() <= log::Level::Info
        }
        fn log(&self, r: &log::Record) {
            if self.enabled(r.metadata()) {
                self.0.lock().unwrap().push(r.args().to_string());
            }
        }
        fn flush(&self) {}
    }
    static LOGGER: Capture = Capture(Mutex::new(Vec::new()));
    log::set_logger(&LOGGER).unwrap();
    log::set_max_level(log::LevelFilter::Info);
    let (entered_tx, entered) = mpsc::channel();
    let (release_tx, release) = mpsc::channel();
    struct Blocking {
        entered: mpsc::Sender<()>,
        release: mpsc::Receiver<()>,
    }
    impl AssetResolver for Blocking {
        fn read(&mut self, _: &str, _: ResourceKind, _: usize) -> io::Result<Vec<u8>> {
            self.entered.send(()).unwrap();
            self.release.recv().unwrap();
            Err(io::Error::new(
                io::ErrorKind::NotFound,
                "controlled original MTL cause",
            ))
        }
    }
    let worker = std::thread::spawn(move || {
        import_obj(
            ImportContext {
                request: 87999,
                source: 55,
            },
            b"mtllib blocked.mtl",
            &mut Blocking {
                entered: entered_tx,
                release,
            },
            &ObjOptions::default(),
            &ImportLimits::default(),
        )
    });
    entered.recv_timeout(Duration::from_secs(5)).unwrap();
    let published = LOGGER
        .0
        .lock()
        .unwrap()
        .iter()
        .any(|r| r == "start #87999 obj_import source=55");
    // Always release before asserting, so a regression does not leave a blocked test thread.
    release_tx.send(()).unwrap();
    let error = worker.join().unwrap().unwrap_err();
    assert!(published);
    assert!(error.to_string().contains("controlled original MTL cause"));
    let logs = LOGGER.0.lock().unwrap();
    assert!(logs
        .iter()
        .any(|r| r.contains("obj_import #87999 source=55 elapsed_ms=")
            && r.contains("phase=resource")
            && r.contains("controlled original MTL cause")));
}

#[test]
fn quoted_names_and_backslash_paths_are_local_and_percent_is_literal() {
    let source = format!("mtllib \"nested dir\\scene.mtl\"\nusemtl White\n{TRI}");
    let mut assets = Assets(BTreeMap::from([
        (
            "nested dir/scene.mtl".into(),
            b"newmtl White\nmap_Kd \"..\\textures\\white %20.png\"".to_vec(),
        ),
        ("textures/white %20.png".into(), png()),
    ]));
    let scene = import_obj(
        CTX,
        source.as_bytes(),
        &mut assets,
        &ObjOptions::default(),
        &ImportLimits::default(),
    )
    .unwrap();
    assert_eq!(
        scene.images()[0].resource.locator.as_deref(),
        Some("textures/white %20.png")
    );
}

#[test]
fn translated_concave_polygon_and_invalid_import_options_are_bounded() {
    let scene=load("v 1000000 1000000 0\nv 1000002 1000000 0\nv 1000002 1000001 0\nv 1000001 1000001 0\nv 1000001 1000002 0\nv 1000000 1000002 0\nf 1 2 3 4 5 6").unwrap();
    assert_eq!(scene.meshes()[0].primitives[0].indices.len(), 12);
    for units in [0., -1., f32::NAN, f32::INFINITY] {
        assert!(import_obj(
            CTX,
            TRI.as_bytes(),
            &mut Assets(BTreeMap::new()),
            &ObjOptions {
                metres_per_unit: units,
                ..ObjOptions::default()
            },
            &ImportLimits::default()
        )
        .is_err());
    }
    let huge = "v 0 0 0\nv 3e38 0 0\nv 0 3e38 0\nf 1 2 3";
    assert!(import_obj(
        CTX,
        huge.as_bytes(),
        &mut Assets(BTreeMap::new()),
        &ObjOptions {
            metres_per_unit: 10.,
            ..ObjOptions::default()
        },
        &ImportLimits::default()
    )
    .is_err());
}
