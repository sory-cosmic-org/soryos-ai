//! SoryOS custom dark theme colors.
//!
//! Colors derived from the reference design image.

use cosmic::iced::widget::container;
use cosmic::iced::{Background, Border, Color, Shadow};
use cosmic::theme::Theme;

// ── Background Colors ──

pub const BG_DARKEST: Color = Color {
    r: 0.051,
    g: 0.067,
    b: 0.090,
    a: 1.0,
};
pub const BG_SIDEBAR: Color = Color {
    r: 0.063,
    g: 0.075,
    b: 0.098,
    a: 1.0,
};
pub const BG_CARD: Color = Color {
    r: 0.086,
    g: 0.102,
    b: 0.133,
    a: 1.0,
};
pub const BG_ELEVATED: Color = Color {
    r: 0.094,
    g: 0.110,
    b: 0.145,
    a: 1.0,
};
pub const BG_HOVER: Color = Color {
    r: 0.106,
    g: 0.122,
    b: 0.161,
    a: 1.0,
};
pub const BG_ACTIVE: Color = Color {
    r: 0.165,
    g: 0.173,
    b: 0.290,
    a: 1.0,
};

// ── Accent Colors ──

pub const ACCENT: Color = Color {
    r: 0.424,
    g: 0.361,
    b: 0.906,
    a: 1.0,
};
pub const ACCENT_LIGHT: Color = Color {
    r: 0.549,
    g: 0.463,
    b: 0.969,
    a: 1.0,
};
pub const SUCCESS: Color = Color {
    r: 0.247,
    g: 0.725,
    b: 0.314,
    a: 1.0,
};
pub const STATUS_GREEN: Color = Color {
    r: 0.184,
    g: 0.808,
    b: 0.443,
    a: 1.0,
};

// ── Text Colors ──

pub const TEXT_PRIMARY: Color = Color {
    r: 0.902,
    g: 0.925,
    b: 0.953,
    a: 1.0,
};
pub const TEXT_SECONDARY: Color = Color {
    r: 0.490,
    g: 0.522,
    b: 0.565,
    a: 1.0,
};
pub const TEXT_TERTIARY: Color = Color {
    r: 0.369,
    g: 0.404,
    b: 0.455,
    a: 1.0,
};

// ── Border Colors ──

pub const BORDER: Color = Color {
    r: 0.165,
    g: 0.184,
    b: 0.224,
    a: 1.0,
};
pub const DIVIDER: Color = Color {
    r: 0.133,
    g: 0.153,
    b: 0.192,
    a: 1.0,
};

// ── Radii ──

pub const RADIUS_SM: f32 = 6.0;
pub const RADIUS_MD: f32 = 10.0;
pub const RADIUS_LG: f32 = 14.0;
pub const RADIUS_XL: f32 = 20.0;
pub const RADIUS_FULL: f32 = 999.0;

// ── Dimensions ──

pub const SIDEBAR_WIDTH: f32 = 260.0;
pub const RIGHT_PANEL_WIDTH: f32 = 300.0;
pub const TOP_BAR_HEIGHT: f32 = 52.0;
pub const INPUT_BAR_HEIGHT: f32 = 56.0;
pub const STATUS_BAR_HEIGHT: f32 = 28.0;

// ── Text Style Helper ──
//
// libcosmic's iced text widget uses `.class(color)` for text colors
// (`From<Color> for cosmic::theme::Text`), not `.style()`.

// ── Container Styles ──

pub fn soryos_background(_theme: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(BG_DARKEST)),
        text_color: Some(TEXT_PRIMARY),
        border: Border::default(),
        ..Default::default()
    }
}

pub fn sidebar_bg(_theme: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(BG_SIDEBAR)),
        text_color: Some(TEXT_PRIMARY),
        border: Border {
            color: BORDER,
            width: 1.0,
            ..Default::default()
        },
        ..Default::default()
    }
}

pub fn right_panel_bg(_theme: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(BG_SIDEBAR)),
        text_color: Some(TEXT_PRIMARY),
        border: Border {
            color: BORDER,
            width: 1.0,
            ..Default::default()
        },
        ..Default::default()
    }
}

pub fn card_bg(_theme: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(BG_CARD)),
        text_color: Some(TEXT_PRIMARY),
        border: Border {
            color: BORDER,
            width: 1.0,
            radius: RADIUS_MD.into(),
            ..Default::default()
        },
        ..Default::default()
    }
}

pub fn elevated_bg(_theme: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(BG_ELEVATED)),
        text_color: Some(TEXT_PRIMARY),
        border: Border {
            color: BORDER,
            width: 1.0,
            radius: RADIUS_LG.into(),
            ..Default::default()
        },
        ..Default::default()
    }
}

pub fn active_item_bg(_theme: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(BG_ACTIVE)),
        text_color: Some(TEXT_PRIMARY),
        border: Border {
            radius: RADIUS_MD.into(),
            ..Default::default()
        },
        ..Default::default()
    }
}

pub fn model_card_highlighted(_theme: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(BG_CARD)),
        text_color: Some(TEXT_PRIMARY),
        border: Border {
            color: ACCENT,
            width: 2.0,
            radius: RADIUS_MD.into(),
            ..Default::default()
        },
        shadow: Shadow {
            color: Color {
                r: 0.424,
                g: 0.361,
                b: 0.906,
                a: 0.15,
            },
            offset: cosmic::iced::Vector::new(0.0, 2.0),
            blur_radius: 12.0,
        },
        ..Default::default()
    }
}

pub fn transparent(_theme: &Theme) -> container::Style {
    container::Style {
        background: None,
        text_color: Some(TEXT_PRIMARY),
        border: Border::default(),
        ..Default::default()
    }
}

pub fn search_bar(_theme: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(BG_ELEVATED)),
        text_color: Some(TEXT_SECONDARY),
        border: Border {
            color: BORDER,
            width: 1.0,
            radius: RADIUS_FULL.into(),
            ..Default::default()
        },
        ..Default::default()
    }
}

pub fn user_area(_theme: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(BG_CARD)),
        text_color: Some(TEXT_PRIMARY),
        border: Border {
            radius: RADIUS_MD.into(),
            ..Default::default()
        },
        ..Default::default()
    }
}

pub fn input_bar_bg(_theme: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(BG_ELEVATED)),
        text_color: Some(TEXT_PRIMARY),
        border: Border {
            color: BORDER,
            width: 1.0,
            radius: RADIUS_LG.into(),
            ..Default::default()
        },
        ..Default::default()
    }
}

pub fn status_bar_bg(_theme: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(BG_SIDEBAR)),
        text_color: Some(TEXT_SECONDARY),
        border: Border {
            color: BORDER,
            width: 1.0,
            ..Default::default()
        },
        ..Default::default()
    }
}
