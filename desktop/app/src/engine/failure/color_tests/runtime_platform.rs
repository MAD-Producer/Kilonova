//! Real desktop capabilities with an explicit test-owned runtime location.
//! This avoids depending on whether the Cargo test executable has a release
//! bundle beside it. No production environment override is introduced.
use motion_host::{Platform, SurfaceTarget, VideoDecoder, VideoQuery};
use motion_media::Result as MediaResult;
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

pub(in crate::engine::failure) struct RuntimePlatform {
    pub manifest: PathBuf,
    pub before_manifest: Option<Arc<dyn Fn() + Send + Sync>>,
    pub desktop: Arc<motion_desktop_media::DesktopPlatform>,
}
impl Platform for RuntimePlatform {
    fn name(&self) -> &'static str {
        self.desktop.name()
    }
    fn color_runtime_manifest(&self) -> motion_host::Result<Option<PathBuf>> {
        if let Some(before) = &self.before_manifest {
            before();
        }
        Ok(Some(self.manifest.clone()))
    }
    fn probe_video(
        &self,
        path: &Path,
        selected: Option<u32>,
        audio: Option<u32>,
        check: &dyn Fn() -> MediaResult<()>,
    ) -> MediaResult<motion_media::VideoProbe> {
        self.desktop.probe_video(path, selected, audio, check)
    }
    fn open_decoder(
        &self,
        path: &Path,
        asset: motion_core::VideoAsset,
        pts: Vec<u64>,
    ) -> MediaResult<Box<dyn VideoDecoder>> {
        self.desktop.open_decoder(path, asset, pts)
    }
    fn decode_audio(
        &self,
        path: &Path,
        pcm: &Path,
        selected: Option<u32>,
        limit: u64,
        check: &mut dyn FnMut(Option<f64>) -> MediaResult<()>,
    ) -> MediaResult<motion_core::AudioAsset> {
        self.desktop.decode_audio(path, pcm, selected, limit, check)
    }
    fn media_capabilities(&self, query: Option<&VideoQuery>) -> MediaResult<serde_json::Value> {
        self.desktop.media_capabilities(query)
    }
    fn attach_surface(
        &self,
        window: Arc<dyn wgpu::DisplayAndWindowHandle + Send + Sync>,
        width: u32,
        height: u32,
        budget: motion_render::AllocationBudget,
    ) -> motion_host::Result<SurfaceTarget> {
        self.desktop.attach_surface(window, width, height, budget)
    }
}
