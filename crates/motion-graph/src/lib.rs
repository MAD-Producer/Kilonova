//! N1 effect graphs: typed documents, checked DAGs, one image pass and real .msfx.
mod blend;
mod compiler;
mod demo;
mod document;
mod error;
mod examples;
mod export;
mod groups;
mod input_defaults;
mod ports;
mod runtime_data;
pub use blend::ColorBlendMode;
pub use compiler::{
    compile, compile_editable, compile_preview, compile_preview_editable, node_ports, validate, CompileOptions, CompileReport, CompiledGraph,
    NodePorts, ValidationReport,
};
pub use demo::demo_graph;
pub use document::*;
pub use error::{Diagnostic, Error, ErrorCode, Result};
pub use examples::{
    builtin_example_catalogue, builtin_examples, BuiltinExample, ExampleCollection,
};
pub use export::{compiled_package_bytes, export_msfx, ExportReport};
pub use groups::scope_topological_order;
pub use input_defaults::authoring_input_defaults;
pub use ports::{
    check_connection, node_description, node_port_specs, port_semantics_compatible,
    ports_compatible, PortSemantic, PortSpec, PortSpecs,
};
