use skia_safe::Color;

#[derive(Clone, Copy)]
pub struct Theme {
    pub bg: Color,
    pub card: Color,
    pub card_hi: Color,
    pub line: Color,
    pub fg: Color,
    pub fg2: Color,
    pub fg3: Color,
    pub fg4: Color,
    pub accent: Color,
    pub up: Color,
    pub down: Color,
}

impl Default for Theme {
    fn default() -> Self {
        Self {
            bg: Color::from_rgb(0x14, 0x15, 0x19),
            card: Color::from_rgb(0x1a, 0x1c, 0x22),
            card_hi: Color::from_rgb(0x20, 0x23, 0x2b),
            line: Color::from_rgb(0x2a, 0x2c, 0x34),
            fg: Color::from_rgb(0xd6, 0xd7, 0xdc),
            fg2: Color::from_rgb(0xb4, 0xb5, 0xbd),
            fg3: Color::from_rgb(0x8b, 0x8c, 0x96),
            fg4: Color::from_rgb(0x52, 0x53, 0x5c),
            accent: Color::from_rgb(0xd8, 0xbb, 0x85),
            up: Color::from_rgb(0xef, 0x6f, 0x6f),
            down: Color::from_rgb(0x5f, 0xc9, 0x8d),
        }
    }
}

pub const CARD_BORDER: Color = Color::from_argb(0x0f, 0xff, 0xff, 0xff);
