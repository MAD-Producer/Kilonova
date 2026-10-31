//! Receiver-time evaluation for a persistent, caller-driven exposure task.
//! A receiver's dependencies use its time; the dependency's own blur setting
//! does not replace that time. Local raster capture and phase compositing are
//! separate GPU operations, so a complete ordinary Scene is never accumulated
//! here as a substitute for independently sampled layers.
use crate::{DrawLayer, Scene};
use motion_model::{
    composition, render_sampling::ExposureClock, Content, FrameEvaluator, Observer, Project, Result,
};
use std::sync::Arc;

/// Validate once per immutable document revision, outside per-frame evaluation.
/// Clones share the same snapshot, including for nested exposure tasks.
#[derive(Clone)]
pub struct ExposureDocument(Arc<Project>, crate::ModelSourceCatalog);
impl ExposureDocument {
    pub fn new(project: Arc<Project>) -> Result<Self> {
        project.validate()?;
        Ok(Self(project, Default::default()))
    }
    pub fn source(&self) -> &Arc<Project> {
        &self.0
    }
}
impl std::ops::Deref for ExposureDocument {
    type Target = Project;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[derive(Clone, Copy, Debug)]
struct Receiver {
    object: u64,
    frame: f64,
}

/// The full context is borrowed until the caller finishes this receiver. A
/// pending media request may poll `current` again without repeating evaluation.
pub struct ReceiverScene<'a> {
    pub object: u64,
    pub frame: f64,
    /// None outside the composition; a missing layer is also transparent.
    pub scene: Option<&'a Scene>,
}
impl ReceiverScene<'_> {
    pub fn layer(&self) -> Option<&DrawLayer> {
        self.scene?.layers.iter().find(|l| l.id == self.object)
    }
}

pub enum ExposureSceneStep<'a> {
    Receiver(ReceiverScene<'a>),
    /// Remains ready until the caller successfully composites and accumulates
    /// the whole phase. Receiving this event does not advance any GPU result.
    PhaseReady {
        index: usize,
        weight: f64,
    },
    Complete,
}

pub struct ExposureScenes<E> {
    document: ExposureDocument,
    /// A sampling view only; never validate or save it as a standalone project.
    root: Option<Project>,
    evaluator: Arc<E>,
    observer: Option<Observer>,
    clock: ExposureClock,
    objects: Vec<u64>,
    schedule: Vec<Receiver>,
    phase: usize,
    receiver: usize,
    prepared: bool,
    failed: bool,
    scene: Scene,
    scene_frame: Option<f64>,
    center: f64,
    center_scene: Option<Box<Scene>>,
    preview: bool,
    scope: ExposureScope,
}

