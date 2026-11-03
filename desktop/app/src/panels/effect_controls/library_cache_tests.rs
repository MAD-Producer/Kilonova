//! Differential catalogue semantics plus real panel-frame cache reuse.
use super::*;
use crate::panels::effect_search::{Fields, GroupedCache, Selection, Sort};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

fn catalogue() -> Arc<Value> {
    Arc::new(serde_json::json!({"packages":[
        {"hash":"old","manifest":{"id":"p","name":"Test Provider","version":"1.0.0","effects":[
            {"id":"blur","name":"旧模糊","english_name":"Old Blur","category":"模糊与锐化"}]}},
        {"hash":"new","manifest":{"id":"p","name":"Test Provider","version":"2.10.0","effects":[
            {"id":"blur","name":"模糊","english_name":"Blur","category":"模糊与锐化"},
            {"id":"glow","name":"辉光","english_name":"Glow","category":"光效"},
            {"id":"same","name":"模糊","english_name":"Blur","category":"模糊与锐化"},
            {"id":"missing","name":"只有中文","category":""},
            {"id":"empty","name":"","english_name":"","category":"Custom Category"},
            {"id":"unnamed","category":"Custom Category"},
            {"id":"unsupported","name":"不可用","english_name":"Unavailable","compatibility":"unsupported","category":"Custom Category"},
            {"id":"mesh","name":"网格","english_name":"Mesh","renderer":"geometry","category":"Custom Category"},
            {"name":"invalid no id"}]}},
        {"hash":"older","manifest":{"id":"p","name":"Test Provider","version":"2.9.0","effects":[
            {"id":"blur","name":"不应显示","english_name":"Wrong Version","category":"Wrong"}]}},
        {"hash":"q","manifest":{"id":"q","name":"Other Provider","version":"1.0.0","effects":[
            {"id":"blur","name":"模糊","english_name":"Blur","category":"模糊与锐化"}]}},
        {"enabled":false,"hash":"off","manifest":{"id":"disabled","version":"1.0.0","effects":[{"id":"hidden"}]}},
        {"manifest":{"id":"local.motionstudio.bound.fixture","version":"1.0.0","effects":[{"id":"bound"}]}},
        {"manifest":{"effects":[{"id":"invalid no plugin"}]}}, null
    ]}))
}

// Keep the previous uncached grouping/search/sort here as a differential oracle.
// Metadata projection itself is unchanged production entries(), with explicit
// version/provider/unsupported assertions below rather than a copied model.
fn legacy_groups(
    catalogue: &Value,
    locale: Locale,
    query: &str,
    tab: usize,
    favorites: &BTreeSet<String>,
    recent: &[String],
    chooser: bool,
) -> Vec<(String, Vec<Entry>)> {
    let query = query.trim().to_lowercase();
    let mut groups: BTreeMap<String, Vec<Entry>> = BTreeMap::new();
    for entry in entries(catalogue, locale) {
        let terms = format!(
            "{} {} {} {} {} {} {} {} {}",
            entry.name,
            entry.english,
            entry.original_name,
            entry.id,
            entry.category,
            entry.raw_category,
            entry.provider,
            entry.plugin,
            entry.key
        )
        .to_lowercase();
        if chooser && !entry.supported
            || !query.split_whitespace().all(|word| terms.contains(word))
            || tab == 1 && !favorites.contains(&entry.key)
            || tab == 2 && !recent.contains(&entry.key)
        {
            continue;
        }
        groups
            .entry(entry.category.clone())
            .or_default()
            .push(entry);
    }
    for group in groups.values_mut() {
        if chooser {
            group.sort_by_key(|entry| {
                (
                    !favorites.contains(&format!("{}/{}", entry.plugin, entry.id)),
                    entry.id.clone(),
                )
            });
        } else if tab == 2 {
            group.sort_by_key(|entry| recent.iter().position(|key| key == &entry.key));
        } else {
            group.sort_by(|a, b| a.name.cmp(&b.name).then_with(|| a.key.cmp(&b.key)));
        }
    }
    groups.into_iter().collect()
}

