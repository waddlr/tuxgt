use std::path::Path;

use super::*;

pub(crate) async fn run(
    pool: &SqlitePool,
    dir: &Path,
    id: String,
    set: Vec<String>,
    unset: Vec<String>,
    force: bool,
    yes: bool,
) -> CliResult {
    let host = PluginHost::load()?;
    let report = mutate_game(pool, dir, &host, &id, async {
        for (field, value) in override_ops(&set, &unset, force)? {
            set_override(pool, &id, &field, value.as_deref()).await?;
        }
        doctor(pool, &id, DetectOpts { force, yes }).await
    })
    .await?;
    for f in report.fields {
        if let Some(src) = f.source {
            println!("{}\t{}\t{src}", f.key, f.value);
        } else {
            println!("{}\t{}", f.key, f.value);
        }
    }
    tracing::info!(game = id.as_str(), "doctor done");
    Ok(())
}

/// Parse `doctor --set`/`--unset` into `(field, value)` ops in application
/// order: every `--set` then every `--unset`, so an `--unset` wins a conflict.
/// `None` value clears the override; empty `--set` value (`field=`) means `None`.
/// Every op (field name and value) is validated here, before the caller writes
/// anything, so one bad op fails the whole argv with zero writes.
pub(crate) fn override_ops(
    set: &[String],
    unset: &[String],
    force: bool,
) -> Result<Vec<(String, Option<String>)>, Error> {
    if force && !(set.is_empty() && unset.is_empty()) {
        return Err(Error::InvalidOverride(
            "--set/--unset cannot be combined with --force (--force clears every override)".into(),
        ));
    }
    let mut ops = Vec::with_capacity(set.len() + unset.len());
    for arg in set {
        let (field, value) = arg
            .split_once('=')
            .ok_or_else(|| Error::InvalidOverride(format!("{arg} (expected FIELD=VALUE)")))?;
        let value = (!value.is_empty()).then(|| value.to_string());
        validate_override(field, value.as_deref())?;
        ops.push((field.to_string(), value));
    }
    for field in unset {
        validate_override(field, None)?;
        ops.push((field.clone(), None));
    }
    Ok(ops)
}

#[cfg(test)]
mod tests {
    use super::override_ops;
    use tuxgt_core::Error;

    fn args(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn override_ops_set_value_and_empty_value_clears() {
        assert_eq!(
            override_ops(&args(&["api=dx11"]), &args(&[]), false).unwrap(),
            vec![("api".to_string(), Some("dx11".to_string()))]
        );
        assert_eq!(
            override_ops(&args(&["api="]), &args(&[]), false).unwrap(),
            vec![("api".to_string(), None)]
        );
        // `=` only splits once: values may contain `=`.
        assert_eq!(
            override_ops(&args(&["build=a=b"]), &args(&[]), false).unwrap(),
            vec![("build".to_string(), Some("a=b".to_string()))]
        );
    }

    #[test]
    fn override_ops_unset_wins_and_repeatable() {
        assert_eq!(
            override_ops(&args(&["api=dx11"]), &args(&["api", "platform"]), false).unwrap(),
            vec![
                ("api".to_string(), Some("dx11".to_string())),
                ("api".to_string(), None),
                ("platform".to_string(), None),
            ]
        );
    }

    #[test]
    fn override_ops_rejects_unknown_field_and_missing_equals() {
        for bad in ["nope=1", "api"] {
            let err = override_ops(&args(&[bad]), &args(&[]), false).unwrap_err();
            assert!(matches!(err, Error::InvalidOverride(_)), "{err}");
            if bad.starts_with("nope") {
                assert!(err.to_string().contains("nope"), "{err}");
            }
        }
        let err = override_ops(&args(&[]), &args(&["nope"]), false).unwrap_err();
        assert!(err.to_string().contains("nope"), "{err}");
    }

    #[test]
    fn override_ops_validates_values_before_any_write() {
        // First op valid, second invalid: the whole argv must fail.
        let err = override_ops(&args(&["api=dx11", "bitness=7"]), &args(&[]), false).unwrap_err();
        assert!(err.to_string().contains("bad bitness: 7"), "{err}");
        let err =
            override_ops(&args(&["api=dx11", "platform=mac"]), &args(&[]), false).unwrap_err();
        assert!(err.to_string().contains("bad platform: mac"), "{err}");
        // Clears are always valid: no value rule applies.
        assert!(override_ops(&args(&["bitness="]), &args(&["platform"]), false).is_ok());
    }

    #[test]
    fn override_ops_force_conflict_errors() {
        let err = override_ops(&args(&["api=dx11"]), &args(&[]), true).unwrap_err();
        assert!(matches!(err, Error::InvalidOverride(_)), "{err}");
        assert!(err.to_string().contains("--force"), "{err}");
        let err = override_ops(&args(&[]), &args(&["api"]), true).unwrap_err();
        assert!(err.to_string().contains("--force"), "{err}");
        assert!(override_ops(&args(&[]), &args(&[]), true)
            .unwrap()
            .is_empty());
    }
}
