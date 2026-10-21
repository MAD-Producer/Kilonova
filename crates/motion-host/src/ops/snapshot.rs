//! Host-facing state projection.
//!
//! This snapshot is the UI's single source of truth and is identical on every
//! platform: a host that cannot satisfy a capability must report it as absent
//! here rather than silently omitting a field, which is what keeps the desktop
//! and Android feature surfaces comparable.
use crate::session::Session;
use motion_render::PreviewMode;
use serde_json::{json, Value};

impl Session {
    /// Explicit diagnostic read; ordinary playback snapshots do not enumerate allocations.
    pub fn texture_resource_ledger(&self) -> Value {
        let mut ledger = motion_render::TextureResourceLedger::default();
        ledger.merge(self.sampling.ledger());
        if let Some(g) = &self.graphics {
            ledger.merge(g.renderer.texture_resource_ledger());
            g.scratch.collect_texture_resources(&mut ledger);
        }
        if let Some(renderer) = &self.editor_renderer {
            ledger.merge(renderer.texture_resource_ledger());
        }
        if let Some(target) = &self.editor_target { ledger.merge(target.texture_resource_ledger()); }
        let preview_submissions = self.graphics.as_ref()
            .map(|g| g.renderer.submission_memory_snapshot(&ledger));
        let editor_submissions = self.editor_renderer.as_ref()
            .map(|r| r.submission_memory_snapshot(&ledger));
        let capacity = motion_render::FrameTextureCapacity::observe(&ledger,
            preview_submissions.iter().chain(editor_submissions.iter()));
        json!({"version":1,"scope":"session_application_owned_texture_capacity",
            "combinedCapacity":{"version":1,"scope":"current_ownership_and_instrumented_pending_texture_capacity",
                "observation":capacity,"isFramePeak":false,"isDriverResidency":false,
                "atomicAcrossRenderers":false,"completeCoverage":false},
            "submissionTracking":{"requested":self.resource_tracking,
                "preview":preview_submissions.as_ref().map(|s|&s.info),
                "editor":editor_submissions.as_ref().map(|s|&s.info),
                "coverage":"conservative_instrumented_submissions",
                "excludes":["raw_queue_submissions","standalone_video_conversions","gpu_uploads","swapchain","ui_and_node_preview_resources","detached_renderer_contexts","view_and_bind_group_only_references"]},
            "graphicsAvailable":self.graphics.is_some(),"editorRendererAvailable":self.editor_renderer.is_some(),
            "knownBytes":ledger.total_known_bytes(),"unknownCapacityCount":ledger.unknown_capacity_count(),
            "resources":ledger.resources().collect::<Vec<_>>(),
            "gpuRetirementBytes":null,"driverResidentBytes":null,
            "excludes":["gpu_buffers","upload_staging","cpu_decode_caches","swapchain",
                "decoder_external_allocations","ui_and_independent_preview_textures","gpu_retirement"]})
    }
    pub fn preview_info(&self) -> Value {
        let tier = self.preview.tier();
        let video = self.video_info();
        let p = self.engine.project();
        let (sw, sh) = self
            .graphics
            .as_ref()
            .map_or((p.width, p.height), |g| (g.config.width, g.config.height));
        let (width, height) = self.preview.render_dimensions(p.width, p.height, sw, sh);
        let quality = self.preview_memory.quality(self.preview.mode == PreviewMode::High);
        let requested = quality.dimensions(width, height);
        let (width, height) = requested;
        json!({"mode":self.preview.mode.name(),"tier":tier.name(),"width":width,"height":height,"fps":if self.preview_memory.blocked(){tier.fps().min(15)}else{tier.fps()},
            "memoryAdaptation":{"version":1,"status":if self.preview_memory.blocked(){"blocked"}else if quality.level>0{"degraded"}else{"normal"},
                "requested":quality,"actual":self.graphics.as_ref().and_then(|g|g.actual_preview_quality),
                "requestedWidth":requested.0,"requestedHeight":requested.1,
                "actualWidth":self.graphics.as_ref().filter(|g|g.scratch.valid()).map(|g|g.scratch.width),
                "actualHeight":self.graphics.as_ref().filter(|g|g.scratch.valid()).map(|g|g.scratch.height),
                "failures":self.preview_memory.failures(),"refusalCount":self.preview_memory.refusal_count,"maxLevels":motion_render::preview_memory::PREVIEW_MEMORY_LEVELS,
                "rejection":self.preview_memory.last_rejection,"held":self.preview_memory_held,
                "hasLastValidFrame":self.graphics.as_ref().is_some_and(|g|g.scratch.valid()),
                "nextProbeInMs":self.preview_memory.next_probe_ms().saturating_sub(self.preview_memory_clock.elapsed().as_millis() as u64)},
            "previewVideoProxyBytes":self.preview_video.bytes(),
            "exposure":self.sampling.status(),
            "exposureRequired":self.sampling.required(),
            "scratchBudgetBytes":self.effects.scratch_budget(),"memoryPolicyVersion":motion_render::resource_policy::MEMORY_POLICY_VERSION,
            "sourceTextureBudgetBytes":self.effects.memory_policy().source_texture_budget_bytes,
            "memoryPolicy":self.effects.memory_policy(),
            "effectResolution":if self.preview.mode == PreviewMode::High {"full_layer"} else {"projected_2d"},
            "imageResolution":quality.image_edge.map_or_else(||"original".to_string(),|edge|format!("proxy_max_{edge}")),
            "videoResolution":quality.video_edge.map_or_else(||"decoded_source".to_string(),|edge|format!("proxy_max_{edge}")),
            "imageDecodes":self.graphics.as_ref().map_or(0,|g|g.renderer.image_decodes),
            "imageProxyCacheHits":self.graphics.as_ref().map_or(0,|g|g.renderer.image_proxy_cache_hits),
            "imageUploadBytes":self.graphics.as_ref().map_or(0,|g|g.renderer.image_upload_bytes),
            "imageMemoryCacheHits":self.graphics.as_ref().map_or(0,|g|g.renderer.image_memory_cache_hits),
            "imageIdleBytes":self.graphics.as_ref().map_or(0,|g|g.renderer.image_idle_bytes()),
            "imageIdleBudgetBytes":motion_render::image_resources::IDLE_TEXTURE_BYTES,
            "imagePrefetches":self.graphics.as_ref().map_or(0,|g|g.renderer.image_prefetches),
            "imagePrefetchSeconds":motion_render::image_resources::PREFETCH_SECONDS,
            "effectPlanBuilds":self.graphics.as_ref().map_or(0,|g|g.renderer.effect_plan_builds()),
            "effectPlanCacheHits":self.graphics.as_ref().map_or(0,|g|g.renderer.effect_plan_cache_hits()),
            "effectPipelineBuilds":self.graphics.as_ref().map_or(0,|g|g.renderer.effect_pipeline_builds()),
            "compositingPipelineSetBuilds":self.graphics.as_ref().map_or(0,|g|g.renderer.compositing_pipeline_set_builds()),
            "surfaceBounded":self.preview.mode != PreviewMode::High,
            "gpuTimingActive":!self.sampling.required() && self.graphics.as_ref().is_some_and(|g|g.timer.is_some()),
            "profiling":self.recorder.is_some(),
            "pluginEditorRecovery":{"open":self.editor.is_some(),"gesture":self.editor.as_ref().is_some_and(|e|e.gesture),
                "leaseMillis":super::effects::PLUGIN_EDITOR_LEASE.as_millis(),"cancelOperation":"cancel_plugin_gesture","last":self.editor_recovery},
            "gpuTimestampSupported":self.graphics.as_ref().is_some_and(|g|g.renderer.device.features().contains(wgpu::Features::TIMESTAMP_QUERY)),
            "resourceLedgerVersion":1,
            "memoryBreakdown":self.graphics.as_ref().map(|g| {
                let memory=g.renderer.texture_memory_breakdown();
                let target=g.scratch.texture_bytes();
                json!({"version":1,"scope":"logical_texture_capacity",
                    "rendererBytes":memory.total_bytes(),"categories":memory,
                    "legacyTextureBytes":g.renderer.texture_bytes(),
                    "previewTargetBytes":target,"exposureTargetBytes":g.renderer.exposure_texture_bytes(),
                    "previewOwnedTextureBytes":memory.total_bytes()+target+g.renderer.exposure_texture_bytes(),
                    "driverResidentBytes":null,"cpuDecodedCacheBytes":null,
                    "cpuVideoDecodedCacheBytes":video["cacheBytes"],
                    "cpuVideoPreviewProxyBytes":self.preview_video.bytes(),
                    "excludes":["gpu_buffers","upload_staging","cpu_decode_caches",
                        "swapchain","decoder_external_allocations","ui_and_independent_preview_textures"]})
            }),
            "video":video})
    }

