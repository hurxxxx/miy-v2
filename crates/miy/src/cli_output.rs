//! One renderer for command results; protocol streams keep their own stdout.
use crate::cli_error;
use clap::ValueEnum;
use serde_json::{Value, json};
use std::io::{self, IsTerminal, Write};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, ValueEnum)]
pub enum OutputMode {
    #[default]
    Auto,
    Text,
    Json,
    Jsonl,
}

impl OutputMode {
    pub fn resolve(self, is_terminal: bool) -> Self {
        match self {
            Self::Auto if is_terminal => Self::Text,
            Self::Auto => Self::Json,
            explicit => explicit,
        }
    }
}

#[derive(Debug)]
pub enum Outcome {
    Data(Value),
    Raw,
}

fn text_value(writer: &mut impl Write, value: &Value, indent: usize) -> io::Result<()> {
    let padding = " ".repeat(indent);
    match value {
        Value::Object(fields) if !fields.is_empty() => {
            for (key, value) in fields {
                if value.is_object() || value.is_array() {
                    writeln!(writer, "{padding}{key}:")?;
                    text_value(writer, value, indent + 2)?;
                } else {
                    write!(writer, "{padding}{key}: ")?;
                    text_scalar(writer, value, indent + 2)?;
                }
            }
        }
        Value::Array(items) if !items.is_empty() => {
            for item in items {
                if item.is_object() || item.is_array() {
                    writeln!(writer, "{padding}-")?;
                    text_value(writer, item, indent + 2)?;
                } else {
                    write!(writer, "{padding}- ")?;
                    text_scalar(writer, item, indent + 2)?;
                }
            }
        }
        _ => {
            write!(writer, "{padding}")?;
            text_scalar(writer, value, indent)?;
        }
    }
    Ok(())
}

fn text_scalar(writer: &mut impl Write, value: &Value, indent: usize) -> io::Result<()> {
    if let Some(text) = value.as_str() {
        for (index, line) in text.split('\n').enumerate() {
            if index != 0 {
                write!(writer, "{}", " ".repeat(indent))?;
            }
            writeln!(writer, "{line}")?;
        }
        Ok(())
    } else {
        writeln!(writer, "{value}")
    }
}

fn json_line(writer: &mut impl Write, value: &Value) -> io::Result<()> {
    let bytes = serde_json::to_vec(value).map_err(io::Error::other)?;
    writer.write_all(&bytes)?;
    writer.write_all(b"\n")
}

fn metadata(data: &Value) -> Value {
    let mut meta = serde_json::Map::new();
    if let Ok((name, context)) = crate::cli_config::selected() {
        meta.insert("context".into(), json!(name));
        meta.insert("origin".into(), json!(context.origin));
    }
    if let Some(cursor) = data.get("nextCursor") {
        meta.insert("nextCursor".into(), cursor.clone());
    }
    Value::Object(meta)
}

fn emit_to(
    mode: OutputMode,
    command: &str,
    result: &Result<Outcome, anyhow::Error>,
    raw: bool,
    stdout: &mut impl Write,
    stderr: &mut impl Write,
) -> io::Result<i32> {
    match result {
        Ok(Outcome::Raw) => Ok(0),
        Ok(Outcome::Data(data)) => {
            let meta = metadata(data);
            let projected;
            let data = if let Some(fields) = crate::cli_config::options().fields.as_deref() {
                match crate::cli_input::project(data, fields) {
                    Ok(value) => {
                        projected = value;
                        &projected
                    }
                    Err(error) => return emit_to(mode, command, &Err(error), raw, stdout, stderr),
                }
            } else {
                data
            };
            if mode == OutputMode::Text {
                text_value(stdout, data, 0)?;
            } else {
                json_line(
                    stdout,
                    &json!({
                        "schemaVersion":1,"command":command,"ok":true,
                        "data":data,"error":null,"meta":meta
                    }),
                )?;
            }
            Ok(0)
        }
        Err(error) => {
            let failure = cli_error::classify(error);
            if raw || mode == OutputMode::Text {
                writeln!(stderr, "error[{}]: {}", failure.code, failure.message)?;
            } else {
                json_line(
                    stdout,
                    &json!({
                        "schemaVersion":1,"command":command,"ok":false,
                        "data":null,"error":failure,"meta":{}
                    }),
                )?;
            }
            Ok(failure.exit())
        }
    }
}