fn legacy_label(catalogue: &Value, entry: &Entry, locale: Locale) -> String {
    catalogue["packages"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|package| {
            package["manifest"]["id"].as_str() == Some(entry.plugin.as_str())
                && package["hash"].as_str() == Some(entry.hash.as_str())
        })
        .and_then(|package| {
            package["manifest"]["effects"]
                .as_array()?
                .iter()
                .find(|effect| effect["id"].as_str() == Some(entry.id.as_str()))
        })
        .and_then(|effect| {
            if locale == Locale::Zh {
                effect["name"].as_str()
            } else {
                effect["english_name"].as_str()
            }
        })
        .unwrap_or("Effect")
        .to_owned()
}

#[test]
fn library_and_chooser_keep_old_order_search_metadata_and_name_fallbacks() {
    let catalogue = catalogue();
    let favorites = BTreeSet::from(["p/same".into(), "q/blur".into()]);
    let recent = vec!["q/blur".into(), "p/same".into(), "p/blur".into()];
    let mut library_cache: GroupedCache<Entry, Locale> = GroupedCache::default();
    let mut chooser_cache: GroupedCache<(Entry, String), Locale> = GroupedCache::default();
    for locale in [Locale::En, Locale::Zh] {
        let entries = entries(&catalogue, locale);
        assert_eq!(entries.len(), 9);
        let blur = entries.iter().find(|entry| entry.key == "p/blur").unwrap();
        assert_eq!((&*blur.version, &*blur.hash), ("2.10.0", "new"));
        assert!(entries.iter().any(|entry| entry.key == "q/blur"));
        assert!(entries.iter().any(|entry| !entry.supported));
        assert!(entries.iter().all(|entry| entry.plugin != "disabled"
            && !entry.plugin.starts_with("local.motionstudio.bound.")));
        for query in [
            "",
            "  BLUR  ",
            "模糊",
            "provider blur",
            "p/",
            "custom category",
            "unavailable",
            "missing",
            "none",
        ] {
            for tab in 0..3 {
                let library = library_cache.grouped(
                    &catalogue,
                    locale,
                    query,
                    Selection {
                        tab,
                        favorites: &favorites,
                        recent: &recent,
                        sort: if tab == 2 { Sort::Recent } else { Sort::Name },
                    },
                    || library::projection(&catalogue, locale),
                    Fields {
                        key: |e| &e.key,
                        name: |e| &e.name,
                        category: |e| &e.category,
                    },
                );
                let actual: Vec<_> = library
                    .groups()
                    .map(|group| {
                        assert_eq!(group.count_text(), group.len().to_string());
                        (
                            group.name().to_owned(),
                            group.iter().cloned().collect::<Vec<_>>(),
                        )
                    })
                    .collect();
                assert_eq!(
                    actual,
                    legacy_groups(&catalogue, locale, query, tab, &favorites, &recent, false)
                );
                let chooser = chooser_cache.grouped(
                    &catalogue,
                    locale,
                    query,
                    Selection {
                        tab,
                        favorites: &favorites,
                        recent: &recent,
                        sort: Sort::FavoritesFirst,
                    },
                    || library::chooser_projection(&catalogue, locale),
                    Fields {
                        key: |(e, _)| &e.key,
                        name: |(e, _)| &e.id,
                        category: |(e, _)| &e.category,
                    },
                );
                let actual: Vec<_> = chooser
                    .groups()
                    .map(|group| {
                        (
                            group.name().to_owned(),
                            group.iter().cloned().collect::<Vec<_>>(),
                        )
                    })
                    .collect();
                let expected: Vec<_> =
                    legacy_groups(&catalogue, locale, query, tab, &favorites, &recent, true)
                        .into_iter()
                        .map(|(category, entries)| {
                            (
                                category,
                                entries
                                    .into_iter()
                                    .map(|entry| {
                                        let label = legacy_label(&catalogue, &entry, locale);
                                        (entry, label)
                                    })
                                    .collect::<Vec<_>>(),
                            )
                        })
                        .collect();
                assert_eq!(actual, expected);
            }
        }
        let projection = library::chooser_projection(&catalogue, locale);
        let label = |key: &str| {
            &projection
                .iter()
                .find(|((entry, _), _)| entry.key == key)
                .unwrap()
                .0
                 .1
        };
        assert_eq!(label("p/empty"), "");
        assert_eq!(label("p/unnamed"), "Effect");
        assert_eq!(
            label("p/missing"),
            if locale == Locale::En {
                "Effect"
            } else {
                "只有中文"
            }
        );
    }
}

