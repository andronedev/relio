//! Embeds every `i18n/<lang>.json` into the binary, plus an index of language names.

use std::fmt::Write;
use std::path::PathBuf;

fn main() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../i18n");
    println!("cargo:rerun-if-changed={}", dir.display());

    let mut files: Vec<PathBuf> = std::fs::read_dir(&dir)
        .expect("i18n directory")
        .map(|entry| entry.expect("i18n entry").path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "json"))
        .collect();
    files.sort();

    let mut index = serde_json::Map::new();
    let mut code = String::from("pub const LOCALES: &[(&str, &[u8])] = &[\n");
    for path in &files {
        println!("cargo:rerun-if-changed={}", path.display());
        let lang = path.file_stem().and_then(|s| s.to_str()).expect("locale name");
        let json: serde_json::Value = serde_json::from_slice(&std::fs::read(path).expect("locale file"))
            .unwrap_or_else(|err| panic!("{}: {err}", path.display()));
        index.insert(lang.to_owned(), json["meta"].clone());
        writeln!(code, "    ({lang:?}, include_bytes!({:?})),", path.canonicalize().expect("locale path")).unwrap();
    }
    code.push_str("];\n");
    writeln!(code, "pub const LOCALE_INDEX: &str = {:?};", serde_json::Value::Object(index).to_string()).unwrap();

    let out = PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR")).join("locales.rs");
    std::fs::write(out, code).expect("write locales.rs");
}
