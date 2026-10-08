//! Embedded build instant, rendered in the user's offset.
//!
//! `build.rs` embeds the UTC instant (`VERGEN_BUILD_TIMESTAMP`, the
//! `SOURCE_DATE_EPOCH` instant for releases); display converts to local.

/// `YYYY-MM-DD` in the user's offset, or `"unknown"` when missing/unparseable.
pub(crate) fn format_build_instant(raw: Option<&str>) -> String {
    let Some(raw) = raw else {
        return "unknown".into();
    };
    let parsed = time::OffsetDateTime::parse(raw, &time::format_description::well_known::Rfc3339);
    let Ok(dt) = parsed else {
        return "unknown".into();
    };
    let offset = time::UtcOffset::local_offset_at(dt).unwrap_or(time::UtcOffset::UTC);
    let local = dt.to_offset(offset);
    format!(
        "{:04}-{:02}-{:02}",
        local.year(),
        local.month() as u8,
        local.day()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_is_unknown() {
        assert_eq!(format_build_instant(None), "unknown");
    }

    #[test]
    fn garbage_is_unknown() {
        assert_eq!(format_build_instant(Some("")), "unknown");
        assert_eq!(format_build_instant(Some("not-a-date")), "unknown");
        // Date-only is not RFC3339.
        assert_eq!(format_build_instant(Some("2022-12-23")), "unknown");
    }

    #[test]
    fn valid_renders_local_date() {
        // Mid-year noon UTC: every real zone renders June 2022 (day 15 or 16).
        let got = format_build_instant(Some("2022-06-15T12:00:00Z"));
        assert!(
            got == "2022-06-15" || got == "2022-06-16",
            "unexpected date: {got}"
        );
    }
}
