use super::*;
use motion_model::{AuthoredCameraSampler, Camera, CameraSample, CameraSampler};

impl ModelSourceCatalog {
    pub fn validate_camera_selections(&self, project: &Project) -> motion_model::Result<()> {
        let validate = |composition: &str,
                        width,
                        height,
                        fps,
                        camera: &Camera,
                        layers: &[motion_model::Layer]|
         -> motion_model::Result<()> {
            for (object, camera) in std::iter::once((0, camera)).chain(
                layers
                    .iter()
                    .filter_map(|l| l.camera_data().map(|camera| (l.id, camera))),
            ) {
                if camera.imported.is_some() {
                    self.sample(camera, 0., fps, [width, height], glam::Mat4::IDENTITY)
                        .map_err(|error| motion_model::Error::Invalid(format!(
                            "camera composition={composition} object={object} phase=selection: {error}")))?;
                }
            }
            Ok(())
        };
        validate(
            &project.composition_id,
            project.width,
            project.height,
            project.fps,
            &project.camera,
            &project.layers,
        )?;
        for child in &project.compositions {
            validate(
                &child.id,
                child.width,
                child.height,
                child.fps,
                &child.camera,
                &child.layers,
            )?;
        }
        Ok(())
    }
}

impl CameraSampler for ModelSourceCatalog {
    fn sample(
        &self,
        camera: &Camera,
        frame: f64,
        fps: u32,
        size: [u32; 2],
        prefix: glam::Mat4,
    ) -> motion_model::Result<CameraSample> {
        let Some(binding) = &camera.imported else {
            return AuthoredCameraSampler.sample(camera, frame, fps, size, prefix);
        };
        binding.validate()?;
        let context = |cause: String| {
            motion_model::Error::Invalid(format!(
                "imported_camera source={} scene={} node={} phase=sample frame={frame}: {cause}",
                binding.asset, binding.scene, binding.node
            ))
        };
        let source = self
            .scene(binding.asset)
            .ok_or_else(|| context("model source not configured".into()))?;
        let animation = binding
            .animation
            .as_ref()
            .map(|animation| {
                animation
                    .sample_seconds(frame, fps)
                    .map(|seconds| motion_scene::AnimationSample {
                        animation: animation.clip as usize,
                        seconds,
                    })
            })
            .transpose()?;
        let world = prefix
            * glam::Mat4::from_translation(
                -glam::Vec3::from_array(binding.pivot) * binding.pixels_per_metre,
            );
        let camera = source
            .camera_view_in_world(
                binding.scene as usize,
                binding.node as usize,
                binding.pixels_per_metre,
                animation,
                size,
                world,
            )
            .map_err(|error| context(error.to_string()))?;
        Ok(CameraSample {
            pose: motion_model::CameraPose {
                eye: camera.eye,
                target: camera.target,
                view_projection: camera.view_projection,
                projection: match camera.kind {
                    motion_scene::CameraKind::Perspective => {
                        motion_model::ProjectionKind::Perspective
                    }
                    motion_scene::CameraKind::Orthographic => {
                        motion_model::ProjectionKind::Orthographic
                    }
                },
            },
            world: camera.world,
            viewport: camera.viewport,
        })
    }
}
