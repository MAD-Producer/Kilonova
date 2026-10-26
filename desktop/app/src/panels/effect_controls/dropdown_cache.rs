//! Own parameter names on first use; borrow them during warm panel frames.
use motion_ui::widgets::Dropdown;
use std::collections::HashMap;

#[derive(Default)]
pub(super) struct Cache {
    effects: HashMap<u64, HashMap<String, Dropdown>>,
}

impl Cache {
    pub fn with<R>(
        &mut self,
        effect: u64,
        param: &str,
        draw: impl FnOnce(&mut Dropdown) -> R,
    ) -> R {
        let parameters = self.effects.entry(effect).or_default();
        if let Some(dropdown) = parameters.get_mut(param) {
            draw(dropdown)
        } else {
            draw(parameters.entry(param.to_owned()).or_default())
        }
    }

    pub fn close(&mut self) {
        for parameters in self.effects.values_mut() {
            for dropdown in parameters.values_mut() {
                dropdown.open = false;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_owned_and_effect_parameter_states_close_independently() {
        let mut cache = Cache::default();
        let name = String::from("混合模式");
        assert_eq!(
            cache.with(7, &name, |dropdown| {
                assert!(!dropdown.open);
                dropdown.open = true;
                42
            }),
            42
        );
        drop(name);
        cache.with(7, "混合模式", |dropdown| assert!(dropdown.open));
        cache.with(8, "混合模式", |dropdown| {
            assert!(!dropdown.open);
            dropdown.open = true;
        });
        cache.with(7, "通道", |dropdown| assert!(!dropdown.open));
        cache.close();
        for (effect, param) in [(7, "混合模式"), (8, "混合模式"), (7, "通道")] {
            cache.with(effect, param, |dropdown| assert!(!dropdown.open));
        }
    }
}
