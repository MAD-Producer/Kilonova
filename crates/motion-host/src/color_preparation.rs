//! Transactional CPU preparation of a document's explicit color programs.
//! A trusted OCIO preparer supplies programs; this does not launch an implicit
//! runtime, install a scene, or remove the managed-rendering refusal boundary.
use motion_core::{color_management::*, Project};
use motion_effects::{
    color_config::{ColorSelection, VerifiedColorConfig},
    color_processor::{
        CodeValueRange, ColorProcessorError, ColorProcessorPhase, ColorProcessorSource,
        CompiledColorImageProcessor,
    },
    image_contract::ImageMeaning,
    AlphaMode,
};
use std::{
    collections::BTreeMap,
    path::Path,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Instant,
};

mod images;
pub use images::{PreparedSourceImage, SourceImageRequest};

pub const MAX_PROGRAMS: usize = 256;
pub const MAX_PROGRAM_PAYLOAD_BYTES: usize = 64 * 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target {
    DefaultInput,
    AuthoredInput,
    Working,
    Source(u64),
    Display,
    Output,
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("color preparation request is already running")]
    Busy,
    #[error("color preparation asset={asset}: {reason}")]
    SourceBinding { asset: u64, reason: &'static str },
    #[error("color preparation asset={asset} phase={phase}")]
    SourceImage {
        asset: u64,
        phase: &'static str,
        #[source]
        source: Box<crate::Error>,
    },
    #[error("color preparation phase=project")]
    Project(#[source] motion_core::Error),
    #[error("color preparation phase=resources")]
    Resources(#[source] motion_core::Error),
    #[error("color preparation target={target:?} black={black} white={white}: range cannot be represented for execution")]
    Range {
        target: Target,
        black: f64,
        white: f64,
        #[source]
        source: ColorProcessorError,
    },
    #[error("color preparation target={target:?} phase={phase:?}: provider failed")]
    Provider {
        target: Target,
        phase: ColorProcessorPhase,
        #[source]
        source: Box<crate::Error>,
    },
    #[error("color preparation target={target:?} phase={phase:?}: {reason}")]
    Binding {
        target: Target,
        phase: ColorProcessorPhase,
        reason: &'static str,
    },
    #[error("color preparation target={target:?} phase={phase:?}: compilation failed")]
    Compile {
        target: Target,
        phase: ColorProcessorPhase,
        #[source]
        source: ColorProcessorError,
    },
    #[error("color preparation field={field}: requested={requested} limit={limit}")]
    Capacity {
        field: &'static str,
        requested: usize,
        limit: usize,
    },
    #[error("color preparation phase=program_key")]
    Json(#[source] serde_json::Error),
}

/// Readable independently while a caller-owned preparation worker is blocked.
/// The request ID is immutable; this observation owns no project/session lock.
#[derive(Clone)]
pub struct Observation {
    request: u64,
    running: Arc<AtomicBool>,
}
impl Observation {
    pub fn new(request: u64) -> Self {
        Self {
            request,
            running: Arc::new(AtomicBool::new(false)),
        }
    }
    pub fn request(&self) -> u64 {
        self.request
    }
    pub fn running(&self) -> bool {
        self.running.load(Ordering::Acquire)
    }
}
struct Flight<'a> {
    observation: &'a Observation,
    operation: &'static str,
    started: Instant,
    finished: bool,
}
impl Drop for Flight<'_> {
    fn drop(&mut self) {
        self.observation.running.store(false, Ordering::Release);
        if !self.finished {
            log::error!(
                "color preparation request={} operation={} phase=abandoned elapsed_ms={}",
                self.observation.request,
                self.operation,
                self.started.elapsed().as_millis()
            );
        }
    }
}

/// The provider receives exact verified bytes and a complete selection for each
/// unique program. Err is retained unchanged; None/fallback/retry is not a reply.
pub struct Request<'a> {
    pub config: &'a VerifiedColorConfig,
    pub selection: &'a ColorSelection,
    pub target: Target,
    pub phase: ColorProcessorPhase,
    pub input_alpha: AlphaMode,
    pub output_alpha: AlphaMode,
}