#[derive(Clone)]
pub(crate) struct ExposureScope {
    registry: Arc<crate::scene::instance_ids::InstanceIds>,
    path: Vec<u64>,
    source_time: motion_model::layer_time::SourceTimeSample,
}
impl ExposureScope {
    pub(crate) fn child(&self, reference: u64, source_time: motion_model::layer_time::SourceTimeSample) -> Self {
        let mut path = self.path.clone();
        path.push(reference);
        Self {
            registry: self.registry.clone(),
            path,
            source_time,
        }
    }
}
impl<E: FrameEvaluator> ExposureScenes<E> {
    /// The caller retains one immutable document snapshot for the task. A new
    /// project/view/request must cancel this task rather than mutate its inputs.
    pub fn new(
        document: ExposureDocument,
        composition: &str,
        frame: f64,
        preview: bool,
        observer: Option<Observer>,
        evaluator: E,
    ) -> Result<Self> {
        Self::new_shared(
            document,
            composition,
            frame,
            preview,
            observer,
            Arc::new(evaluator),
            None,
        )
    }
    pub(crate) fn new_shared(
        document: ExposureDocument,
        composition: &str,
        frame: f64,
        preview: bool,
        observer: Option<Observer>,
        evaluator: Arc<E>,
        scope: Option<ExposureScope>,
    ) -> Result<Self> {
        let root = if composition == document.composition_id {
            None
        } else {
            Some(document.composition_frame_view(composition)?)
        };
        let project = root.as_ref().unwrap_or(&document);
        let scope = match scope {
            Some(scope) => scope,
            None => {
                let mut registry = crate::scene::instance_ids::InstanceIds::default();
                registry.prepare_composition(&document, project)?;
                ExposureScope {
                    registry: Arc::new(registry),
                    path: vec![],
                    source_time: motion_model::layer_time::SourceTimeSample {
                        frame, rate: 1., traversal: motion_model::layer_time::TimeTraversal::Later,
                    },
                }
            }
        };
        let mapping = if scope.path.is_empty() {
            None
        } else {
            Some(
                scope.registry.mapping(
                    &scope.path,
                    &project.composition_id,
                    &project
                        .layers
                        .iter()
                        .map(|layer| layer.id)
                        .collect::<Vec<_>>(),
                )?,
            )
        };
        let alias = |object| -> Result<u64> {
            match &mapping {
                Some(mapping) => {
                    crate::scene::instance_ids::alias(mapping, &project.composition_id, object)
                }
                None => Ok(object),
            }
        };
        let cursor = motion_model::layer_time::SourceTimeSample { frame, ..scope.source_time };
        if !frame.is_finite() || !cursor.in_interval(0., f64::from(project.frames)) {
            return composition::fail(
                &project.composition_id,
                "sample_failed",
                "Invalid exposure composition time",
                serde_json::json!({
                    "phase":"exposure_scene_init", "sample_frame":frame.to_string(), "frames":project.frames}),
            );
        }
        cursor.validate()?;
        let objects = project
            .layers
            .iter()
            .filter(|l| {
                !matches!(
                    l.content,
                    Content::Null | Content::Camera { .. } | Content::Audio { .. }
                )
            })
            .map(|l| alias(l.id))
            .collect::<Result<Vec<_>>>()?;
        let shutters: Vec<_> = project
            .layers
            .iter()
            .filter_map(|l| {
                l.sampling
                    .motion_blur
                    .map(|s| alias(l.id).map(|id| (id, s)))
            })
            .collect::<Result<Vec<_>>>()?;
        let clock = ExposureClock::new(frame, &shutters, preview)?;
        let mut scene = Scene::new(project)?;
        scene.set_model_source_catalog(document.1.clone());
        let mut task = Self {
            document,
            root,
            evaluator,
            observer,
            clock,
            objects,
            schedule: vec![],
            phase: 0,
            receiver: 0,
            prepared: false,
            failed: false,
            scene,
            scene_frame: None,
            center: frame,
            center_scene: None,
            preview,
            scope,
        };
        task.schedule_phase()?;
        Ok(task)
    }
    pub fn document(&self) -> &Arc<Project> {
        self.document.source()
    }
    /// Freeze source ownership before the first receiver/phase is evaluated.
    pub fn set_model_source_catalog(&mut self, catalog: crate::ModelSourceCatalog) -> Result<()> {
        if self.document.1.shares_sources_with(&catalog) { return Ok(()); }
        if !self.at_start() { return self.protocol("model catalog cannot change after exposure sampling starts"); }
        self.document.1 = catalog.clone();
        self.scene.set_model_source_catalog(catalog);
        self.scene_frame = None;
        self.center_scene = None;
        Ok(())
    }
    pub(crate) fn child_factory(&self) -> (ExposureDocument, Arc<E>, ExposureScope) {
        (
            self.document.clone(),
            self.evaluator.clone(),
            self.scope.clone(),
        )
    }
    pub fn project(&self) -> &Project {
        self.root.as_ref().unwrap_or(&self.document)
    }
    pub fn phase(&self) -> usize {
        self.phase
    }
    pub fn phase_count(&self) -> usize {
        self.clock.phases().len()
    }

