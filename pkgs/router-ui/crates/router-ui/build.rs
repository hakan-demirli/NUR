use std::fmt::Write as _;

use serde_json::Value;
use std::env;
use std::fs;
use std::path::PathBuf;

fn main() {
    let manifest = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let json_path = manifest.join("themes/dracula.json");
    println!("cargo:rerun-if-changed={}", json_path.display());

    let raw = fs::read_to_string(&json_path).expect("read dracula.json");
    let root: Value = serde_json::from_str(&raw).expect("parse dracula.json");

    let defs = root["defs"].as_object().expect("defs must be an object");
    let theme = root["theme"].as_object().expect("theme must be an object");

    let mut out = String::from(
        "// AUTO-GENERATED from themes/dracula.json by build.rs — do not edit.\n\
         export global Dracula {\n",
    );

    for (def_name, def_val) in defs {
        if let Some(s) = def_val.as_str() {
            let _ = writeln!(
                out,
                "    out property <color> def_{}: {s};",
                sanitize_ident(def_name)
            );
        }
    }

    for (key, val) in theme {
        let dark = val["dark"]
            .as_str()
            .expect("each theme key needs a `dark` string");
        let resolved = resolve_color(dark, defs);
        let _ = writeln!(
            out,
            "    out property <color> {}: {resolved};",
            sanitize_ident(key)
        );
    }
    out.push_str("}\n");

    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    let dest = out_dir.join("dracula.slint");
    fs::write(&dest, &out).expect("write dracula.slint");
    eprintln!("wrote {}", dest.display());

    slint_build::compile_with_config(
        "ui/app.slint",
        slint_build::CompilerConfiguration::new()
            .with_style("fluent".into())
            .with_include_paths(vec![out_dir])
            .embed_resources(slint_build::EmbedResourcesKind::EmbedForSoftwareRenderer),
    )
    .expect("Slint compile failed");
}

fn resolve_color(name: &str, defs: &serde_json::Map<String, Value>) -> String {
    if let Some(v) = defs.get(name).and_then(Value::as_str) {
        v.to_string()
    } else {
        name.to_string()
    }
}

fn sanitize_ident(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 4);
    for (i, c) in s.chars().enumerate() {
        if c.is_ascii_uppercase() {
            if i != 0 {
                out.push('_');
            }
            out.push(c.to_ascii_lowercase());
        } else if c == '-' || c == ' ' {
            out.push('_');
        } else {
            out.push(c);
        }
    }
    out
}
