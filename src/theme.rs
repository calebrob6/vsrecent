//! Light and dark palettes shared by the platform front ends.

/// A color packed as `0x00BBGGRR`, which is also the layout of a Win32 `COLORREF`.
pub type Color = u32;

pub const fn rgb(red: u8, green: u8, blue: u8) -> Color {
    red as u32 | ((green as u32) << 8) | ((blue as u32) << 16)
}

#[derive(Clone, Copy)]
pub struct Theme {
    pub dark: bool,
    /// The macOS front end keeps the system window background.
    #[cfg_attr(not(windows), allow(dead_code))]
    pub window: Color,
    pub control: Color,
    pub text: Color,
    pub muted: Color,
}

impl Theme {
    pub fn light() -> Self {
        Self {
            dark: false,
            window: rgb(245, 245, 245),
            control: rgb(255, 255, 255),
            text: rgb(25, 25, 28),
            muted: rgb(96, 96, 96),
        }
    }

    pub fn dark() -> Self {
        Self {
            dark: true,
            window: rgb(30, 30, 30),
            control: rgb(37, 37, 38),
            text: rgb(232, 232, 232),
            muted: rgb(165, 165, 165),
        }
    }
}

/// The theme forced with `--dark` or `--light`, if any.
pub fn requested_theme(arguments: &[String]) -> Option<Theme> {
    if arguments
        .iter()
        .any(|argument| argument.eq_ignore_ascii_case("--dark"))
    {
        Some(Theme::dark())
    } else if arguments
        .iter()
        .any(|argument| argument.eq_ignore_ascii_case("--light"))
    {
        Some(Theme::light())
    } else {
        None
    }
}

/// The normal and selected background colors for a row with the given remote display text.
pub fn row_colors(remote: &str, dark: bool) -> (Color, Color) {
    let remote = remote.to_ascii_lowercase();
    if dark && remote == "local" {
        (rgb(38, 40, 45), rgb(79, 86, 99))
    } else if dark && remote.starts_with("wsl") {
        (rgb(57, 37, 31), rgb(194, 70, 25))
    } else if dark && remote.starts_with("ssh") {
        (rgb(31, 44, 61), rgb(31, 92, 173))
    } else if dark && remote.contains("container") {
        (rgb(29, 48, 56), rgb(25, 119, 177))
    } else if dark && (remote == "codespace" || remote == "github") {
        (rgb(45, 36, 61), rgb(103, 63, 168))
    } else if dark && remote == "tunnel" {
        (rgb(28, 52, 49), rgb(0, 116, 104))
    } else if dark {
        let hash = remote.bytes().fold(2_166_136_261u32, |value, byte| {
            (value ^ byte as u32).wrapping_mul(16_777_619)
        });
        let palette = [
            (rgb(58, 43, 27), rgb(166, 91, 0)),
            (rgb(31, 53, 37), rgb(42, 120, 63)),
            (rgb(55, 34, 42), rgb(153, 60, 91)),
            (rgb(36, 40, 61), rgb(67, 79, 164)),
        ];
        palette[hash as usize % palette.len()]
    } else if remote == "local" {
        (rgb(239, 241, 245), rgb(79, 86, 99))
    } else if remote.starts_with("wsl") {
        (rgb(255, 238, 229), rgb(194, 70, 25))
    } else if remote.starts_with("ssh") {
        (rgb(232, 241, 255), rgb(31, 92, 173))
    } else if remote.contains("container") {
        (rgb(228, 246, 255), rgb(25, 119, 177))
    } else if remote == "codespace" || remote == "github" {
        (rgb(241, 234, 255), rgb(103, 63, 168))
    } else if remote == "tunnel" {
        (rgb(227, 247, 242), rgb(0, 116, 104))
    } else {
        let hash = remote.bytes().fold(2_166_136_261u32, |value, byte| {
            (value ^ byte as u32).wrapping_mul(16_777_619)
        });
        let palette = [
            (rgb(255, 241, 223), rgb(166, 91, 0)),
            (rgb(232, 246, 235), rgb(42, 120, 63)),
            (rgb(250, 233, 239), rgb(153, 60, 91)),
            (rgb(235, 239, 255), rgb(67, 79, 164)),
        ];
        palette[hash as usize % palette.len()]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn assigns_distinct_remote_colors() {
        assert_ne!(row_colors("LOCAL", false), row_colors("WSL: Ubuntu", false));
        assert_ne!(
            row_colors("WSL: Ubuntu", false),
            row_colors("SSH: host", false)
        );
        assert_ne!(
            row_colors("SSH: host", false),
            row_colors("DEV CONTAINER", false)
        );
        assert_ne!(row_colors("LOCAL", false), row_colors("LOCAL", true));
    }

    #[test]
    fn packs_colors_as_blue_green_red() {
        assert_eq!(rgb(0x11, 0x22, 0x33), 0x0033_2211);
    }
}
