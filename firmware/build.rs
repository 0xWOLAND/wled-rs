//! Embeds the gzipped web UI (`../web/dist`, produced by `cargo xtask build`) into the firmware.

use std::{
    env, fs,
    io::Write as _,
    path::{Path, PathBuf},
};

use flate2::{write::GzEncoder, Compression};

fn main() {
    embed_web_assets();
    embuild::espidf::sysenv::output();
}

fn embed_web_assets() {
    let dist = Path::new(env!("CARGO_MANIFEST_DIR")).join("../web/dist");
    let out = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    println!("cargo:rerun-if-changed={}", dist.display());

    let mut files: Vec<_> = fs::read_dir(&dist)
        .unwrap_or_else(|_| {
            panic!(
                "{} not found: build with `cargo xtask build`",
                dist.display()
            )
        })
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.is_file())
        .collect();
    files.sort();
    assert!(
        files.iter().any(|p| p.ends_with("index.html")),
        "web/dist has no index.html: build with `cargo xtask build`"
    );

    let mut table = String::from("&[\n");
    for path in files {
        println!("cargo:rerun-if-changed={}", path.display());
        let name = path.file_name().unwrap().to_str().unwrap();
        let gz = out.join(format!("{name}.gz"));

        let mut encoder = GzEncoder::new(Vec::new(), Compression::best());
        encoder.write_all(&fs::read(&path).unwrap()).unwrap();
        fs::write(&gz, encoder.finish().unwrap()).unwrap();

        table += &format!(
            "    Asset {{ path: \"/{name}\", mime: \"{}\", body: include_bytes!({:?}) }},\n",
            mime(&path),
            gz.display().to_string(),
        );
    }
    table += "]\n";
    fs::write(out.join("assets.rs"), table).unwrap();
}

fn mime(path: &Path) -> &'static str {
    match path.extension().and_then(|e| e.to_str()) {
        Some("html") => "text/html; charset=utf-8",
        Some("js") => "text/javascript",
        Some("wasm") => "application/wasm",
        Some("css") => "text/css",
        _ => "application/octet-stream",
    }
}
