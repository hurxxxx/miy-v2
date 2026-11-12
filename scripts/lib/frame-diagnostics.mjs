// Frame asset paths and bearer values can occur in browser exception messages.
// Redact before truncating or handing the text to a console/test reporter.
export function frameDiagnostic(message) {
  return message
    .replace(/\/_miy\/a\/[^\s/?#"'<>]+/g, '/_miy/a/[redacted]')
    .replace(/miy_(?:frame|access|refresh|call|delegate)_[A-Za-z0-9_-]+/g, '[redacted credential]');
}
