//! Local advisory checks for common SDK and UI integration mistakes.
use anyhow::Result;
use serde::Serialize;
use std::{fs, path::Path};

#[derive(Serialize)]
pub(crate) struct Finding {
    code: &'static str,
    path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    line: Option<usize>,
    message: &'static str,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Token<'a> {
    Word(&'a str),
    Text(&'a str),
    Punct(u8),
}

fn tokens(source: &str) -> Vec<(Token<'_>, usize)> {
    let bytes = source.as_bytes();
    let mut output = Vec::new();
    let mut offset = 0;
    let mut line = 1;
    while let Some(&byte) = bytes.get(offset) {
        let rest = &bytes[offset..];
        if byte.is_ascii_whitespace() {
            line += usize::from(byte == b'\n');
            offset += 1;
        } else if rest.starts_with(b"//") {
            offset += rest
                .iter()
                .position(|&value| value == b'\n')
                .unwrap_or(rest.len());
        } else if rest.starts_with(b"/*") {
            let length = rest[2..]
                .windows(2)
                .position(|end| end == b"*/")
                .map_or(rest.len(), |i| i + 4);
            line += rest[..length]
                .iter()
                .filter(|&&value| value == b'\n')
                .count();
            offset += length;
        } else if matches!(byte, b'\'' | b'"' | b'`') {
            let start = offset + 1;
            let start_line = line;
            offset = start;
            while let Some(&value) = bytes.get(offset).filter(|&&value| value != byte) {
                if value == b'\\' && offset + 1 < bytes.len() {
                    offset += 1;
                }
                line += usize::from(bytes[offset] == b'\n');
                offset += 1;
            }
            output.push((Token::Text(&source[start..offset]), start_line));
            offset += 1;
        } else if byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'$') {
            let length = rest
                .iter()
                .take_while(|&&value| value.is_ascii_alphanumeric() || matches!(value, b'_' | b'$'))
                .count();
            output.push((Token::Word(&source[offset..offset + length]), line));
            offset += length;
        } else {
            output.push((Token::Punct(byte), line));
            offset += 1;
        }
    }
    output
}

const UUID: &str = "Browser crypto.randomUUID() needs a secure context. Use requestId() from @miy/components/ui for app and request IDs on internal HTTP as well as HTTPS.";
const FILE_PATH: &str = "fileApi adds /api/files itself. Check this /files prefix against the app backend route to avoid a duplicated segment.";
const UI: &str = "The synced UI package is not imported by the app's web sources. Review DirectoryPicker, ShareEditor and FileTransferPanel for the flows this app uses.";
const STORAGE: &str = "For write flows, persist reservation intent before calling Storage; recover a lost reply using the same request ID, and record detach work until confirmed. Test current ACLs on downloads. See describe --topic harness.";

fn examine(source: &str, path: &str, findings: &mut Vec<Finding>) -> (bool, bool) {
    use Token::{Punct, Text, Word};
    let parsed = tokens(source);
    let mut uses_ui = false;
    let mut uses_directory_or_files = false;
    let mut add = |code, line, message| {
        findings.push(Finding {
            code,
            path: path.into(),
            line: Some(line),
            message,
        })
    };
    for (index, &(token, line)) in parsed.iter().enumerate() {
        let next = |n| parsed.get(index + n).map(|&(value, _)| value);
        match (token, next(1), next(2), next(3)) {
            (Word("crypto"), Some(Punct(b'.')), Some(Word("randomUUID")), Some(Punct(b'('))) => {
                add("browser_uuid_secure_context", line, UUID)
            }
            (Punct(b'.'), Some(Word("fileApi")), Some(Punct(b'(')), argument) => {
                uses_directory_or_files = true;
                if matches!(argument, Some(Text(value)) if value.starts_with("/files/")) {
                    add("file_api_route_prefix", line, FILE_PATH);
                }
            }
            (Punct(b'.'), Some(Word("directory")), _, _) => uses_directory_or_files = true,
            (Word("from" | "import"), Some(Text(value)), _, _)
            | (Word("import"), Some(Punct(b'(')), Some(Text(value)), _) => {
                uses_ui |= value == "@miy/ui"
                    || value == "@miy/components/ui"
                    || value.starts_with("@miy/components/ui/")
                    || value.contains("/platform-components/ui");
            }
            _ => {}
        }
    }
    (uses_ui, uses_directory_or_files)
}

pub(crate) fn inspect(root: &Path, manifest: &crate::contract::Manifest) -> Result<Vec<Finding>> {
    let mut findings = Vec::new();
    let mut uses_ui = false;
    let mut uses_directory_or_files = false;
    crate::platform::reject_reparse(&root.join("web"))?;
    let mut directories = vec![root.join("web/src")];
    while let Some(directory) = directories.pop() {
        crate::platform::reject_reparse(&directory)?;
        for entry in fs::read_dir(directory)? {
            let entry = entry?;
            let path = entry.path();
            let kind = entry.file_type()?;
            if kind.is_dir() && entry.file_name() != "node_modules" {
                directories.push(path);
            } else if kind.is_file()
                && matches!(
                    path.extension().and_then(|ext| ext.to_str()),
                    Some("ts" | "tsx" | "js" | "jsx")
                )
            {
                crate::platform::reject_reparse(&path)?;
                let relative = path
                    .strip_prefix(root)?
                    .to_string_lossy()
                    .replace('\\', "/");
                let (ui, context) = examine(&fs::read_to_string(path)?, &relative, &mut findings);
                uses_ui |= ui;
                uses_directory_or_files |= context;
            }
        }
    }
    if uses_directory_or_files
        && !uses_ui
        && root.join("platform-components/ui/index.tsx").is_file()
    {
        findings.push(Finding {
            code: "synced_ui_reuse_review",
            path: "web/src".into(),
            line: None,
            message: UI,
        });
    }
    if manifest
        .requested_permissions
        .iter()
        .any(|permission| permission.starts_with("platform:files:"))
    {
        findings.push(Finding {
            code: "storage_recovery_review",
            path: "manifest.json".into(),
            line: None,
            message: STORAGE,
        });
    }
    findings.sort_by(|a, b| (&a.path, a.line, a.code).cmp(&(&b.path, b.line, b.code)));
    Ok(findings)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn advises_on_direct_browser_calls_and_prefixed_paths_without_scanning_comments() {
        let mut findings = Vec::new();
        let (ui, uses_platform) = examine(
            "// crypto.randomUUID()\nconst docs = 'crypto.randomUUID()'; /* ignored\n crypto.randomUUID() */\nconst id = window.crypto /* browser */ .randomUUID();\nruntime.fileApi(\x60/files/\x24{id}/content\x60);\nruntime.fileApi('/records/42/content');\nimport { FileTransferPanel } from '@miy/components/ui';",
            "web/src/AppRoot.tsx",
            &mut findings,
        );
        assert!(ui && uses_platform);
        assert_eq!(
            findings
                .iter()
                .map(|finding| (finding.code, finding.line))
                .collect::<Vec<_>>(),
            [
                ("browser_uuid_secure_context", Some(4)),
                ("file_api_route_prefix", Some(5)),
            ]
        );
    }
}
