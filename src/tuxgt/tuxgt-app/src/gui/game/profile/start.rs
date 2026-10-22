/// Hero body, top to bottom. The profile control is its own row.
pub(crate) const HERO_BODY: [&str; 4] = ["hero-play", "hero-profile", "hero-chips", "hero-title"];

/// How a new profile starts. One is always selected; the dialog opens on Copy.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum NewStart {
    Empty,
    Copy,
}

impl NewStart {
    pub(crate) fn shows_base(self) -> bool {
        matches!(self, NewStart::Copy)
    }
}

pub(crate) fn default_new_start() -> NewStart {
    NewStart::Copy
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profile_row_sits_between_play_and_chips() {
        assert_eq!(
            HERO_BODY,
            ["hero-play", "hero-profile", "hero-chips", "hero-title"]
        );
    }

    #[test]
    fn new_profile_opens_on_copy_and_empty_hides_the_base() {
        assert!(matches!(default_new_start(), NewStart::Copy));
        assert!(!NewStart::Empty.shows_base());
        assert!(NewStart::Copy.shows_base());
    }
}