fn draw(
    controls: &mut Controls,
    atlas: &mut TextAtlas,
    width: f32,
    locale: Locale,
    chooser: bool,
    input: &Input,
) -> (PaintList, f32, Vec<Action>) {
    let mut paint = PaintList::default();
    let area = Rect::new(0., 0., width, 480.);
    let background = paint.styles().colors().PANEL;
    paint::rect(&mut paint, area, background);
    let layer = serde_json::json!({"id":7,"locked":false,"content":{"kind":"solid"},"effects":[]});
    let state = serde_json::json!({"frame":0,"revision":0});
    let mut out = Vec::new();
    let mut y = 0.;
    if chooser {
        controls.render(
            &mut paint, atlas, area, &mut y, &layer, &state, input, locale, &mut out,
        );
    } else {
        controls.draw_library(&mut paint, atlas, area, &layer, input, locale, &mut out);
    }
    (paint, y, out)
}

fn paint_snapshot(paint: &PaintList, y: f32) -> String {
    format!(
        "{:?}",
        (&paint.shapes, &paint.colors, &paint.glyphs, &paint.order, y)
    )
}

#[test]
fn real_library_frames_reuse_cache_and_follow_query_locale_favorites_recent_and_refresh() {
    for width in [220., 360.] {
        let mut controls = Controls {
            catalogue: catalogue(),
            library_initialized: true,
            ..Default::default()
        };
        let mut atlas = TextAtlas::from_system_language(12., true).unwrap();
        let (_, _, actions) = draw(
            &mut controls,
            &mut atlas,
            width,
            Locale::Zh,
            false,
            &Input::default(),
        );
        assert!(actions.is_empty());
        assert_eq!(controls.library_cache.builds(), (1, 1));
        for _ in 0..30 {
            let (paint, _, actions) = draw(
                &mut controls,
                &mut atlas,
                width,
                Locale::Zh,
                false,
                &Input::default(),
            );
            assert!(!paint.glyphs.is_empty() && actions.is_empty());
        }
        assert_eq!(controls.library_cache.builds(), (1, 1));
        for (query, locale, tab, favorite, recent, expected) in [
            ("模糊", Locale::Zh, 0, false, false, (1, 2)),
            ("blur", Locale::En, 0, false, false, (2, 3)),
            ("", Locale::En, 1, true, false, (2, 4)),
            ("", Locale::En, 2, false, true, (2, 5)),
        ] {
            controls.query = query.into();
            controls.library_tab = tab;
            if favorite {
                controls.favorites.insert("p/blur".into());
            }
            if recent {
                controls.recent = vec!["q/blur".into(), "p/blur".into()];
            }
            draw(
                &mut controls,
                &mut atlas,
                width,
                locale,
                false,
                &Input::default(),
            );
            assert_eq!(controls.library_cache.builds(), expected);
        }
        let old = Arc::downgrade(&controls.catalogue);
        let mut updated = (*controls.catalogue).clone();
        updated["packages"].as_array_mut().unwrap().push(serde_json::json!({"hash":"imported","manifest":{"id":"imported","version":"1.0.0","effects":[{"id":"new","name":"新增效果","english_name":"Imported"}]}}));
        controls.refresh_catalogue(Arc::new(updated));
        assert!(old.upgrade().is_some());
        controls.query.clear();
        controls.library_tab = 0;
        draw(
            &mut controls,
            &mut atlas,
            width,
            Locale::En,
            false,
            &Input::default(),
        );
        assert_eq!(controls.library_cache.builds(), (3, 6));
        assert!(old.upgrade().is_none());
        assert!(controls.library_row("imported/new").is_some());
        controls.refresh_catalogue(Arc::new(serde_json::json!({"packages":[]})));
        draw(
            &mut controls,
            &mut atlas,
            width,
            Locale::En,
            false,
            &Input::default(),
        );
        assert_eq!(controls.library_cache.builds(), (4, 7));
        assert!(controls.library_row("imported/new").is_none());
    }
}

