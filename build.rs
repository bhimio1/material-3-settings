//! Embeds the shell injection packs in `inject/` into the binary, as
//! `PACK_FILES: &[(&str, &str)]` (path relative to `inject/`, contents).

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    let mut entries: Vec<_> = rd.flatten().map(|e| e.path()).collect();
    entries.sort();
    for p in entries {
        if p.is_dir() {
            walk(&p, out);
        } else {
            out.push(p);
        }
    }
}

fn main() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("inject");
    println!("cargo:rerun-if-changed=inject");
    let mut files = Vec::new();
    walk(&root, &mut files);
    let mut src = String::from("pub static PACK_FILES: &[(&str, &str)] = &[\n");
    for f in &files {
        println!("cargo:rerun-if-changed={}", f.display());
        let rel = f.strip_prefix(&root).unwrap().to_string_lossy().replace('\\', "/");
        writeln!(src, "    ({rel:?}, include_str!({:?})),", f.display().to_string()).unwrap();
    }
    src.push_str("];\n");
    let out = PathBuf::from(std::env::var("OUT_DIR").unwrap()).join("packs.rs");
    std::fs::write(out, src).unwrap();
}
