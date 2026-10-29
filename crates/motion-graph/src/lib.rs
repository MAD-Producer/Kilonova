//! N1 effect graphs: typed documents, checked DAGs, one image pass and real .msfx.
mod blend;
mod compiler;
mod compiler_order;
mod demo;
mod document;
mod editable_compiler;
mod error;
mod examples;
mod export;
mod groups;
mod images;
mod input_defaults;
mod ports;
mod runtime_data;
pub mod plugin_project;
pub use blend::ColorBlendMode;
pub use compiler::{
    compile, compile_editable, compile_preview, compile_preview_editable, node_ports, validate,
    CompileOptions, CompileReport, CompiledGraph, NodePorts, ValidationReport,
};
pub use demo::demo_graph;
pub use document::*;
pub use editable_compiler::{
    EditableCompiler, EditableCompilerStats, EDITABLE_CACHE_CODE_BYTES, EDITABLE_CACHE_ENTRIES,
};
pub use error::{Diagnostic, Error, ErrorCode, Result};
pub use examples::{
    builtin_example_catalogue, builtin_examples, BuiltinExample, ExampleCollection,
};
pub use export::{compiled_package_bytes, export_msfx, ExportReport};
pub use groups::scope_topological_order;
pub use images::{GraphImage, IMAGE_ASSETS_FORMAT_VERSION, MAX_GRAPH_IMAGES, MAX_GRAPH_IMAGE_BYTES,
    MAX_IMAGE_SOURCE_BYTES, MAX_GRAPH_METADATA_BYTES};
pub use input_defaults::authoring_input_defaults;
pub use ports::{
    check_connection, node_description, node_port_specs, port_semantics_compatible,
    ports_compatible, PortSemantic, PortSpec, PortSpecs,
};