#[test]
fn inline_chooser_height_and_render_share_one_projection_and_keep_click_identity() {
    for locale in [Locale::En, Locale::Zh] {
        for width in [220., 360.] {
            let mut controls = Controls {
                catalogue: catalogue(),
                choosing: true,
                ..Default::default()
            };
            let mut atlas = TextAtlas::from_system_language(12., locale == Locale::Zh).unwrap();
            let styles = PaintList::default().styles();
            let layer = serde_json::json!({"id":7});
            let height = controls.height(&layer, &styles, locale, width);
            assert_eq!(controls.chooser_cache.builds(), (1, 1));
            let (paint, y, actions) = draw(
                &mut controls,
                &mut atlas,
                width,
                locale,
                true,
                &Input::default(),
            );
            assert!(actions.is_empty() && y > 0. && !paint.glyphs.is_empty());
            let expected = paint_snapshot(&paint, y);
            for _ in 0..20 {
                assert_eq!(controls.height(&layer, &styles, locale, width), height);
                let (paint, y, actions) = draw(
                    &mut controls,
                    &mut atlas,
                    width,
                    locale,
                    true,
                    &Input::default(),
                );
                assert!(actions.is_empty());
                assert_eq!(paint_snapshot(&paint, y), expected);
            }
            assert_eq!(controls.chooser_cache.builds(), (1, 1));
            // Force a new projection: the same frame geometry/order/text must result.
            controls.chooser_cache = GroupedCache::default();
            let (paint, y, actions) = draw(
                &mut controls,
                &mut atlas,
                width,
                locale,
                true,
                &Input::default(),
            );
            assert_eq!(paint_snapshot(&paint, y), expected);
            assert!(actions.is_empty());
            // Search leaves one real row, whose existing pointer action must carry
            // the newest package identity, and still update recent/close chooser.
            controls.query = "p/blur".into();
            let (paint, y, _) = draw(
                &mut controls,
                &mut atlas,
                width,
                locale,
                true,
                &Input::default(),
            );
            assert!(!paint.glyphs.is_empty());
            let point = [60., y - styles.metric("row.height", 22.) * 0.5];
            let (_, _, actions) = draw(
                &mut controls,
                &mut atlas,
                width,
                locale,
                true,
                &Input {
                    mouse: point,
                    pressed: Some((MouseButton::Left, point)),
                    ..Default::default()
                },
            );
            assert!(
                matches!(actions.as_slice(), [Action::Plugin(command)] if command["object"] == 7 && command["plugin"] == "p" && command["version"] == "2.10.0" && command["hash"] == "new" && command["effect"] == "blur")
            );
            assert!(!controls.choosing);
            assert_eq!(controls.recent[0], "p/blur");
        }
    }
}

#[cfg(feature = "gpu-tests")]
fn capture(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    painter: &mut motion_ui::Painter,
    list: &PaintList,
    logical: [f32; 2],
    ratio: f32,
) -> Vec<u8> {
    let width = (logical[0] * ratio).round() as u32;
    let height = (logical[1] * ratio).round() as u32;
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("text bounds test"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = texture.create_view(&Default::default());
    let row = (width * 4 + 255) & !255;
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: row as u64 * height as u64,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&Default::default());
    {
        let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            multiview_mask: None,
            label: None,
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                depth_slice: None,
                view: &view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });
    }
    painter
        .render_scaled(
            device,
            queue,
            &mut encoder,
            &view,
            list,
            logical,
            [ratio, ratio],
        )
        .expect("UI frame must fit GPU buffers");
    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture: &texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(row),
                rows_per_image: Some(height),
            },
        },
        wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
    );
    queue.submit(Some(encoder.finish()));
    let (send, receive) = std::sync::mpsc::channel();
    buffer
        .slice(..)
        .map_async(wgpu::MapMode::Read, move |result| {
            let _ = send.send(result);
        });
    device
        .poll(wgpu::PollType::wait_indefinitely())
        .expect("GPU poll failed");
    receive.recv().unwrap().unwrap();
    let bytes = buffer
        .slice(..)
        .get_mapped_range()
        .expect("GPU mapped range unavailable");
    let mut pixels = vec![];
    for data in bytes.chunks(row as usize) {
        pixels.extend_from_slice(&data[..width as usize * 4]);
    }
    pixels
}

