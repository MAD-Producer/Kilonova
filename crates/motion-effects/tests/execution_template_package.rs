use motion_effects::execution_plan::{ExecutionCapability, ExecutionPlanError};
use motion_effects::execution_template::{
    EXECUTION_TEMPLATE_CAPABILITY, ExecutionTemplate, TemplateError, TemplateValue,
};
use motion_effects::{EffectPackage, Error, PassDefinition};
use std::{
    collections::BTreeMap,
    io::{Cursor, Write},
};

const PATH: &str = "shaders/runtime.wgsl";
const SOURCE: &str = "fn main_fx(p:vec2<f32>)->vec4<f32>{return vec4(0.2,0.4,0.6,1.0);}";
fn files() -> BTreeMap<String, Vec<u8>> {
    let mut manifest = motion_effects::builtin::manifest();
    manifest.id = "test.runtime-template-loader".into();
    let mut definition = manifest
        .effects
        .iter()
        .find(|e| e.id == "invert")
        .unwrap()
        .clone();
    definition.params.clear();
    definition.resources.clear();
    definition.shader_data = None;
    definition.native_editor = None;
    definition.editor = None;
    definition.passes = vec![PassDefinition {
        shader: PATH.into(),
        entry: "main_fx".into(),
    }];
    definition.required_capabilities = vec![EXECUTION_TEMPLATE_CAPABILITY.into()];
    let shader = motion_effects::shader::compile(SOURCE, "main_fx").unwrap();
    definition.execution = Some(ExecutionTemplate::single_pass(&definition, &shader));
    manifest.effects = vec![definition];
    BTreeMap::from([
        (
            "manifest.json".into(),
            serde_json::to_vec(&manifest).unwrap(),
        ),
        (PATH.into(), SOURCE.as_bytes().to_vec()),
    ])
}
fn bytes(files: &BTreeMap<String, Vec<u8>>) -> Vec<u8> {
    let mut archive = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for (name, data) in files {
        archive
            .start_file(name, zip::write::SimpleFileOptions::default())
            .unwrap();
        archive.write_all(data).unwrap();
    }
    archive.finish().unwrap().into_inner()
}
fn edit(files: &mut BTreeMap<String, Vec<u8>>, update: impl FnOnce(&mut ExecutionTemplate)) {
    let mut manifest: motion_effects::PluginManifest =
        serde_json::from_slice(&files["manifest.json"]).unwrap();
    update(manifest.effects[0].execution.as_mut().unwrap());
    files.insert(
        "manifest.json".into(),
        serde_json::to_vec(&manifest).unwrap(),
    );
}

#[test]
fn actual_package_loader_refuses_runtime_version_and_capabilities_before_shader_parsing() {
    let healthy = files();
    let mut invalid = healthy.clone();
    edit(&mut invalid, |p| p.version += 1);
    invalid.insert(PATH.into(), b"this is invalid WGSL".to_vec());
    assert!(matches!(
        EffectPackage::from_bytes(bytes(&invalid)),
        Err(Error::ExecutionTemplate(TemplateError::Version { .. }))
    ));
    invalid = healthy.clone();
    edit(&mut invalid, |p| {
        p.required_capabilities.insert(ExecutionCapability::History);
    });
    invalid.insert(PATH.into(), b"this is invalid WGSL".to_vec());
    assert!(matches!(
        EffectPackage::from_bytes(bytes(&invalid)),
        Err(Error::ExecutionTemplate(TemplateError::Plan(
            ExecutionPlanError::UnsupportedCapability {
                capability: ExecutionCapability::History
            }
        )))
    ));
    assert!(EffectPackage::from_bytes(bytes(&healthy)).is_ok());
}

#[test]
fn actual_package_loader_binds_template_to_real_compiled_code_and_recovers() {
    let healthy = files();
    let mut invalid = healthy.clone();
    invalid.insert(PATH.into(), SOURCE.replace("0.2", "0.8").into_bytes());
    assert!(matches!(
        EffectPackage::from_bytes(bytes(&invalid)),
        Err(Error::ExecutionTemplate(TemplateError::Code {
            node: 0,
            pass: 0
        }))
    ));
    let reopened = EffectPackage::from_bytes(bytes(&healthy)).unwrap();
    assert!(reopened.manifest.effects[0].execution.is_some());
    let reopened_again = EffectPackage::from_bytes(reopened.bytes.clone()).unwrap();
    assert_eq!(reopened_again.manifest, reopened.manifest);
    assert_eq!(reopened_again.hash, reopened.hash);
}

#[test]
fn actual_package_loader_preserves_typed_cycles_and_contract_failures() {
    let healthy = files();
    let mut invalid = healthy.clone();
    edit(&mut invalid, |p| {
        p.nodes[0].input = TemplateValue::Node { id: 0 }
    });
    assert!(matches!(
        EffectPackage::from_bytes(bytes(&invalid)),
        Err(Error::ExecutionTemplate(TemplateError::Plan(
            ExecutionPlanError::Cycle { total: 1, .. }
        )))
    ));
    invalid = healthy.clone();
    edit(&mut invalid, |p| {
        p.resources.push("assets/undeclared.png".into())
    });
    assert!(matches!(
        EffectPackage::from_bytes(bytes(&invalid)),
        Err(Error::ExecutionTemplate(TemplateError::Contract))
    ));
    assert!(EffectPackage::from_bytes(bytes(&healthy)).is_ok());
}
