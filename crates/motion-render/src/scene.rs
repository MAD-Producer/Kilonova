use motion_model::{to_world, ensure, CameraPose, Content, Layer, Observer, Project, Result};
use glam::{Mat4, Vec3};
use serde::Serialize;
pub(crate) mod instance_ids;
mod depth;
mod view;
pub use view::ViewCamera;
// Bounded authored payload per scene; old/new samples may coexist during refresh.
const MAX_CURVE_SOURCE_BYTES: usize = 32 * 1024 * 1024;

/// Add the reference site while preserving the model's original error code,
/// target composition, message and time/rate details.
pub(crate) fn composition_source_frame(
    clip: &motion_model::CompositionClip,
    local_frame: f64,
    parent_fps: u32,
    child_fps: u32,
    parent: &str,
    reference: u64,
    phase: &str,
) -> Result<f64> {
    clip.source_frame(local_frame, parent_fps, child_fps).map_err(|mut error| {
        if let motion_model::Error::Composition(detail) = &mut error {
            detail.details["parent_composition"] = parent.into();
            detail.details["reference"] = reference.into();
            detail.details["phase"] = phase.into();
        }
        error
    })
}

#[derive(Clone, Debug, Serialize)]
pub struct HitCandidate {
    pub id: u64,
    pub uv: [f32; 2],
    pub depth: f32,
    pub order: usize,
    pub three_d: bool,
}

#[derive(Clone, Debug)]
pub struct DrawLayer {
    pub id: u64,
    pub model: Mat4,
    pub size: [f32; 2],
    pub source_size: [f32; 2],
    pub source_rect: [f32; 4],
    pub color: [f32; 4],
    pub opacity: f32,
    pub asset: Option<u64>,
    pub model_source: Option<motion_model::ModelClip>,
    pub model_animation: Option<motion_scene::AnimationSample>,
    pub video: Option<motion_model::VideoSample>,
    pub vector: Option<std::sync::Arc<motion_model::vector::SampledVector>>,
    pub masks: std::sync::Arc<Vec<motion_model::masks::SampledMask>>,
    pub blend:motion_model::compositing::LayerBlend,
    pub track_matte:Option<motion_model::compositing::TrackMatte>,
    pub composite_visible:bool,
    pub adjustment: bool,
    pub composition: bool,
    pub depth: f32,
    pub order: usize,
    pub three_d: bool,
    /// The generated raster is already in composition pixels. Logical model,
    /// node transforms and three_d remain available for editing/emitter poses.
    pub screen_overlay: bool,
    pub sampling: motion_model::render_sampling::LayerSampling,
    /// 2D layers use a fixed composition projection; 3D layers use the camera.
    pub view_projection: Mat4,
}

