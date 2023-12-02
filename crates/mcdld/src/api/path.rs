use serde::Serialize;

use eyre::{Report, WrapErr};

pub fn fmt(path: &str, params: &impl Serialize) -> Result<String, Report> {
    let serialized = serde_json::to_value(params).wrap_err("failed to serialize params")?;

    match serialized {
        serde_json::Value::Object(params) => fmt_map(path, params),
        serde_json::Value::Array(params) => fmt_array(path, params),
        param => fmt_array(path, vec![param]),
    }
}

fn fmt_array(path: &str, params: Vec<serde_json::Value>) -> Result<String, Report> {
    let mut param_iter = params.iter();
    path.split('/')
        .map(|part| {
            if let Some(part) = part.strip_prefix(':') {
                param_iter
                    .next()
                    .map(|value| match value {
                        serde_json::Value::String(s) => s.clone(),
                        v => v.to_string(),
                    })
                    .ok_or_else(|| eyre::eyre!("missing param for part"))
                    // .add_context_debug("part", part)
                    // .add_context_debug("params", &params)
                    // .add_context_debug("path", path)
            } else {
                Ok(part.to_string())
            }
        })
        .collect::<Result<Vec<_>, Report>>()
        .map(|s| s.join("/"))
}

fn fmt_map(
    path: &str,
    params: serde_json::Map<String, serde_json::Value>,
) -> Result<String, Report> {
    path.split('/')
        .map(|part| {
            if let Some(part) = part.strip_prefix(':') {
                params
                    .get(part)
                    .map(|value| match value {
                        serde_json::Value::String(s) => s.clone(),
                        v => v.to_string(),
                    })
                    .ok_or_else(|| eyre::eyre!("missing param for part"))
                    // .add_context_debug("part", part)
                    // .add_context_debug("params", &params)
                    // .add_context_debug("path", path)
            } else {
                Ok(part.to_string())
            }
        })
        .collect::<Result<Vec<_>, Report>>()
        .map(|s| s.join("/"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_params(params: impl Serialize, tests: &[(&str, Result<&str, &str>)]) {
        for (path, want) in tests {
            let got = fmt(path, &params);
            #[allow(clippy::panic)]
            match (got, want) {
                (Ok(got), Ok(want)) => assert_eq!(&got, want, "path: {path}"),
                (Ok(got), Err(want)) => {
                    panic!("expected error got ok, got: {got}, want: {want}, path: {path}")
                }
                (got, Ok(want)) => {
                    got
                    // .add_context("want", want.to_string())
                        .expect("failed to write path");
                }
                (Err(got), Err(want)) => {
                    assert_eq!(&got.to_string(), want, "path: {path}")
                }
            }
        }
    }

    #[test]
    fn test_one_param() {
        #[derive(Serialize)]
        struct Params {
            id: &'static str,
        }

        for params in [
            serde_json::to_value(("p",)).expect("failed to serialize"),
            serde_json::to_value(&Params { id: "p" }).expect("failed to serialize"),
        ] {
            test_params(
                params,
                &[
                    ("/:id", Ok("/p")),
                    ("/:id/b", Ok("/p/b")),
                    ("/a/:id", Ok("/a/p")),
                    ("/a/:id/b", Ok("/a/p/b")),
                    ("/a/:id/b/:name", Err("missing param for part")),
                ],
            );
        }
    }

    #[test]
    fn test_two_params() {
        #[derive(Serialize)]
        struct Params {
            id: &'static str,
            name: &'static str,
        }

        for params in [
            serde_json::to_value(("p", "q")).expect("failed to serialize"),
            serde_json::to_value(&Params { id: "p", name: "q" }).expect("failed to serialize"),
        ] {
            test_params(
                params,
                &[
                    ("/:id/:name", Ok("/p/q")),
                    ("/:id/:name/b", Ok("/p/q/b")),
                    ("/a/:id/b/:name", Ok("/a/p/b/q")),
                    ("/a/:id/b/:name/c", Ok("/a/p/b/q/c")),
                    ("/a/:id/b/:name/c/:omg", Err("missing param for part")),
                ],
            );
        }
    }
}