pub fn emit(
    mode: OutputMode,
    command: &str,
    result: &Result<Outcome, anyhow::Error>,
    raw: bool,
) -> io::Result<i32> {
    let stdout = io::stdout();
    let mode = mode.resolve(stdout.is_terminal());
    emit_to(
        mode,
        command,
        result,
        raw,
        &mut stdout.lock(),
        &mut io::stderr().lock(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli_error::{Category, CliError};

    fn rendered(
        mode: OutputMode,
        result: Result<Outcome, anyhow::Error>,
        raw: bool,
    ) -> (i32, Vec<u8>, Vec<u8>) {
        let mut stdout = vec![];
        let mut stderr = vec![];
        let code = emit_to(mode, "app status", &result, raw, &mut stdout, &mut stderr).unwrap();
        (code, stdout, stderr)
    }

    #[test]
    fn auto_depends_only_on_stdout_tty_and_explicit_modes_are_preserved() {
        assert_eq!(OutputMode::Auto.resolve(true), OutputMode::Text);
        assert_eq!(OutputMode::Auto.resolve(false), OutputMode::Json);
        for mode in [OutputMode::Text, OutputMode::Json, OutputMode::Jsonl] {
            assert_eq!(mode.resolve(true), mode);
            assert_eq!(mode.resolve(false), mode);
        }
    }

    #[test]
    fn json_success_is_one_utf8_envelope_with_empty_stderr() {
        let (code, stdout, stderr) = rendered(
            OutputMode::Json,
            Ok(Outcome::Data(
                json!({"name":"한글 앱","value":"line one\nline two"}),
            )),
            false,
        );
        assert_eq!(code, 0);
        assert!(stderr.is_empty());
        let text = std::str::from_utf8(&stdout).unwrap();
        assert!(text.contains("한글 앱"));
        assert_eq!(text.lines().count(), 1);
        assert_eq!(
            serde_json::from_slice::<Value>(&stdout).unwrap(),
            json!({
                "schemaVersion":1,"command":"app status","ok":true,
                "data":{"name":"한글 앱","value":"line one\nline two"},"error":null,"meta":{}
            })
        );
    }

    #[test]
    fn json_failure_is_on_stdout_and_returns_the_typed_exit_code() {
        let (code, stdout, stderr) = rendered(
            OutputMode::Json,
            Err(CliError::new(Category::Auth, "login_required", "Sign in to continue.").into()),
            false,
        );
        assert_eq!(code, 3);
        assert!(stderr.is_empty());
        let value: Value = serde_json::from_slice(&stdout).unwrap();
        assert_eq!(value["schemaVersion"], 1);
        assert_eq!(value["ok"], false);
        assert!(value["data"].is_null());
        assert_eq!(value["error"]["code"], "login_required");
        assert_eq!(value["error"]["retryable"], false);
        assert_eq!(value["meta"], json!({}));
        assert_eq!(std::str::from_utf8(&stdout).unwrap().lines().count(), 1);
    }

    #[test]
    fn protocol_streams_never_receive_an_envelope() {
        let (code, stdout, stderr) = rendered(OutputMode::Json, Ok(Outcome::Raw), true);
        assert_eq!(code, 0);
        assert!(stdout.is_empty());
        assert!(stderr.is_empty());
        let (code, stdout, stderr) = rendered(
            OutputMode::Json,
            Err(anyhow::anyhow!("secret-token-from-protocol-error")),
            true,
        );
        assert_eq!(code, 1);
        assert!(stdout.is_empty());
        let stderr = std::str::from_utf8(&stderr).unwrap();
        assert!(stderr.starts_with("error[internal_error]:"));
        assert!(!stderr.contains("secret-token"));
    }

    #[test]
    fn text_renders_labeled_nested_data_and_errors_go_to_stderr() {
        let (code, stdout, stderr) = rendered(
            OutputMode::Text,
            Ok(Outcome::Data(
                json!({"name":"한글 앱","health":{"status":"ready"},"checks":["one","two"]}),
            )),
            false,
        );
        assert_eq!(code, 0);
        assert!(stderr.is_empty());
        let text = std::str::from_utf8(&stdout).unwrap();
        assert!(text.contains("name: 한글 앱\n"));
        assert!(text.contains("health:\n  status: ready\n"));
        assert!(text.contains("checks:\n  - one\n  - two\n"));
        let (code, stdout, stderr) = rendered(
            OutputMode::Text,
            Err(CliError::new(Category::Interrupted, "interrupted", "Observation stopped.").into()),
            false,
        );
        assert_eq!(code, 130);
        assert!(stdout.is_empty());
        assert!(
            std::str::from_utf8(&stderr)
                .unwrap()
                .contains("Observation stopped.")
        );
    }

    #[test]
    fn output_write_failures_are_returned() {
        struct Closed;
        impl Write for Closed {
            fn write(&mut self, _: &[u8]) -> io::Result<usize> {
                Err(io::Error::from(io::ErrorKind::BrokenPipe))
            }
            fn flush(&mut self) -> io::Result<()> {
                Ok(())
            }
        }
        let error = emit_to(
            OutputMode::Json,
            "app status",
            &Ok(Outcome::Data(json!({}))),
            false,
            &mut Closed,
            &mut vec![],
        )
        .unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::BrokenPipe);
    }
}
