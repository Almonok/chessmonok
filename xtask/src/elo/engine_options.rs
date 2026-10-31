//! UCI options passed to the engines, e.g. `--engine-option Hash=64`.

use std::path::Path;

use anyhow::{Result, bail};

/// One UCI option and the value to give it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EngineOption {
    pub name: String,
    pub value: String,
}

/// Parse a command-line `NAME=VALUE`, splitting at the first `=`.
///
/// A value naming an existing file (a net) is made absolute: `cargo xtask`
/// may run from any subdirectory, while fastchess always runs from the
/// repository root, so a relative path would point somewhere else there.
pub fn parse_engine_option(text: &str) -> Result<EngineOption> {
    let Some((name, value)) = text.split_once('=').filter(|(name, _)| !name.is_empty()) else {
        bail!("expected NAME=VALUE, got `{text}`");
    };
    Ok(EngineOption {
        name: name.to_string(),
        value: absolute_if_file(value)?,
    })
}

fn absolute_if_file(value: &str) -> Result<String> {
    let path = Path::new(value);
    if path.is_file() {
        Ok(std::path::absolute(path)?.to_string_lossy().into_owned())
    } else {
        Ok(value.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_on_the_first_equals_sign() {
        let option = parse_engine_option("A=b=c").unwrap();
        assert_eq!((option.name.as_str(), option.value.as_str()), ("A", "b=c"));
    }

    #[test]
    fn rejects_text_without_an_equals_sign() {
        assert!(parse_engine_option("Hash").is_err());
    }

    #[test]
    fn plain_values_are_kept() {
        assert_eq!(parse_engine_option("Hash=16").unwrap().value, "16");
    }

    #[test]
    fn relative_file_values_become_absolute() {
        // Tests run in xtask/, where Cargo.toml exists.
        let value = parse_engine_option("EvalFile=Cargo.toml").unwrap().value;
        assert!(Path::new(&value).is_absolute(), "{value}");
        assert!(value.ends_with("xtask/Cargo.toml"), "{value}");
    }
}
