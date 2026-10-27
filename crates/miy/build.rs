use std::{env, fs, path::Path};

fn logical_path(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap()
        .components()
        .map(|part| {
            let std::path::Component::Normal(part) = part else {
                panic!("template path must be relative: {}", path.display());
            };
            let part = part.to_str().expect("template path must be UTF-8");
            assert!(
                !part.contains('\\'),
                "template path must not contain a backslash: {}",
                path.display()
            );
            part
        })
        .collect::<Vec<_>>()
        .join("/")
}

fn verify_text(path: &Path) {
    let content = fs::read_to_string(path).expect("embedded text must be UTF-8");
    // Never rewrite migration bytes: SQLx and installed apps retain their hashes.
    if path.extension().is_none_or(|extension| extension != "sql") {
        assert!(
            !content.contains('\r'),
            "managed text must use LF; restore the checkout using .gitattributes: {}",
            path.display()
        );
    }
}

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
        let metadata = fs::symlink_metadata(&path).unwrap();
        assert!(
            !metadata.file_type().is_symlink(),
            "embedded template must not contain symlinks: {}",
            path.display()
        );
        if metadata.is_dir() {
            visit(root, &path, entries);
        } else {
            assert!(metadata.is_file(), "template entry must be a file");
            verify_text(&path);
            entries.push(format!(
                "({:?}, include_str!({:?})),",
                logical_path(root, &path),
                path.to_str().unwrap()
            ));
            println!("cargo:rerun-if-changed={}", path.display());
        }
    }
}
fn main() {
    let manifest_dir = env::var("CARGO_MANIFEST_DIR").unwrap();
    let root = Path::new(&manifest_dir)
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
    for relative in [
        "guidance/miy-cli/SKILL.md",
        "guidance/miy-cli/references/workflow.md",
    ] {
        let path = Path::new(&manifest_dir).join(relative);
        verify_text(&path);
        println!("cargo:rerun-if-changed={}", path.display());
    }
}
