//! Cold, bounded preparation of the project's alpha-less output endpoint.
use super::*;
use motion_effects::native_color::NativeColorRuntime;

pub fn prepare_native_opaque_output(
    colors: &PreparedProjectColors,
    runtime_manifest: &Path,
    observation: &Observation,
) -> Result<CompiledColorImageProcessor, Error> {
    observed_operation(observation, "prepare_opaque_color_output", || {
        let ProjectColorManagement::Ocio { settings } = colors.settings() else {
            return Err(Error::Binding {
                target: Target::Output,
                phase: ColorProcessorPhase::WorkingToOutput,
                reason: "managed output is unavailable",
            });
        };
        let phase = ColorProcessorPhase::WorkingToOutput;
        if settings.working.transfer != ColorTransfer::Linear
            || settings.working.range != (SignalRange::Full {})
            || settings.working.alpha != AlphaMode::Premultiplied
            || settings.output.range != (SignalRange::Full {})
        {
            return Err(Error::Binding { target: Target::Output, phase,
                reason: "opaque output requires linear premultiplied full-range working storage and full-range output" });
        }
        let count = colors.program_count().saturating_add(1);
        if count > MAX_PROGRAMS {
            return Err(Error::Capacity {
                field: "opaque output program count",
                requested: count,
                limit: MAX_PROGRAMS,
            });
        }
        let source = (|| -> crate::Result<_> {
            let runtime = NativeColorRuntime::load(runtime_manifest)?;
            let program = runtime.prepare(
                colors.config(),
                &settings.selection,
                phase,
                settings.working.alpha,
                settings.output.alpha,
            )?;
            Ok(program.source().clone())
        })()
        .map_err(|source| Error::Provider {
            target: Target::Output,
            phase,
            source: Box::new(source),
        })?;
        let shader_sha256 = format!("{:x}", Sha256::digest(source.shader.as_bytes()));
        let shader_bytes = source.shader.len();
        color_log!(Info, "color preparation request={} operation=prepare_opaque_color_output target=Output phase=compile transform={phase:?} config_key={:02x?} selection_key={:02x?} shader_sha256={shader_sha256} shader_bytes={shader_bytes}", observation.request, colors.config().key(), source.selection_key);
        let program = source
            .compile_output_over_black(colors.config(), &settings.selection)
            .map_err(|source| Error::Compile {
                target: Target::Output,
                phase,
                shader_sha256,
                shader_bytes,
                source,
            })?;
        let requested = colors
            .payload_bytes()
            .saturating_add(program.payload_bytes());
        if requested > MAX_PROGRAM_PAYLOAD_BYTES {
            return Err(Error::Capacity {
                field: "opaque output compiled payload bytes",
                requested,
                limit: MAX_PROGRAM_PAYLOAD_BYTES,
            });
        }
        Ok(program)
    })
}
