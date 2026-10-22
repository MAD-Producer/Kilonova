//! Parsed tracks live for the caller's root/composition/revision snapshot.
use motion_core::EffectParam;
use std::collections::HashMap;

#[derive(Default)]
pub struct TrackCache {
    tracks: HashMap<(u64, u64), HashMap<String, EffectParam>>,
}
impl TrackCache {
    pub fn clear(&mut self) {
        self.tracks.clear();
    }

    pub fn get_or_try_insert<E>(
        &mut self,
        object: u64,
        effect: u64,
        param: &str,
        load: impl FnOnce() -> Result<EffectParam, E>,
    ) -> Result<&EffectParam, E> {
        // Instance IDs are Copy; the inner String key supports borrowed &str
        // lookup. Own the parameter name only after a successful cache miss.
        let parameters = self.tracks.entry((object, effect)).or_default();
        if !parameters.contains_key(param) {
            let parameter = load()?;
            parameters.insert(param.to_owned(), parameter);
        }
        Ok(&parameters[param])
    }
}
