//! One mailbox command at a time; no tensor payload or unbounded queue in IPC.
use serde::Deserialize;
use std::path::{Path, PathBuf};

pub const COMMAND_BYTES: u64 = 16 * 1024;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Command {
    pub schema: u32,
    pub sequence: u64,
    pub request: PathBuf,
    pub output: PathBuf,
}
impl Command {
    pub fn validate(&self, previous: u64, workspace: &Path) -> Result<(), String> {
        if self.schema != 1 || previous.checked_add(1) != Some(self.sequence) {
            return Err("retained command schema or sequence differs".into());
        }
        if !self.request.is_absolute() || !self.output.is_absolute() {
            return Err("retained command requires absolute paths".into());
        }
        let request = self.request.canonicalize().map_err(|e| e.to_string())?;
        let parent = request.parent().ok_or("retained request parent missing")?;
        if !parent.starts_with(workspace)
            || parent == workspace
            || request != parent.join("native-request.json")
            || self.output != parent.join("native")
            || self.output.exists()
        {
            return Err(
                "retained request/output must stay in a fresh owned request directory".into(),
            );
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn admits_only_the_next_sequence_in_a_fresh_owned_request_directory() {
        let root = tempfile::tempdir().unwrap();
        let root = root.path().canonicalize().unwrap();
        let job = root.join("job");
        std::fs::create_dir(&job).unwrap();
        let request = job.join("native-request.json");
        std::fs::write(&request, b"{}").unwrap();
        let mut command = Command {
            schema: 1,
            sequence: 1,
            request,
            output: job.join("native"),
        };
        command.validate(0, &root).unwrap();
        assert!(command.validate(1, &root).is_err());
        command.sequence = 0;
        assert!(command.validate(u64::MAX, &root).is_err());
        command.sequence = 1;
        assert!(command.validate(0, &job).is_err());
        std::fs::create_dir(&command.output).unwrap();
        assert!(command.validate(0, &root).is_err());
    }
    #[test]
    fn refuses_unknown_fields_and_relative_paths() {
        assert!(
            serde_json::from_str::<Command>(
                r#"{"schema":1,"sequence":1,"request":"a","output":"b","extra":true}"#
            )
            .is_err()
        );
        let command: Command =
            serde_json::from_str(r#"{"schema":1,"sequence":1,"request":"a","output":"b"}"#)
                .unwrap();
        assert!(command.validate(0, Path::new(".")).is_err());
    }
}