    pub fn video_info(&self) -> Value {
        let mut info = self.video_frames.metrics();
        info["renderAttempts"] = json!(self.render_attempts);
        info["pendingAttempts"] = json!(self.video_pending_attempts);
        info["lastPrepareUs"] = json!(self.video_prepare_us);
        info["lastUploadUs"] = json!(self.video_upload_us);
        if let Some(g) = &self.graphics {
            info["uploadBytes"] = json!(g.renderer.video_upload_bytes);
            info["uploads"] = json!(g.renderer.video_uploads);
            info["gpuConversions"] = json!(g.renderer.video_gpu_conversions);
            info["standaloneConversionSubmissions"] = json!(g.renderer.video_conversion_submissions);
            info["encodedConversions"] = json!(g.renderer.video_encoded_conversions);
            info["frameBlendSubmissions"] = json!(g.renderer.video_blend_submissions);
        }
        info
    }

    /// Platform-specific video capability declaration.
    ///
    /// Both hosts fill this from the same template; only `decoder`, the container
    /// list and the encoder description differ, and they differ because the
    /// underlying platform does, not because the host forgot to implement it.
    fn video_capabilities(&self) -> Value {
        json!({
            "container":"MP4","codec":"H.264 baseline/main/high, 8-bit 4:2:0 SDR",
            "containers":["MP4","MOV","3GP","Matroska","WebM"],
            "codecs":["H.264","H.265 Main","VP8","VP9 profile 0"],
            "profile":"8-bit 4:2:0 SDR","device_query":"media_capabilities",
            "max_pixels":motion_core::MAX_VIDEO_PIXELS,"max_dimension":motion_core::MAX_VIDEO_DIMENSION,
            "max_fps":motion_core::MAX_VIDEO_FPS,"max_index_frames":motion_core::MAX_VIDEO_FRAMES,
            "preserves_source_aspect_ratio":true,"arbitrary_aspect_ratio":true,"square_pixels_only":true,
            "input_is_independent_of_composition":true,"max_duration_seconds":3600,
            "async_frames":true,"frame_format":"rgba8",
            "decoder":self.platform().name(),
            "max_decoders":4,"default_with_audio":true,"frozen_source_frames":true,
            "legacy_gles_export_integrated":true
        })
    }

