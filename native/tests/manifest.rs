//! The manifest's tab: its `entry` names an existing `.wasm` file inside the
//! kit, and that file is a module Hangar will load (the ABI of
//! docs/native/kits-native.md). The rest is checked by test/manifest.test.js.

mod common;

use common::{kit, manifest};

fn tab() -> serde_json::Value {
    manifest()["tabs"][0].clone()
}

#[test]
fn the_manifest_is_schema_2_with_the_module_as_the_entry() {
    let manifest = manifest();
    assert_eq!(manifest["schema"], 2, "a native tab needs schema 2");
    assert_eq!(manifest["hangar"], ">=2.0.0");
    let tab = tab();
    assert_eq!(tab["entry"], "tabs/services.wasm");
    assert!(
        tab.get("native").is_none(),
        "schema 2 has no `native` field"
    );
    assert_eq!(tab["id"], "services");
    assert_eq!(tab["singleton"], true);
}

#[test]
fn the_entry_names_an_existing_wasm_file_inside_the_kit() {
    let tab = tab();
    let native = tab["entry"].as_str().expect("entry is a path");
    assert!(
        !native.starts_with('/') && !native.starts_with('\\') && !native.contains(':'),
        "{native}: must stay inside the kit"
    );
    assert!(
        !native
            .split(['/', '\\'])
            .any(|part| part == ".." || part.starts_with('.')),
        "{native}: no parent, no hidden part"
    );
    assert!(native.to_lowercase().ends_with(".wasm"), "{native}: .wasm");
    let file = kit().join(native);
    assert!(file.is_file(), "{native} must exist");
    let inside = file
        .canonicalize()
        .unwrap()
        .starts_with(kit().canonicalize().unwrap());
    assert!(inside, "{native} is outside the kit");
    assert!(
        file.metadata().unwrap().len() < 64 * 1024 * 1024,
        "a kit's file is 64 MB at most"
    );
}

#[test]
fn the_permissions_and_commands_are_the_five_the_tab_needs() {
    // The tab asks for these five permissions and no more.
    let mut granted = common::granted();
    granted.sort();
    assert_eq!(
        granted,
        [
            "command:docker-list",
            "command:docker-logs",
            "command:ports",
            "panes.read",
            "panes.send"
        ]
    );
    let manifest = manifest();
    for permission in granted
        .iter()
        .filter_map(|permission| permission.strip_prefix("command:"))
    {
        assert!(
            manifest["commands"][permission].is_object(),
            "{permission} is declared"
        );
    }
}

// ---------------------------------------------------------------- The module.

fn leb(bytes: &[u8], at: &mut usize) -> usize {
    let (mut value, mut shift) = (0usize, 0);
    loop {
        let byte = bytes[*at];
        *at += 1;
        value |= usize::from(byte & 0x7f) << shift;
        if byte & 0x80 == 0 {
            return value;
        }
        shift += 7;
    }
}

fn name(bytes: &[u8], at: &mut usize) -> String {
    let len = leb(bytes, at);
    let text = String::from_utf8(bytes[*at..*at + len].to_vec()).expect("names are UTF-8");
    *at += len;
    text
}

/// The `(module, name)` of every import and the name of every export.
fn imports_and_exports(bytes: &[u8]) -> (Vec<(String, String)>, Vec<String>) {
    assert_eq!(&bytes[..8], b"\0asm\x01\0\0\0", "a WebAssembly module");
    let (mut imports, mut exports) = (Vec::new(), Vec::new());
    let mut at = 8;
    while at < bytes.len() {
        let id = bytes[at];
        at += 1;
        let size = leb(bytes, &mut at);
        let end = at + size;
        let mut body = at;
        match id {
            2 => {
                for _ in 0..leb(bytes, &mut body) {
                    let module = name(bytes, &mut body);
                    let field = name(bytes, &mut body);
                    let kind = bytes[body];
                    body += 1;
                    match kind {
                        0 => {
                            leb(bytes, &mut body);
                        }
                        1 => {
                            body += 1;
                            let flags = bytes[body];
                            body += 1;
                            leb(bytes, &mut body);
                            if flags & 1 != 0 {
                                leb(bytes, &mut body);
                            }
                        }
                        2 => {
                            let flags = bytes[body];
                            body += 1;
                            leb(bytes, &mut body);
                            if flags & 1 != 0 {
                                leb(bytes, &mut body);
                            }
                        }
                        3 => body += 2,
                        other => panic!("an import of kind {other}"),
                    }
                    imports.push((module, field));
                }
            }
            7 => {
                for _ in 0..leb(bytes, &mut body) {
                    exports.push(name(bytes, &mut body));
                    body += 1;
                    leb(bytes, &mut body);
                }
            }
            _ => {}
        }
        at = end;
    }
    (imports, exports)
}

#[test]
fn the_module_imports_hangars_log_and_now_and_nothing_else() {
    // Hangar refuses a module importing WASI or wasm-bindgen glue.
    let bytes = std::fs::read(kit().join("tabs/services.wasm")).unwrap();
    let (mut imports, mut exports) = imports_and_exports(&bytes);
    imports.sort();
    assert_eq!(
        imports,
        [
            ("hangar".to_owned(), "log".to_owned()),
            ("hangar".to_owned(), "now".to_owned())
        ]
    );
    exports.sort();
    for needed in [
        "hangar_abi_version",
        "hangar_alloc",
        "hangar_init",
        "hangar_update",
        "hangar_view",
        "memory",
    ] {
        assert!(
            exports.iter().any(|export| export == needed),
            "exports {needed}: {exports:?}"
        );
    }
}

#[test]
fn the_module_is_small_and_carries_no_path_of_the_machine_that_built_it() {
    let bytes = std::fs::read(kit().join("tabs/services.wasm")).unwrap();
    assert!(
        bytes.len() < 1024 * 1024,
        "{} bytes: a tab is 300 to 600 KB",
        bytes.len()
    );
    let text = String::from_utf8_lossy(&bytes);
    let home = std::env::var("HOME").unwrap_or_default();
    if !home.is_empty() {
        assert!(!text.contains(&home), "the module names {home}");
    }
    for personal in ["/home/", "/Users/", "C:\\Users"] {
        assert!(!text.contains(personal), "the module names {personal}");
    }
}
