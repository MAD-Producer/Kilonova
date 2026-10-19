//! Stored literals are fallbacks, never substitutes for a connected input.
use super::*;

pub(super) fn value_text(value: &InputValue) -> String {
    match value {
        InputValue::Bool(value) => value.to_string(),
        _ => value
            .components()
            .iter()
            .map(|v| v.to_string())
            .collect::<Vec<_>>()
            .join(", "),
    }
}

impl NodeEditor {
    pub(super) fn input_default(&self, id: &str, port: &str) -> Result<&InputValue, Diagnostic> {
        self.document
            .nodes
            .iter()
            .find(|node| node.id == id)
            .and_then(|node| node.input_defaults.get(port))
            .ok_or_else(|| {
                diag(
                    ErrorCode::UnknownPort,
                    Some(id),
                    Some(port),
                    "Input has no stored default",
                )
            })
    }

    pub(super) fn replace_input_default(
        &mut self,
        id: &str,
        port: &str,
        value: InputValue,
    ) -> Result<(), Diagnostic> {
        // The card is read-only while wired, including stale edits after a reconnect.
        if self
            .document
            .connections
            .iter()
            .any(|edge| edge.to == PortRef::new(id, port))
        {
            return Err(diag(
                ErrorCode::InvalidValue,
                Some(id),
                Some(port),
                "Disconnect the input before editing its default",
            ));
        }
        self.input_default(id, port)?;
        let mut candidate = self
            .document
            .nodes
            .iter()
            .find(|node| node.id == id)
            .unwrap()
            .clone();
        candidate.input_defaults.insert(port.into(), value);
        self.check_node(&candidate)?;
        *self
            .document
            .nodes
            .iter_mut()
            .find(|node| node.id == id)
            .unwrap() = candidate;
        Ok(())
    }

    pub fn set_input_default(
        &mut self,
        id: &str,
        port: &str,
        value: InputValue,
    ) -> Result<(), Diagnostic> {
        if self.input_default(id, port)? == &value {
            return Ok(());
        }
        let before = self.state(true);
        self.replace_input_default(id, port, value)?;
        self.finish_edit(before);
        Ok(())
    }

    pub(super) fn set_input_text(
        &mut self,
        id: &str,
        port: &str,
        text: &str,
    ) -> Result<(), Diagnostic> {
        let mut value = self.input_default(id, port)?.clone();
        if let InputValue::Bool(slot) = &mut value {
            *slot = match text.trim() {
                "true" | "1" => true,
                "false" | "0" => false,
                _ => {
                    return Err(diag(
                        ErrorCode::InvalidValue,
                        Some(id),
                        Some(port),
                        "Enter true or false",
                    ))
                }
            };
        } else {
            let values = text
                .split(',')
                .map(|part| part.trim().parse::<f32>())
                .collect::<Result<Vec<_>, _>>()
                .map_err(|_| {
                    diag(
                        ErrorCode::InvalidValue,
                        Some(id),
                        Some(port),
                        "Enter comma-separated finite components",
                    )
                })?;
            if values.len() != value.components().len() {
                return Err(diag(
                    ErrorCode::InvalidValue,
                    Some(id),
                    Some(port),
                    "Enter one number per component, separated by commas",
                ));
            }
            for (index, component) in values.into_iter().enumerate() {
                value.set_component(index, component);
            }
        }
        self.set_input_default(id, port, value)
    }
}
