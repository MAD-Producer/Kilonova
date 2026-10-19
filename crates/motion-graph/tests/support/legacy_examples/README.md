# Frozen GPU references

These three graphs are Motion Studio's original MIT-licensed teaching examples,
frozen from commit `fb34584f51067d782302b851959f44a3225a6034` before the compact
example refactor. JSON whitespace is compressed; document values are unchanged.
They contain no external images, presets, third-party resources or proprietary
project data. They are test references, not additional entries in the library.

`compact_examples_gpu.rs` exports and renders both these references and the
current built-in examples. It compares actual pixels across parameter bounds,
translucent color, animated parameters, aspect ratios, fractional time and seek
recovery. Do not regenerate these files from the compact examples: doing so
would remove the independent pre-refactor reference.