    pub fn preview(&self) -> bool {
        self.preview
    }
    pub(crate) fn at_start(&self) -> bool {
        self.phase == 0 && self.receiver == 0 && !self.failed
    }
    pub fn phase_weights(&self) -> impl Iterator<Item = f64> + '_ {
        self.clock.phases().iter().map(|phase| phase.weight)
    }
    pub fn receiver(&self) -> Option<(u64, f64)> {
        (self.phase < self.phase_count())
            .then(|| self.schedule.get(self.receiver))
            .flatten()
            .map(|r| (r.object, r.frame))
    }
    /// Evaluate the immutable center without changing receiver/phase progress.
    /// Repeated calls reuse the same context, including across media polls.
    pub fn center_scene(&mut self) -> Result<&Scene> {
        if self.failed {
            return self.protocol("Exposure evaluation has failed; discard this task");
        }
        self.prepare_context(self.center, None)?;
        match self.center_scene.as_deref() {
            Some(scene) => Ok(scene),
            None => self.protocol("Exposure center context is missing"),
        }
    }

    fn schedule_phase(&mut self) -> Result<()> {
        self.schedule.clear();
        for &object in &self.objects {
            self.schedule.push(Receiver {
                object,
                frame: self.clock.layer_frame(object, self.phase)?,
            });
        }
        // Group equal times to reuse one complete context. This is evaluation
        // order only; final compositing must use the DrawLayer's authored order.
        self.schedule.sort_by(|a, b| a.frame.total_cmp(&b.frame));
        Ok(())
    }
    fn protocol<T>(&self, message: &str) -> Result<T> {
        composition::fail(
            &self.project().composition_id,
            "sample_failed",
            message,
            serde_json::json!({"phase":"exposure_scene","exposure_phase":self.phase,
                "receiver":self.schedule.get(self.receiver).map(|r|r.object)}),
        )
    }
    fn prepare_context(&mut self, frame: f64, receiver: Option<u64>) -> Result<()> {
        let centered = frame.to_bits() == self.center.to_bits();
        let cached = if centered {
            self.center_scene.is_some()
        } else {
            self.scene_frame
                .is_some_and(|cached| cached.to_bits() == frame.to_bits())
        };
        if !cached {
            let root = self.root.as_ref().unwrap_or(&self.document);
            let scene = if centered {
                if self.center_scene.is_none() {
                    let mut scene = match Scene::new(root) {
                        Ok(scene) => scene,
                        Err(error) => { self.failed = true; return Err(error); }
                    };
                    scene.set_model_source_catalog(self.document.1.clone());
                    self.center_scene = Some(Box::new(scene));
                }
                self.center_scene.as_deref_mut().ok_or_else(||
                    motion_model::Error::Invalid("exposure center scene missing after construction".into()))?
            } else {
                &mut self.scene
            };
            if let Err(mut error) = scene.sample_exposure_instance(
                &self.document,
                root,
                motion_model::layer_time::SourceTimeSample { frame, ..self.scope.source_time },
                self.observer.as_ref(),
                self.evaluator.as_ref(),
                &self.scope.registry,
                &self.scope.path,
            ) {
                self.failed = true;
                let context = serde_json::json!({"phase":"exposure_scene","exposure_phase":self.phase,
                "receiver":receiver,"sample_frame":frame});
                if let motion_model::Error::Composition(detail) = &mut error {
                    // Preserve the primary code, message and source phase.
                    // Providers can supply non-object details or an inner
                    // exposure context; never panic or overwrite either.
                    if !detail.details.is_object() || detail.details.get("exposure").is_some() {
                        let cause = std::mem::take(&mut detail.details);
                        detail.details =
                            serde_json::json!({"exposure":context,"source_details":cause});
                    } else {
                        detail.details["exposure"] = context;
                    }
                    return Err(error);
                }
                return composition::fail(
                    &root.composition_id,
                    "sample_failed",
                    &error.to_string(),
                    context,
                );
            }
            if !centered {
                self.scene_frame = Some(frame);
            }
        }
        Ok(())
    }
    pub fn current(&mut self) -> Result<ExposureSceneStep<'_>> {
        if self.failed {
            return self.protocol("Exposure evaluation has failed; discard this task");
        }
        if self.phase == self.phase_count() {
            return Ok(ExposureSceneStep::Complete);
        }
        let Some(&receiver) = self.schedule.get(self.receiver) else {
            return Ok(ExposureSceneStep::PhaseReady {
                index: self.phase,
                weight: self.clock.phases()[self.phase].weight,
            });
        };
        let in_range = motion_model::layer_time::SourceTimeSample {
            frame: receiver.frame, ..self.scope.source_time
        }.in_interval(0., f64::from(self.project().frames));
        if !self.prepared {
            if in_range {
                self.prepare_context(receiver.frame, Some(receiver.object))?;
            }
            self.prepared = true;
        }
        Ok(ExposureSceneStep::Receiver(ReceiverScene {
            object: receiver.object,
            frame: receiver.frame,
            scene: if in_range {
                if receiver.frame.to_bits() == self.center.to_bits() {
                    self.center_scene.as_deref()
                } else {
                    Some(&self.scene)
                }
            } else {
                None
            },
        }))
    }
    /// Call only after this receiver's local raster has been retained, or after
    /// its transparent result was handled. Media pending never calls this.
    pub fn finish_receiver(&mut self) -> Result<()> {
        if self.failed || !self.prepared || self.receiver >= self.schedule.len() {
            return self.protocol("Exposure receiver is not ready to finish");
        }
        self.receiver += 1;
        self.prepared = false;
        Ok(())
    }
    /// Call only after the completed phase was successfully accumulated. The
    /// sampler never retries an errored evaluator or advances on decoder polls.
    pub fn finish_phase(&mut self) -> Result<()> {
        if self.failed || self.phase >= self.phase_count() || self.receiver != self.schedule.len() {
            return self.protocol("Exposure phase is not ready to finish");
        }
        self.phase += 1;
        self.receiver = 0;
        if self.phase < self.phase_count() {
            self.schedule_phase()?;
        }
        Ok(())
    }
}
