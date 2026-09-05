use std::collections::BTreeSet;
use wasmparser::{Parser, Payload};
use wit_component::ComponentEncoder;

pub fn encode_component(module: &[u8]) -> Result<Vec<u8>, String> {
    ComponentEncoder::default()
        .validate(true)
        .module(module)
        .map_err(|err| format!("module: {err}"))?
        .encode()
        .map_err(|err| format!("component: {err}"))
}

pub fn component_imports(wasm: &[u8]) -> Result<Vec<String>, String> {
    let mut names = BTreeSet::new();
    for payload in Parser::new(0).parse_all(wasm) {
        let payload = payload.map_err(|err| format!("wasm: {err}"))?;
        if let Payload::ComponentImportSection(section) = payload {
            for import in section {
                let import = import.map_err(|err| format!("import: {err}"))?;
                let name = import.name.0.to_string();
                if name.contains(':') {
                    names.insert(name);
                }
            }
        }
    }
    Ok(names.into_iter().collect())
}
