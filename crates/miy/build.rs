use std::{env, fs, path::Path};
fn visit(root: &Path, path: &Path, entries: &mut Vec<String>) {
    for entry in fs::read_dir(path).unwrap() {
        let path = entry.unwrap().path();
        if matches!(
            path.file_name().and_then(|v| v.to_str()),
            Some(
                "node_modules" | "target" | "dist" | ".git" | ".pnpm-store" | ".miy" | ".miy-tools"
            )
        ) {
            continue;
        }
        if path.is_dir() {
            visit(root, &path, entries);
        } else {
            entries.push(format!(
                "({:?}, include_str!({:?})),",
                path.strip_prefix(root).unwrap().to_str().unwrap(),
                path.to_str().unwrap()
            ));
            println!("cargo:rerun-if-changed={}", path.display());
        }
    }
}
fn main() {
    let root = Path::new(&env::var("CARGO_MANIFEST_DIR").unwrap())
        .join("../../templates/rust-react")
        .canonicalize()
        .unwrap();
    let mut entries = vec![];
    visit(&root, &root, &mut entries);
    entries.sort();
    fs::write(
        Path::new(&env::var("OUT_DIR").unwrap()).join("template.rs"),
        format!(
            "const TEMPLATE: &[(&str,&str)] = &[{}];",
            entries.join("\n")
        ),
    )
    .unwrap();
    println!("cargo:rerun-if-changed={}", root.display());
}