pub(crate) fn effect_has_output(effect: &motion_model::SampledEffect) -> bool {
    effect.enabled && effect.param_ids.iter().position(|id| id == "effect_opacity")
        .is_none_or(|index| effect.values[index][0] != 0.)
}
pub(crate) fn effect_is_screen_generator(effect: &motion_model::SampledEffect) -> bool {
    // The validated SDK contract requires scene state for every sprite renderer
    // and forbids it for image renderers; opacity zero is the wrapper identity.
    effect.scene.is_some() && effect_has_output(effect)
}
pub(crate) fn plane_model(layer: &DrawLayer, overlay: bool) -> Mat4 {
    if overlay { Mat4::IDENTITY } else { layer.model }
}
pub(crate) fn plane_projection(scene: &Scene, layer: &DrawLayer, overlay: bool) -> Mat4 {
    if overlay { Mat4::from_scale(Vec3::new(2. / scene.width as f32, 2. / scene.height as f32, 1.)) }
    else { layer.view_projection }
}
/// Canonical UVs invert the logical composition projection in f64. Promoting
/// the already-rounded f32 2/width coefficient instead would introduce a tiny
/// scale even in an authored identity draw, blending adjacent high-contrast
/// texels on full-precision consumers. Custom and camera projections retain
/// their supplied matrix; the GPU draw ABI remains f32.
pub(crate) fn plane_uv_projection(scene: &Scene, layer: &DrawLayer, overlay: bool) -> glam::DMat4 {
    let projection = plane_projection(scene, layer, overlay);
    let flat = Mat4::orthographic_rh(
        -(scene.width as f32) * 0.5, scene.width as f32 * 0.5,
        -(scene.height as f32) * 0.5, scene.height as f32 * 0.5, -1., 1.,
    );
    let mut precise = projection.as_dmat4();
    if overlay || (!layer.three_d && projection == flat) {
        precise.x_axis.x = 2. / f64::from(scene.width);
        precise.y_axis.y = 2. / f64::from(scene.height);
    }
    precise
}
pub(crate) fn plane_size(scene: &Scene, layer: &DrawLayer, overlay: bool) -> [f32; 2] {
    if overlay { [scene.width as f32, scene.height as f32] } else { layer.size }
}
#[derive(Clone, Debug)]
pub struct Scene {
    pub camera: CameraPose,
    pub camera_viewport: [f32; 4],
    camera_sample: Option<motion_model::CameraSample>,
    model_sources: crate::ModelSourceCatalog,
    depth_required: std::collections::HashMap<u64, motion_model::depth::SavedDepthCalibration>,
    depth_surfaces: std::collections::HashMap<u64, depth::BoundSurface>,
    depth_pending: std::collections::HashSet<u64>,
    pub(crate) shared_rasters: std::collections::HashSet<u64>,
    node_cameras: Vec<Option<motion_model::CameraSample>>,
    pub layers: Vec<DrawLayer>,
    pub background: [f32; 4],
    pub width: u32,
    pub height: u32,
    pub effects: Vec<motion_model::SampledEffect>,
    pub curve_luts: Vec<motion_model::CurveLut>,
    pub(crate) curve_sources: Vec<motion_model::SampledColorCurve>,
    pub frame: f64,
    pub fps: u32,
    pub sprite_assets: std::collections::HashMap<u64, [u32;2]>,
    pub composition_id: String,
    pub nested: Vec<NestedScene>,
    nested_spare: Vec<NestedScene>,
    node_world: Vec<Mat4>,
    node_states: Vec<u8>,
    node_ids: Vec<u64>,
    node_spatial: Vec<bool>,
    source_objects:Vec<(u64,u64)>,
    instance_ids: instance_ids::InstanceIds,
    evaluated_project: Option<Project>,
    vector_cache: std::collections::HashMap<
        u64,
        (
            motion_model::vector::VectorContent,
            [f32; 2],
            Option<f64>,
            std::sync::Arc<motion_model::vector::SampledVector>,
        ),
    >,
    mask_cache: std::collections::HashMap<u64, (Vec<motion_model::masks::LayerMask>, Option<f64>, std::sync::Arc<Vec<motion_model::masks::SampledMask>>)>,
}
#[derive(Clone, Debug)]
pub struct NestedScene {
    pub layer: u64,
    pub composition: String,
    pub scene: Box<Scene>,
}
impl Scene {
    /// Install immutable source ownership before sampling, including child scenes.
    pub fn set_model_source_catalog(&mut self, catalog: crate::ModelSourceCatalog) { self.model_sources = catalog; }
    pub fn model_source_catalog(&self) -> crate::ModelSourceCatalog { self.model_sources.clone() }
    pub fn native_camera(&self) -> CameraPose {
        if self.camera_viewport == [0., 0., self.width as f32, self.height as f32] { return self.camera; }
        self.camera_sample.map_or(self.camera, |sample| sample.pose)
    }
    /// Already sampled in the active camera's local animation clock. Observer
    /// views have no authored aperture, just as they replace the authored pose.
    pub fn native_depth_of_field(&self) -> Option<motion_model::SampledDepthOfField> {
        self.camera_sample.and_then(|sample| sample.depth_of_field)
            .filter(|sample| sample.aperture_diameter > 0.)
    }
    /// Portable plane consumers cannot preserve an active native aperture,
    /// including one sampled by a child composition's independent camera.
    pub fn has_native_depth_of_field(&self) -> bool {
        self.native_depth_of_field().is_some()
            || self.nested.iter().any(|nested| nested.scene.has_native_depth_of_field())
    }
    pub(crate) fn native_camera_viewport(&self, width: u32, height: u32) -> Option<[f32; 4]> {
        if self.camera_viewport == [0., 0., self.width as f32, self.height as f32] { return None; }
        let [x,y,w,h] = self.camera_viewport;
        Some([x * width as f32 / self.width as f32, y * height as f32 / self.height as f32,
            w * width as f32 / self.width as f32, h * height as f32 / self.height as f32])
    }
    pub fn is_screen_overlay(&self, layer: u64) -> bool {
        if self.shared_rasters.contains(&layer) { return true; }
        if self.layers.iter().any(|l| l.id == layer && l.model_source.is_some()) { return true; }
        self.effects.iter().any(|effect| effect.layer == layer && effect_is_screen_generator(effect))
    }
    /// Create an empty, unsampled context. Imported cameras require a catalog
    /// before `sample`; the initial empty pose is never a source-camera fallback.
    pub fn new(project: &Project) -> motion_model::Result<Self> {
        let initial = if project.camera.imported.is_some() { motion_model::Camera::new(project.width, project.height).pose(0., project.width, project.height)? } else { project.camera.pose(0., project.width, project.height)? };
        Ok(Self::empty(initial, project.background,
            project.width, project.height, project.fps, project.composition_id.clone()))
    }
    /// Create the empty context before a host changes its active composition.
    /// Install source ownership and sample before using it for content rendering.
    pub fn for_composition(project: &Project, id: &str) -> motion_model::Result<Self> {
        let view = project.composition_view(id)?;
        let initial = if view.camera.imported.is_some() { motion_model::Camera::new(view.width, view.height).pose(0., view.width, view.height)? } else { view.camera.pose(0., view.width, view.height)? };
        Ok(Self::empty(initial, view.background,
            view.width, view.height, view.fps, view.id.to_owned()))
    }
    pub(crate) fn plane_context(&self, layers: Vec<DrawLayer>) -> Self {
        let mut context = Self::empty(self.camera, self.background, self.width, self.height,
            self.fps, self.composition_id.clone());
        context.camera_viewport = self.camera_viewport;
        context.camera_sample = self.camera_sample;
        context.model_sources = self.model_sources.clone();
        context.depth_required = self.depth_required.clone();
        context.depth_surfaces = self.depth_surfaces.clone();
        context.depth_pending = self.depth_pending.clone();
        context.shared_rasters = self.shared_rasters.clone();
        context.frame = self.frame;
        context.layers = layers;
        context
    }
    /// Render-ready local context after the caller has independently prepared
    /// every nested image. Keep evaluated world/source data used by generators,
    /// but do not let the ordinary nested pass overwrite those GPU images.
    pub(crate) fn local_context(&self) -> Self {
        let mut context = self.plane_context(self.layers.clone());
        context.effects = self.effects.clone();
        context.curve_luts = self.curve_luts.clone();
        context.curve_sources = self.curve_sources.clone();
        context.sprite_assets = self.sprite_assets.clone();
        context.node_world = self.node_world.clone();
        context.node_ids = self.node_ids.clone();
        context.node_states = self.node_states.clone();
        context.node_spatial = self.node_spatial.clone();
        context.source_objects = self.source_objects.clone();
        context
    }
    fn empty(camera: CameraPose, background: [f32; 4], width: u32, height: u32, fps: u32, composition_id: String) -> Self {
        Self {
            camera,
            camera_viewport: [0., 0., width as f32, height as f32],
            camera_sample: None,
            model_sources: Default::default(),
            depth_required: Default::default(),
            depth_surfaces: Default::default(),
            depth_pending: Default::default(),
            shared_rasters: Default::default(),
            node_cameras: Vec::new(),
            layers: Vec::with_capacity(motion_model::MAX_LAYERS),
            background,
            width,
            height,
            effects: Vec::with_capacity(motion_model::MAX_LAYERS * motion_effects::MAX_EFFECTS_PER_LAYER),
            curve_luts: Vec::new(),
            curve_sources: Vec::new(),
            frame: 0.0,
            fps,
            sprite_assets: Default::default(),
            composition_id,
            nested: Vec::new(),
            nested_spare: Vec::new(),
            node_world: Vec::with_capacity(motion_model::MAX_LAYERS + 1),
            node_states: Vec::with_capacity(motion_model::MAX_LAYERS + 1),
            node_ids: Vec::with_capacity(motion_model::MAX_LAYERS + 1),
            node_spatial: Vec::with_capacity(motion_model::MAX_LAYERS + 1),
            source_objects:Vec::new(),
            instance_ids: Default::default(),
            evaluated_project: None,
            vector_cache: Default::default(),
            mask_cache: Default::default(),
        }
    }
    pub fn sample(
        &mut self,
        project: &Project,
        frame: f64,
        observer: Option<&Observer>,
        evaluator: &dyn motion_model::FrameEvaluator,
    ) -> Result<()> {
        self.sample_root(project, project, frame, observer, evaluator)
    }
    /// Explicit view selection belongs to this scene, never the document or a
    /// session-global observer. Child compositions retain their own cameras.
    pub fn sample_view(&mut self, project: &Project, frame: f64,
        view: ViewCamera<'_>, evaluator: &dyn motion_model::FrameEvaluator) -> Result<()> {
        self.sample_view_root(project, project, frame, view, evaluator)
    }
    /// A composition sampling view resolves resources and nested references
    /// against its immutable owning document, never as an independent project.
    pub(crate) fn sample_root(&mut self, document: &Project, project: &Project, frame: f64,
        observer: Option<&Observer>, evaluator: &dyn motion_model::FrameEvaluator) -> Result<()> {
        self.sample_view_root(document, project, frame, ViewCamera::from(observer), evaluator)
    }
    fn sample_view_root(&mut self, document: &Project, project: &Project, frame: f64,
        view: ViewCamera<'_>, evaluator: &dyn motion_model::FrameEvaluator) -> Result<()> {
        self.sample_inner(document, project, frame, view, 0, &mut 0, evaluator)?;
        if self.nested.is_empty() { return Ok(()); }
        self.instance_ids.prepare_composition(document, project)?;
        let aliases = &self.instance_ids;
        let mut path = Vec::with_capacity(motion_model::composition::MAX_COMPOSITION_DEPTH);
        for node in &mut self.nested {
            path.push(node.layer);
            let result = node.scene.assign_instance_ids(aliases, &mut path);
            path.pop();
            result?;
        }
        Ok(())
    }
    pub(crate) fn sample_exposure_instance(&mut self, document:&Project, project:&Project,
        frame:f64, view:ViewCamera<'_>, evaluator:&dyn motion_model::FrameEvaluator,
        registry:&instance_ids::InstanceIds, path:&[u64]) -> Result<()> {
        self.sample_inner(document,project,frame,view,0,&mut 0,evaluator)?;
        let mut path = path.to_vec();
        if !path.is_empty() { return self.assign_instance_ids(registry,&mut path); }
        for node in &mut self.nested {
            path.push(node.layer);
            let result=node.scene.assign_instance_ids(registry,&mut path);
            path.pop();
            result?;
        }
        Ok(())
    }
    fn sample_inner(&mut self, document:&Project, project:&Project, frame:f64, view:ViewCamera<'_>, depth:usize, instances:&mut usize, evaluator:&dyn motion_model::FrameEvaluator)->Result<()> {
        ensure(depth<motion_model::composition::MAX_COMPOSITION_DEPTH,"composition nesting too deep")?;
        if *instances >= motion_model::composition::MAX_RENDER_COMPOSITION_INSTANCES {
            return motion_model::composition::fail(&project.composition_id, "render_resource_limit",
                "Active composition instance limit exceeded",
                serde_json::json!({"max_render_instances":motion_model::composition::MAX_RENDER_COMPOSITION_INSTANCES,
                                   "requested_instances":*instances+1,"frame":frame}));
        }
        *instances += 1;
        ensure(
            frame.is_finite() && frame >= 0.0 && frame < f64::from(project.frames),
            "invalid sample time",
        )?;
        let original = project;
        let evaluated = evaluator.evaluate(project, frame)?;
        let project = evaluated.as_ref();
        self.frame = frame;
        self.fps = project.fps;
        self.sprite_assets.clear();
        self.sprite_assets.extend(project.assets.iter().map(|a| (a.id,[a.width,a.height])));
        self.composition_id.clone_from(&project.composition_id);
        self.reset_depth();
        let mut previous_nested = std::mem::take(&mut self.nested);
        for node in &mut previous_nested { node.layer = self.source_object(node.layer); }
        // Alias IDs belong to the last rendered instance, not to authored tracks.
        for effect in &mut self.effects {
            effect.layer = self.source_objects.iter().find(|v| v.0 == effect.layer).map_or(effect.layer, |v| v.1);
        }
        self.nested = std::mem::take(&mut self.nested_spare);
        self.source_objects.clear();
        self.curve_luts.clear();
        let previous_curve_sources = std::mem::take(&mut self.curve_sources);
        let mut curve_source_bytes = 0;
        let mut effect_index = 0;
        for layer in &project.layers {
            for e in &layer.effects {
                if effect_index >= self.effects.len() {
                    self.effects.push(motion_model::SampledEffect::new(layer.id, e));
                } else if !self.effects[effect_index].matches(layer.id, e) {
                    self.effects[effect_index] = motion_model::SampledEffect::new(layer.id, e);
                }
                let sampled = &mut self.effects[effect_index];
                sampled.local_frame = layer.local_frame(frame);
                sampled.enabled = e.enabled && layer.effects_enabled;
                sampled.seed = e.seed;
                sampled.image_input = e.image_input;
                if !match (&sampled.shader_data, &e.shader_data) {
                    (Some(a),Some(b)) => std::sync::Arc::ptr_eq(a,b), (None,None) => true, _ => false,
                } { sampled.shader_data = e.shader_data.clone(); }
                sampled.scene = e.scene.clone();
                sampled.lut = None;
                sampled.particle_history_error = None;
                if e.params.len() > motion_effects::MAX_PARAMS {
                    return Err(motion_model::Error::Invalid(format!(
                        "effect {} on layer {} has too many parameters (max {})",
                        e.id,
                        layer.id,
                        motion_effects::MAX_PARAMS
                    )));
                }
                if sampled.enabled && e.scene.as_ref().is_some_and(|s| s.particle_space.is_some()) {
                    let raw_layer = original.layers.iter().find(|l| l.id == layer.id).ok_or_else(|| {
                        motion_model::Error::Invalid(format!(
                            "particle effect layer {} is missing from the source project",
                            layer.id
                        ))
                    })?;
                    let raw_effect = raw_layer.effects.iter().find(|v| v.id == e.id).ok_or_else(|| {
                        motion_model::Error::Invalid(format!(
                            "particle effect {} is missing from source layer {}",
                            e.id, layer.id
                        ))
                    })?;
                    match motion_model::particle_history::ParticleHistory::capture(original, raw_layer, raw_effect, sampled.particle_history.as_ref()) {
                        Ok(history) => sampled.particle_history = Some(history),
                        Err(error) => {
                            sampled.particle_history = None;
                            sampled.particle_history_error = Some(std::sync::Arc::new(error));
                        }
                    }
                } else {
                    sampled.particle_history = None;
                }
                for (i, p) in e.params.values().enumerate() {
                    sampled.values[i] = p.sample(sampled.local_frame)?;
                    if let Some(c) = p.curve.as_ref().filter(|_| sampled.enabled) {
                        let source = c.sampled_curve_with_budget(
                            sampled.local_frame,
                            previous_curve_sources.get(self.curve_sources.len()),
                            MAX_CURVE_SOURCE_BYTES.saturating_sub(curve_source_bytes),
                        )?;
                        curve_source_bytes += source.source_bytes();
                        let lut = source.preview_lut();
                        sampled.lut = Some(self.curve_luts.len());
                        self.curve_luts.push(lut);
                        self.curve_sources.push(source);
                    }
                }
                effect_index += 1;
            }
        }
        self.effects.truncate(effect_index);
        motion_model::hierarchy::matrices_with_cameras(project, frame, &mut self.node_world,
            &mut self.node_states, &self.model_sources, &mut self.node_cameras)?;
        self.node_ids.clear();
        self.node_ids.extend(project.layers.iter().map(|l| l.id));
        self.node_ids.push(0);
        self.node_spatial.clear();
        self.node_spatial.extend(project.layers.iter().map(|l| l.three_d));
        self.node_spatial.push(true);
        self.select_view(project, frame, view)?;
        self.width = project.width;
        self.height = project.height;
        self.background = project.background;
        self.layers.clear();
        self.mask_cache.retain(|id,_| project.layers.iter().any(|l| l.id == *id));
        self.vector_cache.retain(|id, _| {
            project
                .layers
                .iter()
                .any(|l| l.id == *id && matches!(l.content, Content::Vector { .. }))
        });
        let forward = (self.camera.target - self.camera.eye).normalize();
        let flat_projection = Mat4::orthographic_rh(
            -(project.width as f32) * 0.5,
            project.width as f32 * 0.5,
            -(project.height as f32) * 0.5,
            project.height as f32 * 0.5,
            -1.0,
            1.0,
        );
        let matte_sources=motion_model::compositing::matte_sources(&project.layers);
        let hidden_sources=motion_model::compositing::hidden_sources(&project.layers);
        let solo_active = project.solo_active();
        for (order, layer) in project.layers.iter().enumerate() {
            if matches!(layer.content, Content::Null | Content::Camera { .. } | Content::Audio { .. }) {
                continue;
            }
            let referenced = matte_sources.contains(&layer.id) || self.effects.iter().any(|e| e.enabled && e.image_input.and_then(motion_model::EffectImageInput::layer) == Some(layer.id));
            let clip=layer.interval(project.frames);
            let output_enabled = layer.visible && (!solo_active || layer.solo);
            if frame<clip.in_frame||frame>=clip.out_frame||(!output_enabled&&!referenced) {
                continue;
            }
            let opacity = layer
                .transform
                .opacity
                .sample(layer.local_frame(frame))?
                .clamp(0.0, 1.0);
            if opacity <= 0.0 && !referenced {
                continue;
            }
            let center = self.node_world[order].w_axis.truncate();
            if let Content::Composition { clip } = &layer.content {
                let child=document.composition_frame_view(&clip.composition)?;
                let source=composition_source_frame(clip,layer.local_frame(frame),project.fps,child.fps,
                    &project.composition_id,layer.id,"scene_sample")?;
                if source<0.||source>=f64::from(child.frames) {continue;}
                let mut node = if let Some(index) = previous_nested.iter().position(|n| n.layer == layer.id && n.composition == clip.composition) {
                    previous_nested.swap_remove(index)
                } else {
                    NestedScene {layer:layer.id, composition:clip.composition.clone(), scene:Box::new(Scene::new(&child)?)}
                };
                node.scene.model_sources = self.model_sources.clone();
                if let Err(error)=node.scene.sample_inner(document,&child,source,ViewCamera::Composition,depth+1,instances,evaluator){
                    if matches!(&error, motion_model::Error::Composition(e) if e.code == "render_resource_limit") { return Err(error); }
                    return motion_model::composition::fail(&clip.composition,"sample_failed",&error.to_string(),serde_json::json!({"frame":source,"parent_composition":project.composition_id,"reference":layer.id}));
                }
                self.nested.push(node);
            }
            let video = if let Content::Video { video } = &layer.content {
                let a = project
                    .video_assets
                    .iter()
                    .find(|a| a.id == video.asset)
                    .ok_or_else(|| {
                        motion_model::Error::Invalid(format!(
                            "video asset {} is missing for layer {}",
                            video.asset, layer.id
                        ))
                    })?;
                let time = video.picture_time_us(layer.local_frame(frame), project.fps, a);
                if time < a.video_start_us as i64 || time >= a.video_end_us as i64 {
                    continue;
                }
                Some(motion_model::VideoSample {
                    asset: a.id,
                    source_time_us: time as u64,
                })
            } else {
                None
            };
            let (color, asset) = match &layer.content {
                Content::Null | Content::Camera { .. } | Content::Audio { .. } => unreachable!(),
                Content::Solid { color } => (*color, None),
                Content::Video { .. } => ([1.0; 4], None),
                Content::Vector { .. } | Content::Adjustment => ([1.0; 4], None),
                Content::Composition { .. } => ([1.0;4],None),
                Content::Model { .. } => ([1.0;4],None),
                Content::Image { asset } => ([1.0; 4], Some(*asset)),
                Content::Text {
                    color,
                    raster_asset,
                    ..
                } => (*color, Some(*raster_asset)),
            };
            let vector = if let Content::Vector { vector } = &layer.content {
                let time = vector.animated().then_some(layer.local_frame(frame));
                let dirty = self
                    .vector_cache
                    .get(&layer.id)
                    .is_none_or(|(v, size, t, _)| v != vector || *size != layer.size || *t != time);
                if dirty {
                    self.vector_cache.insert(
                        layer.id,
                        (
                            vector.clone(),
                            layer.size,
                            time,
                            std::sync::Arc::new(
                                vector
                                    .sample(layer.local_frame(frame), layer.size)
                                    .map_err(|e| {
                                        motion_model::Error::Invalid(format!(
                                            "layer {} vector: {e}",
                                            layer.id
                                        ))
                                    })?,
                            ),
                        ),
                    );
                }
                Some(self.vector_cache[&layer.id].3.clone())
            } else {
                None
            };
            let mut size = layer.size;
            let mut vector_center = Vec3::ZERO;
            let mask_time = layer.masks.iter().any(|m| m.animated()).then_some(layer.local_frame(frame));
            if self.mask_cache.get(&layer.id).is_none_or(|(m,t,_)| m != &layer.masks || *t != mask_time) {
                let masks = layer.masks.iter().map(|m| m.sample(layer.local_frame(frame)))
                    .collect::<Result<Vec<_>>>().map_err(|e| motion_model::Error::Invalid(format!("layer {} masks at frame {frame}: {e}",layer.id)))?
                    .into_iter().flatten().collect();
                self.mask_cache.insert(layer.id,(layer.masks.clone(),mask_time,std::sync::Arc::new(masks)));
            }
            let masks = self.mask_cache[&layer.id].2.clone();
            if let Some(v) = &vector {
                if let Some(mut bounds) = crate::vector_mesh::group_bounds(v) {
                    let dependencies = project.layers.iter().any(|l| l.effects.iter().any(|e|
                        e.enabled && e.image_input.and_then(motion_model::EffectImageInput::layer)==Some(layer.id)));
                    let world = self.node_world[order] * geometry_offset(layer, frame)?;
                    let m = glam::Mat3::from_cols(world.x_axis.truncate().truncate().extend(0.),
                        world.y_axis.truncate().truncate().extend(0.),world.w_axis.truncate().truncate().extend(1.));
                    let spatial_input = (layer.effects_enabled && layer.effects.iter().any(|e|e.enabled)) || !layer.masks.is_empty() || dependencies;
                    if !layer.three_d && !spatial_input && m.determinant().abs()>1e-8
                        && world.x_axis.w.abs()<1e-8 && world.y_axis.w.abs()<1e-8 && (world.w_axis.w-1.).abs()<1e-8 {
                        let inverse=m.inverse();let mut min=glam::Vec2::splat(f32::INFINITY);let mut max=glam::Vec2::splat(f32::NEG_INFINITY);
                        for x in [-(self.width as f32)*0.5,self.width as f32*0.5] {
                            for y in [-(self.height as f32)*0.5,self.height as f32*0.5] {
                                let p=inverse.transform_point2(glam::Vec2::new(x,y));let p=glam::Vec2::new(p.x,-p.y);
                                min=min.min(p-glam::Vec2::ONE);max=max.max(p+glam::Vec2::ONE);
                            }
                        }
                        let a=glam::Vec2::new(bounds[0],bounds[1]).max(min);
                        let b=glam::Vec2::new(bounds[0]+bounds[2],bounds[1]+bounds[3]).min(max);
                        bounds=if b.x>a.x&&b.y>a.y {[a.x,a.y,b.x-a.x,b.y-a.y]}else{[-0.5,-0.5,1.,1.]};
                    } else if spatial_input {
                        let x=bounds[0].min(-layer.size[0]*0.5);let y=bounds[1].min(-layer.size[1]*0.5);
                        bounds=[x,y,(bounds[0]+bounds[2]).max(layer.size[0]*0.5)-x,(bounds[1]+bounds[3]).max(layer.size[1]*0.5)-y];
                    }
                    let right=(bounds[0]+bounds[2]).ceil();let bottom=(bounds[1]+bounds[3]).ceil();
                    bounds[0]=bounds[0].floor();bounds[1]=bounds[1].floor();bounds[2]=(right-bounds[0]).max(1.);bounds[3]=(bottom-bounds[1]).max(1.);
                    size=[bounds[2],bounds[3]];
                    vector_center=Vec3::new(bounds[0]+size[0]*0.5,-bounds[1]-size[1]*0.5,0.);
                }
                let pad = v.stroke.map_or(0., |s| {
                    s.1 * 0.5
                        * if s.3 == motion_model::vector::LineJoin::Miter {
                            s.4
                        } else {
                            1.
                        }
                });
                for n in v.paths.iter().flat_map(|p| &p.nodes) {
                    for axis in 0..2 {
                        let extent = n[axis]
                            .abs()
                            .max((n[axis] + n[axis + 2]).abs())
                            .max((n[axis] + n[axis + 4]).abs());
                        size[axis] = size[axis].max(2. * (extent + pad + 1.));
                    }
                }
            }
            let screen_overlay = matches!(layer.content, Content::Model { .. }) || self.is_screen_overlay(layer.id);
            // Native model sources are already rasterized through this
            // composition's camera. Their image-domain extent is independent
            // of the authored model bounds used by its world transform.
            let model_raster = matches!(layer.content, Content::Model { .. });
            let source_size = if model_raster {
                [self.width as f32, self.height as f32]
            } else {
                layer.size
            };
            if layer.three_d {
                let calibration = if let Some(track) = &layer.video_depth_calibration {
                    let source = video.as_ref().ok_or_else(|| motion_model::Error::Invalid(
                        format!("video depth object={} phase=sample: original video source is absent", layer.id)))?;
                    let sample = track.at(source.source_time_us).ok_or_else(|| motion_model::Error::Invalid(
                        format!("video depth object={} source={} target_us={} phase=sample: no indexed calibration interval", layer.id, source.asset, source.source_time_us)))?;
                    motion_model::ensure(sample.calibration().reference().is_some(),
                        &format!("video depth object={} phase=sample: reference camera is missing", layer.id))?;
                    Some(sample.calibration())
                } else { layer.depth_calibration.as_ref().filter(|c| c.reference().is_some()) };
                if let Some(calibration) = calibration {
                    self.depth_required.insert(layer.id, calibration.clone());
                    self.depth_pending.insert(layer.id);
                }
            }
            self.layers.push(DrawLayer {
                id: layer.id,
                model: self.node_world[order] * geometry_offset(layer, frame)? * Mat4::from_translation(vector_center),
                size,
                source_size,
                source_rect: if model_raster { [0., 0., source_size[0], source_size[1]] } else { [
                    (layer.size[0] - size[0]) * 0.5 + vector_center.x,
                    (layer.size[1] - size[1]) * 0.5 - vector_center.y,
                    size[0],
                    size[1],
                ] },
                color,
                opacity,
                asset,
                model_source: if let Content::Model { model } = &layer.content { Some(model.clone()) } else { None },
                model_animation: if let Content::Model { model } = &layer.content {
                    model.animation.as_ref().map(|animation| animation.sample_seconds(layer.local_frame(frame), project.fps)
                        .map(|seconds| motion_scene::AnimationSample { animation: animation.clip as usize, seconds })).transpose()?
                } else { None },
                video,
                vector,
                masks,
                blend:layer.blend,track_matte:layer.track_matte,
                composite_visible:output_enabled&&!hidden_sources.contains(&layer.id),
                adjustment: matches!(layer.content, Content::Adjustment),
                composition: matches!(layer.content,Content::Composition {..}),
                depth: (center - self.camera.eye).dot(forward),
                order,
                three_d: layer.three_d,
                screen_overlay,
                sampling: layer.sampling,
                view_projection: if layer.three_d {
                    self.camera.view_projection
                } else {
                    flat_projection
                },
            });
        }
        // A visible 2D layer is a compositing boundary between spatial groups.
        let mut start = 0;
        while start < self.layers.len() {
            if !self.layers[start].three_d || self.layers[start].screen_overlay {
                start += 1;
                continue;
            }
            let end = self.layers[start..]
                .iter()
                .position(|l| !l.three_d || l.screen_overlay)
                .map_or(self.layers.len(), |n| start + n);
            self.layers[start..end]
                .sort_unstable_by(|a, b| b.depth.total_cmp(&a.depth).then(a.order.cmp(&b.order)));
            start = end;
        }
        previous_nested.clear();
        self.depth_surfaces.retain(|id, surface| self.depth_required.get(id) == Some(&surface.calibration));
        self.nested_spare = previous_nested;
        self.evaluated_project = match evaluated {
            std::borrow::Cow::Owned(p) => Some(p),
            std::borrow::Cow::Borrowed(p) if std::ptr::eq(p, original) => None,
            std::borrow::Cow::Borrowed(p) => Some(p.clone()),
        };
        Ok(())
    }
    fn assign_instance_ids(&mut self, registry: &instance_ids::InstanceIds,
        path: &mut Vec<u64>) -> Result<()> {
        let mapping = registry.mapping(path, &self.composition_id, &self.node_ids)?;
        let alias = |object| instance_ids::alias(&mapping, &self.composition_id, object);
        self.depth_required = std::mem::take(&mut self.depth_required).into_iter().map(|(id, value)| Ok((alias(id)?, value))).collect::<Result<_>>()?;
        self.depth_surfaces = std::mem::take(&mut self.depth_surfaces).into_iter().map(|(id, value)| Ok((alias(id)?, value))).collect::<Result<_>>()?;
        self.depth_pending = std::mem::take(&mut self.depth_pending).into_iter().map(alias).collect::<Result<_>>()?;
        for id in &mut self.node_ids { if *id!=0 { *id=alias(*id)?; } }
        self.source_objects.extend(mapping.iter().map(|(source,alias)|(*alias,*source)));
        for l in &mut self.layers {l.id=alias(l.id)?;if let Some(m)=&mut l.track_matte{m.source=alias(m.source)?;}}
        for e in &mut self.effects {
            e.layer=alias(e.layer)?;
            if let Some(motion_model::EffectImageInput::Layer { layer, .. }) = &mut e.image_input { *layer=alias(*layer)?; }
            if let Some(source)=e.scene.as_mut().and_then(|s|s.source_layer.as_mut()) { *source=alias(*source)?; }
        }
        for n in &mut self.nested {
            path.push(n.layer);
            n.layer=alias(n.layer)?;
            let result=n.scene.assign_instance_ids(registry,path);
            path.pop();
            result?;
        }
        Ok(())
    }
    pub fn video_layers(&self) -> Vec<&DrawLayer> {
        let mut layers:Vec<_>=self.layers.iter().filter(|l|l.video.is_some()).collect();
        for node in &self.nested {layers.extend(node.scene.video_layers());} layers
    }
    pub fn source_object(&self,id:u64)->u64 {self.source_objects.iter().find(|v|v.0==id).map_or(id,|v|v.1)}
    pub fn diagnostic(&self,message:&str)->String {
        let mut text=message.to_string();for (alias,source) in &self.source_objects{text=text.replace(&format!("layer {alias},"),&format!("layer {source},"));}
        format!("composition {}: {text}",self.composition_id)
    }
    /// Computed numeric values for UI snapshots; original project tracks stay intact.
    pub fn sampled_project<'a>(&'a self, original: &'a Project) -> &'a Project {
        self.evaluated_project.as_ref().unwrap_or(original)
    }
    pub fn node_position(&self, id: u64) -> Option<[f32; 3]> {
        self.node_ids.iter().position(|v| *v == id).map(|i| {
            motion_model::to_project(
                self.node_world[i].w_axis.truncate(),
                self.width,
                self.height,
            )
        })
    }
    pub fn world_matrix(&self, id: u64) -> Option<Mat4> {
        self.node_ids
            .iter()
            .position(|v| *v == id)
            .map(|i| self.node_world[i])
    }
    pub fn project_point(&self, point: [f32; 3]) -> [f32; 3] {
        let clip =
            self.camera.view_projection * to_world(point, self.width, self.height).extend(1.0);
        let ndc = clip.truncate() / clip.w;
        [
            (ndc.x * 0.5 + 0.5) * self.width as f32,
            (0.5 - ndc.y * 0.5) * self.height as f32,
            ndc.z,
        ]
    }
    pub fn project_node(&self, id: u64) -> Option<[f32; 3]> {
        let point = self.node_position(id)?;
        let spatial = self.node_is_spatial(id)?;
        Some(if spatial {
            self.project_point(point)
        } else {
            [point[0], point[1], 0.5]
        })
    }
    pub fn node_is_spatial(&self, id: u64) -> Option<bool> {
        self.node_ids
            .iter()
            .position(|v| *v == id)
            .map(|i| self.node_spatial[i])
    }
    /// Bounds picking ordered at this exact pixel, rather than by layer centre.
    /// Texture alpha is deliberately not read back; transparent bounds remain selectable.
    pub fn hit_candidates(&self, point: [f32; 2]) -> Result<Vec<HitCandidate>> {
        ensure(
            point.into_iter().all(f32::is_finite),
            "invalid hit-test point",
        )?;
        let x = point[0] / self.width as f32 * 2.0 - 1.0;
        let y = 1.0 - point[1] / self.height as f32 * 2.0;
        let mut hits = Vec::new();
        let mut start = 0;
        while start < self.layers.len() {
            let spatial = |l: &DrawLayer| l.three_d && !self.is_screen_overlay(l.id);
            let end = if spatial(&self.layers[start]) {
                self.layers[start..]
                    .iter()
                    .position(|l| !spatial(l))
                    .map_or(self.layers.len(), |n| start + n)
            } else {
                start + 1
            };
            let group_start = hits.len();
            for l in &self.layers[start..end] {
                if l.opacity <= 0.0 || !l.composite_visible { continue; }
                let [vx, vy, vw, vh] = self.camera_viewport;
                if l.three_d && (point[0] < vx || point[0] >= vx + vw || point[1] < vy || point[1] >= vy + vh) { continue; }
                let mvp = (plane_projection(self, l, l.screen_overlay) * plane_model(l, l.screen_overlay)).as_dmat4();
                let size = plane_size(self, l, l.screen_overlay);
                if mvp.determinant().abs() < 1e-20 {
                    continue;
                }
                let inverse = mvp.inverse();
                let a = inverse * glam::DVec4::new(f64::from(x), f64::from(y), 0.0, 1.0);
                let b = inverse * glam::DVec4::new(f64::from(x), f64::from(y), 1.0, 1.0);
                if a.w.abs() < 1e-12 || b.w.abs() < 1e-12 {
                    continue;
                }
                let a = a.truncate() / a.w;
                let b = b.truncate() / b.w;
                let direction = b - a;
                if direction.z.abs() < 1e-12 {
                    continue;
                }
                let p = a + direction * (-a.z / direction.z);
                let clip = mvp * p.extend(1.0);
                if clip.w <= 0.0 || !clip.is_finite() {
                    continue;
                }
                let depth = clip.z / clip.w;
                let uv = [
                    (p.x / f64::from(size[0]) + 0.5) as f32,
                    (0.5 - p.y / f64::from(size[1])) as f32,
                ];
                if (0.0..=1.0).contains(&depth) && uv.into_iter().all(|v| (0.0..=1.0).contains(&v))
                {
                    hits.push(HitCandidate {
                        id: l.id,
                        uv,
                        depth: depth as f32,
                        order: l.order,
                        three_d: l.three_d,
                    });
                }
            }
            // Construct back-to-front here, reverse once after all 2D boundaries.
            hits[group_start..]
                .sort_unstable_by(|a, b| b.depth.total_cmp(&a.depth).then(a.order.cmp(&b.order)));
            start = end;
        }
        hits.reverse();
        Ok(hits)
    }
    /// Convert a preview drag to translation in the camera's view plane.
    /// Constant clip depth keeps front-facing planes at the same apparent scale.
    /// The UI's currently selected property never participates in this calculation.
    pub fn screen_translation(
        &self,
        point: [f32; 3],
        delta: [f32; 2],
        viewport: [u32; 2],
    ) -> Result<[f32; 3]> {
        ensure(
            viewport.into_iter().all(|v| v > 0)
                && point.into_iter().chain(delta).all(f32::is_finite),
            "invalid preview drag",
        )?;
        let scale =
            (viewport[0] as f64 / self.width as f64).min(viewport[1] as f64 / self.height as f64);
        let vp = self.camera.view_projection.as_dmat4();
        let world = to_world(point, self.width, self.height).as_dvec3();
        let mut clip = vp * world.extend(1.0);
        ensure(
            clip.is_finite() && clip.w > 1.0e-8,
            "drag target is behind the camera",
        )?;
        clip.x += 2.0 * delta[0] as f64 / (self.width as f64 * scale) * clip.w;
        clip.y -= 2.0 * delta[1] as f64 / (self.height as f64 * scale) * clip.w;
        let moved = vp.inverse() * clip;
        ensure(
            moved.is_finite() && moved.w.abs() > 1.0e-8,
            "invalid drag projection",
        )?;
        let offset = moved.truncate() / moved.w - world;
        let result = [offset.x as f32, -offset.y as f32, -offset.z as f32];
        ensure(
            result.into_iter().all(f32::is_finite),
            "drag exceeds numeric range",
        )?;
        Ok(result)
    }
}

pub(crate) fn geometry_offset(layer: &Layer, frame: f64) -> motion_model::Result<Mat4> {
    let anchor = layer.transform.anchor.sample(layer.local_frame(frame))?;
    Ok(Mat4::from_translation(Vec3::new(
        (0.5 - anchor[0]) * layer.size[0],
        (anchor[1] - 0.5) * layer.size[1],
        0.0,
    )))
}
