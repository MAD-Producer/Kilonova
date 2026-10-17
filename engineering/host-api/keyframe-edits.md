# Keyframe destinations

`move_key` and `copy_key` require an existing source key and a vacant destination
on the edited track. An occupied destination returns an error rather than
replacing its value, easing, temporal curve, or spatial handles. This also applies
to effect parameters and color transfer curve keys.

Command frames are composition frames; layer-local conversion happens before
checking the stored destination. Moving or copying an existing key to its own
frame succeeds without changing the project, revision, or history. A missing
source still fails, including when `from == to`.

For separated dimensions, an explicit `axis` edits that axis only. Whole-vector
operations retain their existing all-axis validation and reject occupied
destinations without committing a partial axis edit.

Collisions return through the existing command `Result` and host error reply.
Generic and color curve collisions use `keyframe destination is occupied`;
partially occupied separated destinations retain the more specific whole-axis
error. A failed command batch rolls back all earlier commands in that batch and
preserves revision, undo/redo history, and any previously saved project file.
When moving multiple keys, each destination must be vacant when its command is
applied; the host does not implicitly reorder a batch or overwrite another key.

Value editing (`set_at`, `upsert`, and their corresponding set commands) retains
its intentional replacement behavior. To replace a destination using key editing,
clients must explicitly delete that key before copying or moving into its frame.