#[test]
#[cfg(feature = "gpu-tests")]
fn real_cached_library_and_chooser_keep_fresh_gpu_pixels_at_both_widths_and_languages() {
    let instance = wgpu::Instance::default();
    let adapter =
        pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
            .unwrap();
    let (device, queue) =
        pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).unwrap();
    let mut painter = motion_ui::Painter::new(&device, wgpu::TextureFormat::Rgba8Unorm);
    let output = std::env::var_os("MOTION_EFFECT_CATALOGUE_CAPTURE").map(std::path::PathBuf::from);
    if let Some(output) = &output {
        std::fs::create_dir_all(output).unwrap();
    }
    for locale in [Locale::En, Locale::Zh] {
        let name = if locale == Locale::Zh { "zh" } else { "en" };
        let mut atlas = TextAtlas::from_system_language(12., locale == Locale::Zh).unwrap();
        for width in [220., 360.] {
            for chooser in [false, true] {
                let mut controls = Controls {
                    catalogue: catalogue(),
                    library_initialized: true,
                    choosing: chooser,
                    ..Default::default()
                };
                draw(
                    &mut controls,
                    &mut atlas,
                    width,
                    locale,
                    chooser,
                    &Input::default(),
                );
                let (cached, y, actions) = draw(
                    &mut controls,
                    &mut atlas,
                    width,
                    locale,
                    chooser,
                    &Input::default(),
                );
                assert!(actions.is_empty() && !cached.glyphs.is_empty());
                controls.library_cache = GroupedCache::default();
                controls.chooser_cache = GroupedCache::default();
                let (fresh, fresh_y, actions) = draw(
                    &mut controls,
                    &mut atlas,
                    width,
                    locale,
                    chooser,
                    &Input::default(),
                );
                assert!(actions.is_empty());
                assert_eq!(paint_snapshot(&cached, y), paint_snapshot(&fresh, fresh_y));
                controls.query = "p/blur".into();
                let (changed, _, _) = draw(
                    &mut controls,
                    &mut atlas,
                    width,
                    locale,
                    chooser,
                    &Input::default(),
                );
                painter
                    .upload_atlas(
                        &device,
                        &queue,
                        atlas.pixel_data(),
                        atlas.config().width,
                        atlas.config().height,
                    )
                    .unwrap();
                let logical = [width, 480.];
                let expected = capture(&device, &queue, &mut painter, &fresh, logical, 1.);
                let actual = capture(&device, &queue, &mut painter, &cached, logical, 1.);
                assert_eq!(
                    actual, expected,
                    "width={width} locale={name} chooser={chooser}"
                );
                let wrong = capture(&device, &queue, &mut painter, &changed, logical, 1.);
                let differences = actual.iter().zip(&wrong).filter(|(a, b)| a != b).count();
                assert!(
                    differences > 100,
                    "query-change control must visibly change output"
                );
                if let Some(output) = &output {
                    for (kind, pixels) in [
                        ("cached", &actual),
                        ("fresh", &expected),
                        ("query-control", &wrong),
                    ] {
                        image::save_buffer(
                            output.join(format!("{name}-{width}-chooser{chooser}-{kind}.png")),
                            pixels,
                            width as u32,
                            480,
                            image::ColorType::Rgba8,
                        )
                        .unwrap();
                    }
                }
                println!("effect catalogue GPU locale={name} width={width} chooser={chooser}: exact cached/fresh match; changed-query control differs in {differences} bytes");
            }
        }
    }
}