    pub fn snapshot(&self) -> Value {
        let original = self.engine.project();
        let p = self.scene.sampled_project(original);
        let f = self.frame;
        let camera = json!({"position":p.camera.position_at(f),"target":p.camera.target.sample(f),
            "fov":p.camera.fov.sample(f).clamp(10.0,120.0),"roll":p.camera.roll.sample(f),
            "radius":p.camera.radius.sample(f).clamp(1.0,10_000_000.0),
            "azimuth":p.camera.azimuth.sample(f),"elevation":p.camera.elevation.sample(f).clamp(-89.0,89.0)});
        let layers: Vec<_> = p
            .layers
            .iter()
            .map(|l| {
                let local = l.local_frame(f);
                let camera = l.camera_data().map(|c| json!({"position":c.position_at(local),"target":c.target.sample(local),
                    "roll":c.roll.sample(local),"fov":c.fov.sample(local),"radius":c.radius.sample(local),
                    "azimuth":c.azimuth.sample(local),"elevation":c.elevation.sample(local)}));
                let mut sample = json!({"id":l.id,"position":l.transform.position.sample(local),
            "rotation":l.transform.rotation.sample(local),"scale":l.transform.scale.sample(local),"anchor":l.transform.anchor.sample(local),
            "opacity":l.transform.opacity.sample(local).clamp(0.0,1.0),"active":l.active(f,p.frames),"three_d":l.three_d,
            "audio_levels":l.audio_levels.as_ref().map(|levels|{let v=levels.sample(local);json!({"left":v[0],"right":v[1]})})});
                if let Some(camera) = camera {
                    for (key, value) in camera.as_object().into_iter().flatten() {
                        sample[key] = value.clone();
                    }
                }
                sample
            })
            .collect();
        let mut projected: Vec<_> = self
            .scene
            .layers
            .iter()
            .filter_map(|layer| {
                let corners = motion_render::selection_geometry::polygon(layer, &self.scene, &self.effects.registry)?;
                let anchor = p
                    .layers
                    .iter()
                    .find(|l| l.id == layer.id)
                    .and_then(|l| self.scene.project_node(l.id));
                Some(json!({"id":layer.id,"anchor":anchor,"corners":corners,"selection_space":"effect_output"}))
            })
            .collect();
        for layer in p
            .layers
            .iter()
            .filter(|l| matches!(l.content, motion_core::Content::Null) && l.active(f, p.frames))
        {
            if let Some(point) = self.scene.project_node(layer.id) {
                let [x, y, _] = point;
                if x.is_finite() && y.is_finite() {
                    projected.push(json!({"id":layer.id,"anchor":point,"null":true,
                    "corners":[[x-12.0,y-12.0],[x+12.0,y-12.0],[x+12.0,y+12.0],[x-12.0,y+12.0]]}));
                }
            }
        }
        let mask_layers:Vec<_>=self.scene.layers.iter().filter_map(|layer| {
            let stored=p.layers.iter().find(|l|l.id==layer.id)?;
            if stored.masks.is_empty(){return None;}
            let local=stored.local_frame(f);
            let masks:Vec<_>=stored.masks.iter().map(|m|{
                let nodes:Vec<_>=m.path.nodes.iter().map(|n|json!({"id":n.id,"geometry":n.geometry.sample(local)})).collect();
                json!({"id":m.id,"opacity":m.opacity.sample(local).clamp(0.,100.),
                    "feather":m.feather.sample(local).map(|v|v.clamp(0.,motion_core::masks::MAX_MASK_DISTANCE)),
                    "expansion":m.expansion.sample(local),
                    "path":{"id":m.path.id,"closed":m.path.closed,"nodes":nodes}})
            }).collect();
            Some(json!({"id":layer.id,"mvp":(layer.view_projection*layer.model).to_cols_array(),"masks":masks}))
        }).collect();
        let vector_layers: Vec<_> = self.scene.layers.iter().filter_map(|layer| {
            let vector=layer.vector.as_ref()?;
            let stored=p.layers.iter().find(|l|l.id==layer.id).and_then(|l|match &l.content {
                motion_core::Content::Vector{vector}=>Some(&vector.source),_=>None,
            });
            let paths:Vec<_>=vector.paths.iter().enumerate().map(|(i,path)| {
                let original_path=match stored {Some(motion_core::vector::VectorSource::Paths{paths})=>paths.get(i),_=>None};
                let nodes:Vec<_>=path.nodes.iter().enumerate().map(|(j,geometry)|json!({"id":original_path.and_then(|p|p.nodes.get(j)).map_or(j as u64+1,|n|n.id),"geometry":geometry})).collect();
                json!({"id":original_path.map_or(i as u64+1,|p|p.id),"closed":path.closed,"nodes":nodes})
            }).collect();
            let parameters=match stored {
                Some(motion_core::vector::VectorSource::Shape{parameters,..})=>{
                    let offset=p.layers.iter().find(|l|l.id==layer.id).map_or(0,|l|l.clip(p.frames).offset_frame);
                    parameters.iter().map(|(name,track)|(name.clone(),json!(track.sample(f-f64::from(offset))))).collect::<serde_json::Map<_,_>>()
                },_=>serde_json::Map::new(),
            };
            let mut modifier_parameters=serde_json::Map::new();
            if let Some(t)=&vector.trim {
                modifier_parameters.insert("trim_start".into(),json!(t.start));
                modifier_parameters.insert("trim_end".into(),json!(t.end));
                modifier_parameters.insert("trim_offset".into(),json!(t.offset));
            }
            if let Some(d)=&vector.dashes {
                modifier_parameters.insert("dash_offset".into(),json!(d.offset));
                for (i,value) in d.pattern.iter().enumerate() {modifier_parameters.insert(format!("dash_{i}"),json!(value));}
            }
            Some(json!({"id":layer.id,"canvas_size":layer.source_size,"source_rect":layer.source_rect,
                "mvp":(layer.view_projection*layer.model).to_cols_array(),"paths":paths,"parameters":parameters,"modifier_parameters":modifier_parameters,
                "group_parameters":vector.group_parameters,
                "fill":vector.fill,"stroke":vector.stroke.map(|s|json!({"color":s.0,"width":s.1,"cap":s.2,"join":s.3,"miter_limit":s.4}))}))
        }).collect();
        // Logical source plane, before vector/effect output bounds expansion.
        // Hosts use this same evaluated matrix for transform and path gestures.
        let source_transforms: Vec<_> = p.layers.iter().filter_map(|layer| {
            if !layer.active(f,p.frames) || matches!(layer.content,motion_core::Content::Audio{..} | motion_core::Content::Camera { .. }) {return None;}
            let world=self.scene.world_matrix(layer.id)?;
            let vp=self.scene.layers.iter().find(|l|l.id==layer.id).map(|l|l.view_projection)
                .unwrap_or_else(||if layer.three_d {self.scene.camera.view_projection}else{
                    glam::Mat4::orthographic_rh(-(p.width as f32)*0.5,p.width as f32*0.5,
                        -(p.height as f32)*0.5,p.height as f32*0.5,-1.,1.)
                });
            let anchor=layer.transform.anchor.sample(layer.local_frame(f));
            let offset=glam::Mat4::from_translation(glam::Vec3::new(
                (0.5-anchor[0])*layer.size[0],
                (anchor[1]-0.5)*layer.size[1],0.));
            Some(json!({"id":layer.id,"mvp":(vp*world*offset).to_cols_array(),
                "size":layer.size,"anchor":anchor,"coordinates":"source_centered_pixels"}))
        }).collect();
        let camera_properties: Vec<&str> = if !p.camera.created {
            vec![]
        } else if p.camera.mode == motion_core::CameraMode::Position {
            vec!["position", "target"]
        } else {
            vec!["target"]
        };
        let (has_audio, audio_error) = match p.audio_voices() {
            Ok(voices) => (Some(!voices.is_empty()), None),
            Err(error) => (None, Some(error.to_string())),
        };
        {
            let mut previous = self.audio_snapshot_error.borrow_mut();
            if audio_error != *previous {
                if let Some(error) = &audio_error {
                    log::warn!("snapshot audio plan failed: composition={} reason={error}", p.composition_id);
                }
                *previous = audio_error.clone();
            }
        }
        let mut snapshot=json!({"project":original,"root":self.root.to_string_lossy(),"frame":f,"revision":self.engine.revision(),"canUndo":self.engine.can_undo(),
            "main_composition":"comp-main","composition":p.composition_id,"compositions":p.composition_list(),
            "has_audio":has_audio,"audio_error":audio_error,"history":self.engine.history_diagnostics(),
            "composition_context":self.composition_context_snapshot(),
            "vector_layers":vector_layers,"mask_layers":mask_layers,"source_transforms":source_transforms,
            "platform":{"name":self.platform().name(),"graphics":"wgpu"},
            "capabilities":{"spatial_paths":{"supported":true,"version":1,"query":"position_path","command":"spatial","parameter_scope":true,"max_visible_keys":512,"max_trace_samples":257,"coordinates":"property_local","temporal_easing":"independent","separated_axes":"trace_only"},"adjustment_layers":{"supported":true,"command":"add_adjustment","composite":"lower_layers","mask":"transformed_rectangle","background":"excluded","three_d":false},"vector_drawing":{"supported":true,"protocol":1,"command":"vector","coordinates":"centered_canvas_pixels_y_down","max_paths":motion_core::vector::MAX_PATHS,"max_nodes":motion_core::vector::MAX_NODES,"fill_rules":["non_zero","even_odd"],"stroke_caps":["butt","round","square"],"stroke_joins":["miter","round","bevel"],"shape_catalog":motion_core::vector::shape_catalog()},"scene_effects":{"sdk_version":motion_effects::SDK_VERSION,"plugin_editor_protocol":1,"max_particles_per_effect":20000,"max_sprites_per_frame":65536,"occlusion":"source_alpha_planes","simulation":"analytic_world_birth_or_legacy_local_space","particle_birth_history":true,"particle_history_expressions":false,"particle_rate_animation":false},"native_plugin_ui":{"supported":true,"protocol":1,"slots":["preview","timeline","parameters","layer_source","image_sprite","seed","transform","note"],"preview":"shared_wgpu_surface","timeline":"shared_composition_clock"},"property_expressions":{"supported":true,"profile":motion_core::EXPRESSION_PROFILE,"engine":"QuickJS-NG","source_max_bytes":8192,"max_expressions":motion_core::MAX_EXPRESSIONS,"cross_property_references":false,"opacity_unit":"percent"},"camera_layers":{"supported":true,"project_format":motion_core::CURRENT_PROJECT_VERSION,"activation":"topmost_enabled_in_range","legacy_object":0},"layer_clips":true,"layer_3d":{"supported":true,"default":false,"activation":"explicit","command":"set_layer_3d"},
                "planar_intersections":{"supported":true,"method":"bsp","geometry_api":"sampleGeometryInto","max_batches":8192,"max_vertices":65536},
                "separate_dimensions":{"supported":true,"activation":"explicit",
                "layer_properties":["position","rotation","scale"],"camera_properties":camera_properties,"axes":["x","y","z"]},
                "composition":{"fps_range":[1,motion_core::MAX_COMPOSITION_FPS],"fps_presets":[24,25,30,50,60,90,120,144,240],"fps_type":"integer"},
                "project_package":crate::ops::media::package_limits(),"multiple_compositions":true,"precompose":true,"composition_api":{"version":1,"project_format":motion_core::CURRENT_PROJECT_VERSION,"max_compositions":motion_core::composition::MAX_COMPOSITIONS,"max_depth":motion_core::composition::MAX_COMPOSITION_DEPTH,"depth_includes_root":true,"max_instances":motion_core::composition::MAX_COMPOSITION_INSTANCES,"max_render_instances":motion_core::composition::MAX_RENDER_COMPOSITION_INSTANCES,"reference_3d":true,"collapse_transformations":false,"precompose_modes":["move_all_attributes"],"precompose_range":["composition"],"precompose_contiguous":true,"precompose_3d":false,"history_scope":"project"},"video_import":true,"audio_import":true,"model_import":false,"prerender":false,
                "layer_masks":{"version":1,"supported":true,"space":"source_pixels_y_down","stage":"before_image_effects","max_masks":16,"max_nodes":2048,"modes":["none","add","subtract","intersect","lighten","darken","difference"],"animated":["path","opacity","feather","expansion"],"variable_feather":false,"adjustment_masks":false},
                "video":self.video_capabilities(),
                "effect_image_inputs":{"supported":true,"sdk":6,"command":"image_input","sources":["package","asset","layer","empty"],"stages":["source","effects"],"default_stage":"effects","hidden_source":true,"feedback":false,"gpu_readbacks":0},
                "font_rasterization":{"supported":true,"system_catalogue":"NativeBridge.systemFonts","import":"NativeBridge.importFont","operations":["font_catalogue","font_import","font_atlas","font_text"],"formats":["ttf","otf","ttc"],"max_font_bytes":motion_media::fonts::MAX_FONT_BYTES,"glyph_cache_bytes":motion_media::fonts::GLYPH_CACHE_BYTES,"shaping":"ltr_kerning_wrap","variable_axes":false},
                "audio":{"supported_formats":["M4A/AAC-LC/ALAC","MP3","FLAC","Ogg/Vorbis/Opus","ADTS/AAC","WAV/PCM8/16/24/32/float","AIFF"],"sample_rates":[8000,11025,12000,16000,22050,24000,32000,44100,48000,88200,96000,176400,192000],"sample_rate_range":[8000,192000],"channels":[1,2],"device_query":"media_capabilities",
                "output_rate":48000,"output_channels":2,"pcm":"f32le_interleaved","waveform_bucket_us":10000,
                "source_limit_bytes":motion_core::storage::MAX_MEDIA_ASSET,"source_duration_limit_seconds":3600,
                "pcm_block_limit_frames":motion_media::MAX_BLOCK_FRAMES,"async_import":true,"ui_playback_integrated":true,"mp4_audio_mux_integrated":true}},
            "canRedo":self.engine.can_redo(),"observing":self.observing,"sampledCamera":camera,
            "sampledLayers":layers,"timeline_layers":original.timeline_layers(f),
            "timeline_camera":{"position":original.camera.position.timeline(0),"target":original.camera.target.timeline(0)},
            "projectedLayers":projected,"presented":self.presented,"cpuPrepareUs":self.last_cpu_us,
            "renderError":self.last_error,
            "effectErrors":self.graphics.as_ref().map(|g|&g.renderer.effect_diagnostics),
            "sampledEffects":self.scene.effects.iter().map(|e|json!({"layer":e.layer,"instance":e.instance,
                "values":e.param_ids.iter().enumerate().map(|(i,id)|(id.clone(),json!(e.values[i]))).collect::<serde_json::Map<String,Value>>(),
                "curve_lut":e.lut.map(|i|&self.scene.curve_luts[i][..])})).collect::<Vec<_>>(),
            "lastPresentedFrame":self.last_presented_frame,
            "lastPresentedRevision":self.last_presented_revision,"viewRevision":self.view_revision,
            "lastPresentedViewRevision":self.last_presented_view_revision,"surfaceEpoch":self.surface_epoch,
            "diagnosticsEnabled":cfg!(feature="diagnostics"),
            "observationView":match self.observer.view {motion_core::ObservationView::Free=>"free",motion_core::ObservationView::Top=>"top",motion_core::ObservationView::Side=>"side"},
            "preview":self.preview_info(),
            "graphics":self.graphics.as_ref().map(|g|json!({"width":g.config.width,"height":g.config.height,
                "renderWidth":g.scratch.width,"renderHeight":g.scratch.height,
                "renderTargetBytes":g.scratch.texture_bytes(),"previewImageReadbackBytes":0,
                "adapter":g.renderer.adapter_info.name,
                "backend":format!("{:?}",g.renderer.adapter_info.backend),
                "textureBytes":g.renderer.texture_bytes()}))});
        snapshot["capabilities"]["separate_dimensions"]["merge"] = json!({
            "supported":true,"command":"merge_dimensions","policy":"aligned_keys_shared_interpolation",
            "resampling":false,"on_mismatch":"error_preserves_axes","expressions":"preserved",
            "already_coupled":"no_change","project_format_change":false});
        snapshot["capabilities"]["layer_sampling"]=crate::render_sampling::capabilities();
        snapshot["capabilities"]["composition_api"]["project_format"]=json!(motion_core::CURRENT_PROJECT_VERSION);
        snapshot["capabilities"]["layer_3d"]["rotation_order"]=json!({"supported":true,"project_format":motion_core::CURRENT_PROJECT_VERSION,"command":"set_rotation_order",
            "default":"fixed_xyz","missing_field":"local_xyz","orders":["local_xyz","fixed_xyz"],"fixed_xyz":"apply_x_then_y_then_z_about_parent_axes",
            "changes_pose":true,"animated":false,"camera":false});
        snapshot["capabilities"]["layer_timeline"]["subframe_ranges"]=json!({"supported":true,"project_format":motion_core::CURRENT_PROJECT_VERSION,
            "command":"set_layer_range","field":"subframe_range","coordinates":"composition_frames",
            "in_frame_min":-(motion_core::MAX_FRAMES as i32),"out_frame_max":motion_core::MAX_FRAMES,
            "interval":"half_open","offset_precision":"integer_frames","audio_boundaries":"ceil_output_sample"});
        snapshot["capabilities"]["audio"]["levels"]=json!({"supported":true,"project_format":motion_core::CURRENT_PROJECT_VERSION,"command":"audio_levels",
            "unit":"dB","range":[motion_core::audio_levels::MIN_AUDIO_DB,motion_core::audio_levels::MAX_AUDIO_DB],"channels":["left","right"],"animated":true,
            "sampling":"output_sample","time_coordinates":"layer_local_frames","nested":true,
            "conversion":"10^(dB/20)","expressions":false,"curve_overshoot":"clamp_to_range"});
        snapshot["capabilities"]["audio"]["reference_markers"] = json!({
            "supported":true,"project_format":motion_core::CURRENT_PROJECT_VERSION,"command":"audio_markers","max_count":motion_core::audio_edit::MAX_AUDIO_MARKERS,
            "storage_coordinates":"layer_local_frames","edit_coordinates":"composition_frames",
            "generated_replacement":"preserve_manual_and_corrected","corrected_references":"retain_analysis_origin"});
        snapshot["capabilities"]["audio"]["channel_separation"] = json!({
            "supported":true,"project_format":motion_core::CURRENT_PROJECT_VERSION,"command":"split_audio_channels",
            "requires":"stereo_single_source","preserves_stereo_sum":true,"output":"left_and_right_layers"});
        snapshot["capabilities"]["audio"]["extraction"] = json!({
            "supported":true,"command":"extract_audio","requires":"video_with_audio",
            "output":"audio_layer","source":"preserved_with_audio_muted","reencode":false});
        snapshot["capabilities"]["layer_timeline"]["extended_spans"]=json!({"supported":true,"project_format":motion_core::CURRENT_PROJECT_VERSION,
            "in_frame_min":-(motion_core::MAX_FRAMES as i32),"out_frame_max":motion_core::MAX_FRAMES,
            "coordinates":"signed_composition_frames","outside_composition":"retained_not_rendered"});
        snapshot["capabilities"]["vector_drawing"]["protocol"]=json!(3);
        snapshot["capabilities"]["vector_drawing"]["groups"]=json!({"supported":true,"project_format":motion_core::CURRENT_PROJECT_VERSION,
            "max_depth":motion_core::vector::groups::MAX_DEPTH,"max_items":motion_core::vector::groups::MAX_ITEMS,
            "ordered_paints":true,"isolated_opacity":true,"repeater":true,"multiple_repeaters":true,"max_copies":1024,
            "node_parameters":{"supported":true,"key":"node:<path_id>:<node_id>","components":6},"stroke_dash_parameters":true,
            "position_coordinates":"group_local_pixels_y_down","expressions":false,"separated_axes":false,
            "viewport_roi":"2d_without_effects_masks_or_image_consumers"});
        snapshot["capabilities"]["vector_drawing"]["path_modifiers"]=json!({"project_format":motion_core::CURRENT_PROJECT_VERSION,
            "trim_paths":true,"trim_modes":["simultaneously","individually"],"trim_range_percent":[0,100],
            "trim_offset_degrees":[-360000,360000],"stroke_dashes":true,"max_dash_pairs":3,
            "dash_length_pixels":[0.1,32768],"gap_length_pixels":[0,32768],"dash_offset_pixels":[-32768,32768],
            "max_output_nodes":motion_core::vector::path_ops::MAX_OUTPUT_NODES,"evaluation_order":["source","trim","dash","paint"],
            "groups":false,"repeater":false,"zero_length_dots":false});
        snapshot["capabilities"]["layer_compositing"]=json!({"supported":true,"version":1,"project_format":motion_core::CURRENT_PROJECT_VERSION,
            "blend_command":"set_layer_blend","matte_command":"set_track_matte",
            "blend_modes":["normal","add","multiply","screen","overlay","darken","lighten","difference","exclusion","subtract","divide","color_dodge","color_burn","hard_light","soft_light"],
            "blend_spaces":["linear","srgb"],"matte_modes":["alpha","alpha_inverted","luma","luma_inverted"],
            "stage":"after_effects_transforms_opacity","background":"excluded","source_scope":"same_composition",
            "max_matte_depth":motion_core::compositing::MAX_MATTE_DEPTH,"source_deletion":"reject_while_referenced","three_d":true});
        snapshot
    }
}