/// Prepare with the explicitly bundled native runtime. Loading happens inside
/// the existing observed provider operation, after its Info start and in-flight
/// publication. The real native error remains in the host source chain.
/// Scene installation still requires the full managed rendering contract.
pub fn prepare_native_project_colors(
    project: &Project,
    root: &Path,
    runtime_manifest: &Path,
    observation: &Observation,
) -> Result<Option<PreparedProjectColors>, Error> {
    let mut runtime = None;
    prepare_project_colors(
        project,
        root,
        |request| {
            if runtime.is_none() {
                runtime = Some(motion_effects::native_color::NativeColorRuntime::load(
                    runtime_manifest,
                )?);
            }
            let native = runtime.as_ref().ok_or_else(|| {
                crate::Error::from(motion_effects::native_color::Error::Admission(
                    "native runtime was not retained",
                ))
            })?;
            let program = native.prepare(
                request.config,
                request.selection,
                request.phase,
                request.input_alpha,
                request.output_alpha,
            )?;
            Ok(program.source().clone())
        },
        observation,
    )
}

#[derive(Clone)]
pub enum PreparedInput {
    Color(Arc<CompiledColorImageProcessor>),
    Data(ImageMeaning),
}

/// All programs are prepared or no result is published. This object is not a
/// rendering authorization: Session/Scene still require their full managed path.
pub struct PreparedProjectColors {
    settings: Arc<ProjectColorManagement>,
    config: VerifiedColorConfig,
    default_input: Arc<CompiledColorImageProcessor>,
    authored_input: Arc<CompiledColorImageProcessor>,
    sources: BTreeMap<u64, PreparedInput>,
    display: Arc<CompiledColorImageProcessor>,
    output: Arc<CompiledColorImageProcessor>,
    program_count: usize,
    payload_bytes: usize,
}
impl PreparedProjectColors {
    pub fn settings(&self) -> &ProjectColorManagement {
        &self.settings
    }
    pub fn config(&self) -> &VerifiedColorConfig {
        &self.config
    }
    pub fn default_input(&self) -> &Arc<CompiledColorImageProcessor> {
        &self.default_input
    }
    pub fn authored_input(&self) -> &Arc<CompiledColorImageProcessor> {
        &self.authored_input
    }
    pub fn source(&self, asset: u64) -> Option<&PreparedInput> {
        self.sources.get(&asset)
    }
    pub fn display(&self) -> &Arc<CompiledColorImageProcessor> {
        &self.display
    }
    pub fn output(&self) -> &Arc<CompiledColorImageProcessor> {
        &self.output
    }
    pub fn program_count(&self) -> usize {
        self.program_count
    }
    /// Bounded compiled shader/table payload, not a measurement of allocator or GPU memory.
    pub fn payload_bytes(&self) -> usize {
        self.payload_bytes
    }
}

struct Programs<'a, F> {
    config: &'a VerifiedColorConfig,
    provider: &'a mut F,
    observation: &'a Observation,
    programs: BTreeMap<Vec<u8>, Arc<CompiledColorImageProcessor>>,
    payload_bytes: usize,
}
impl<F: FnMut(Request<'_>) -> crate::Result<ColorProcessorSource>> Programs<'_, F> {
    fn get(
        &mut self,
        selection: &ColorSelection,
        target: Target,
        phase: ColorProcessorPhase,
        input_alpha: AlphaMode,
        output_alpha: AlphaMode,
        input_range: CodeValueRange,
        output_range: CodeValueRange,
    ) -> Result<Arc<CompiledColorImageProcessor>, Error> {
        let key = serde_json::to_vec(&(
            selection,
            phase,
            input_alpha,
            output_alpha,
            input_range,
            output_range,
        ))
        .map_err(Error::Json)?;
        if let Some(program) = self.programs.get(&key) {
            return Ok(program.clone());
        }
        if self.programs.len() == MAX_PROGRAMS {
            return Err(Error::Capacity {
                field: "programs",
                requested: MAX_PROGRAMS + 1,
                limit: MAX_PROGRAMS,
            });
        }
        // Publish before entering an arbitrary provider or a native compiler.
        log::info!("color preparation request={} operation=prepare_project_colors target={target:?} phase=provider transform={phase:?}", self.observation.request);
        let source = (self.provider)(Request {
            config: self.config,
            selection,
            target,
            phase,
            input_alpha,
            output_alpha,
        })
        .map_err(|source| Error::Provider {
            target,
            phase,
            source: Box::new(source),
        })?;
        if source.phase != phase
            || source.input_alpha != input_alpha
            || source.output_alpha != output_alpha
        {
            return Err(Error::Binding {
                target,
                phase,
                reason: "provider phase/alpha differs from the request",
            });
        }
        log::info!("color preparation request={} operation=prepare_project_colors target={target:?} phase=compile transform={phase:?}", self.observation.request);
        let program = source
            .compile_image_with_ranges(self.config, selection, input_range, output_range)
            .map_err(|source| Error::Compile {
                target,
                phase,
                source,
            })?;
        let requested = self
            .payload_bytes
            .checked_add(program.payload_bytes())
            .unwrap_or(usize::MAX);
        if requested > MAX_PROGRAM_PAYLOAD_BYTES {
            return Err(Error::Capacity {
                field: "compiled program payload bytes",
                requested,
                limit: MAX_PROGRAM_PAYLOAD_BYTES,
            });
        }
        self.payload_bytes = requested;
        let program = Arc::new(program);
        self.programs.insert(key, program.clone());
        Ok(program)
    }
}

fn range(signal: &ColorSignal, target: Target) -> Result<CodeValueRange, Error> {
    let (black, white) = match signal.range {
        SignalRange::Full {} => return Ok(CodeValueRange::FULL),
        SignalRange::Limited { black, white } => (black, white),
    };
    CodeValueRange::new(black, white).map_err(|source| Error::Range {
        target,
        black,
        white,
        source,
    })
}

fn prepare<F: FnMut(Request<'_>) -> crate::Result<ColorProcessorSource>>(
    project: &Project,
    root: &Path,
    provider: &mut F,
    observation: &Observation,
) -> Result<Option<PreparedProjectColors>, Error> {
    project.validate().map_err(Error::Project)?;
    let ProjectColorManagement::Ocio { settings } = project.color_management.as_ref() else {
        return Ok(None);
    };
    // Refuse unrepresentable intervals before calling any provider. Valid code
    // ranges are executed around the OCIO transform, never relabelled as full.
    for (signal, target) in [
        (&settings.default_input, Target::DefaultInput),
        (&settings.authored_input.signal, Target::AuthoredInput),
        (&settings.working, Target::Working),
        (&settings.display, Target::Display),
        (&settings.output, Target::Output),
    ] {
        range(signal, target)?;
    }
    for source in &settings.sources {
        if let SourceInterpretation::Color { input } = &source.interpretation {
            range(&input.signal, Target::Source(source.asset))?;
        }
    }
    log::info!(
        "color preparation request={} operation=prepare_project_colors phase=read_config",
        observation.request
    );
    let config =
        motion_core::storage::read_color_config(root, settings).map_err(Error::Resources)?;
    let mut programs = Programs {
        config: &config,
        provider,
        observation,
        programs: BTreeMap::new(),
        payload_bytes: 0,
    };
    let working_alpha = settings.working.alpha;
    let working_range = range(&settings.working, Target::Working)?;
    let default_input = programs.get(
        &settings.selection,
        Target::DefaultInput,
        ColorProcessorPhase::InputToWorking,
        settings.default_input.alpha,
        working_alpha,
        range(&settings.default_input, Target::DefaultInput)?,
        working_range,
    )?;
    let mut selection = settings.selection.clone();
    selection.input = settings.authored_input.space.clone();
    let authored_input = programs.get(
        &selection,
        Target::AuthoredInput,
        ColorProcessorPhase::InputToWorking,
        settings.authored_input.signal.alpha,
        working_alpha,
        range(&settings.authored_input.signal, Target::AuthoredInput)?,
        working_range,
    )?;
    let assignments: BTreeMap<_, _> = settings
        .sources
        .iter()
        .map(|s| (s.asset, &s.interpretation))
        .collect();
    let mut sources = BTreeMap::new();
    for asset in project
        .assets
        .iter()
        .map(|a| a.id)
        .chain(project.video_assets.iter().map(|a| a.id))
    {
        let input = match assignments.get(&asset) {
            Some(SourceInterpretation::Data { meaning }) => PreparedInput::Data(*meaning),
            Some(SourceInterpretation::Color { input }) => {
                let mut selection = settings.selection.clone();
                selection.input = input.space.clone();
                PreparedInput::Color(programs.get(
                    &selection,
                    Target::Source(asset),
                    ColorProcessorPhase::InputToWorking,
                    input.signal.alpha,
                    working_alpha,
                    range(&input.signal, Target::Source(asset))?,
                    working_range,
                )?)
            }
            None => PreparedInput::Color(default_input.clone()),
        };
        sources.insert(asset, input);
    }
    let display = programs.get(
        &settings.selection,
        Target::Display,
        ColorProcessorPhase::WorkingToDisplay,
        working_alpha,
        settings.display.alpha,
        working_range,
        range(&settings.display, Target::Display)?,
    )?;
    let output = programs.get(
        &settings.selection,
        Target::Output,
        ColorProcessorPhase::WorkingToOutput,
        working_alpha,
        settings.output.alpha,
        working_range,
        range(&settings.output, Target::Output)?,
    )?;
    let (program_count, payload_bytes) = (programs.programs.len(), programs.payload_bytes);
    drop(programs);
    Ok(Some(PreparedProjectColors {
        settings: project.color_management.clone(),
        config,
        default_input,
        authored_input,
        sources,
        display,
        output,
        program_count,
        payload_bytes,
    }))
}

/// Run in a caller-owned non-Present preparation operation. No thread, restart,
/// retry, disk search or environment-selected OCIO runtime is introduced here.
pub fn prepare_project_colors<F: FnMut(Request<'_>) -> crate::Result<ColorProcessorSource>>(
    project: &Project,
    root: &Path,
    mut provider: F,
    observation: &Observation,
) -> Result<Option<PreparedProjectColors>, Error> {
    if observation
        .running
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .is_err()
    {
        return Err(Error::Busy);
    }
    let mut flight = Flight {
        observation,
        operation: "prepare_project_colors",
        started: Instant::now(),
        finished: false,
    };
    log::info!(
        "color preparation request={} operation=prepare_project_colors phase=start",
        observation.request
    );
    let result = prepare(project, root, &mut provider, observation);
    match &result {
        Ok(_) => log::debug!("color preparation request={} operation=prepare_project_colors phase=done elapsed_ms={}", observation.request, flight.started.elapsed().as_millis()),
        Err(error) => log::error!("color preparation request={} operation=prepare_project_colors phase=failed elapsed_ms={} cause={}",
            observation.request, flight.started.elapsed().as_millis(), brief(error)),
    }
    flight.finished = true;
    result
}

fn brief(error: &Error) -> String {
    use std::fmt::Write;
    struct Bounded(String);
    impl std::fmt::Write for Bounded {
        fn write_str(&mut self, value: &str) -> std::fmt::Result {
            let remaining = 1024usize.saturating_sub(self.0.len());
            let end = value
                .char_indices()
                .map(|(i, _)| i)
                .chain(std::iter::once(value.len()))
                .take_while(|i| *i <= remaining)
                .last()
                .unwrap_or(0);
            self.0.push_str(&value[..end]);
            if end != value.len() {
                Err(std::fmt::Error)
            } else {
                Ok(())
            }
        }
    }
    let mut out = Bounded(String::new());
    let mut cause: Option<&dyn std::error::Error> = Some(error);
    for index in 0..8 {
        let Some(current) = cause else {
            break;
        };
        if write!(&mut out, "{}{current}", if index == 0 { "" } else { ": " }).is_err() {
            break;
        }
        cause = current.source();
    }
    out.0
}

#[cfg(test)]
mod tests;
