//! Converting other terminals' color schemes to `[theme.colors]` (F6-01): iTerm2's
//! `.itermcolors` (a property list) and Alacritty's TOML themes. `hivetty theme import <file>`
//! prints the result for the user to paste into `config.toml`.

use crate::ThemeColors;

/// The 16 ANSI color names in Alacritty's `[colors.normal]` and `[colors.bright]` order.
const ANSI_NAMES: [&str; 8] = [
    "black", "red", "green", "yellow", "blue", "magenta", "cyan", "white",
];

/// Largest theme file accepted.
pub const MAX_BYTES: usize = 1 << 20;

/// `#rrggbb` from 0–1 components.
fn hex_from_unit(r: f64, g: f64, b: f64) -> String {
    let c = |v: f64| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    format!("#{:02x}{:02x}{:02x}", c(r), c(g), c(b))
}

/// The contents of the `<dict>` that follows `<key>name</key>`.
fn plist_dict<'a>(xml: &'a str, name: &str) -> Option<&'a str> {
    let key = format!("<key>{name}</key>");
    let start = xml.find(&key)? + key.len();
    let rest = &xml[start..];
    let open = rest.find("<dict>")?;
    // The dict must come right after the key (only whitespace between).
    if !rest[..open].trim().is_empty() {
        return None;
    }
    let body = &rest[open + "<dict>".len()..];
    let close = body.find("</dict>")?;
    Some(&body[..close])
}

/// The number after `<key>name</key>` inside a color dict (`<real>` or `<integer>`).
fn plist_number(dict: &str, name: &str) -> Option<f64> {
    let key = format!("<key>{name}</key>");
    let rest = &dict[dict.find(&key)? + key.len()..];
    let rest = rest.trim_start();
    let (open, close) = if rest.starts_with("<real>") {
        ("<real>", "</real>")
    } else if rest.starts_with("<integer>") {
        ("<integer>", "</integer>")
    } else {
        return None;
    };
    let value = &rest[open.len()..rest.find(close)?];
    value.trim().parse().ok().filter(|v: &f64| v.is_finite())
}

fn plist_color(xml: &str, name: &str) -> Option<String> {
    let dict = plist_dict(xml, name)?;
    Some(hex_from_unit(
        plist_number(dict, "Red Component")?,
        plist_number(dict, "Green Component")?,
        plist_number(dict, "Blue Component")?,
    ))
}

/// Reads an iTerm2 `.itermcolors` file.
pub fn from_itermcolors(xml: &str) -> Result<ThemeColors, String> {
    let ansi = (0..16)
        .map(|i| {
            plist_color(xml, &format!("Ansi {i} Color"))
                .ok_or_else(|| format!("missing or invalid \"Ansi {i} Color\""))
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(ThemeColors {
        foreground: plist_color(xml, "Foreground Color")
            .ok_or("missing or invalid \"Foreground Color\"")?,
        background: plist_color(xml, "Background Color")
            .ok_or("missing or invalid \"Background Color\"")?,
        cursor: plist_color(xml, "Cursor Color"),
        selection: plist_color(xml, "Selection Color"),
        ansi,
    })
}

/// `#rrggbb` from Alacritty's `"#rrggbb"` or `"0xrrggbb"`.
fn alacritty_hex(value: Option<&toml::Value>) -> Option<String> {
    let text = value?.as_str()?.trim();
    let digits = text
        .strip_prefix('#')
        .or_else(|| text.strip_prefix("0x"))
        .or_else(|| text.strip_prefix("0X"))?;
    (digits.len() == 6 && digits.chars().all(|c| c.is_ascii_hexdigit()))
        .then(|| format!("#{}", digits.to_ascii_lowercase()))
}

/// Reads an Alacritty TOML theme (`[colors.primary]`, `[colors.normal]`, `[colors.bright]`…).
pub fn from_alacritty(text: &str) -> Result<ThemeColors, String> {
    let root: toml::Table = toml::from_str(text).map_err(|e| e.to_string())?;
    let colors = root.get("colors").ok_or("no [colors] section")?;
    let section = |name: &str| colors.get(name);
    let primary = section("primary").ok_or("no [colors.primary] section")?;
    let mut ansi = Vec::with_capacity(16);
    for group in ["normal", "bright"] {
        let table = section(group).ok_or_else(|| format!("no [colors.{group}] section"))?;
        for name in ANSI_NAMES {
            ansi.push(
                alacritty_hex(table.get(name))
                    .ok_or_else(|| format!("missing or invalid colors.{group}.{name}"))?,
            );
        }
    }
    Ok(ThemeColors {
        foreground: alacritty_hex(primary.get("foreground"))
            .ok_or("missing or invalid colors.primary.foreground")?,
        background: alacritty_hex(primary.get("background"))
            .ok_or("missing or invalid colors.primary.background")?,
        cursor: alacritty_hex(section("cursor").and_then(|c| c.get("cursor"))),
        selection: alacritty_hex(section("selection").and_then(|c| c.get("background"))),
        ansi,
    })
}

/// The `[theme.colors]` block for `config.toml`.
pub fn to_toml(colors: &ThemeColors) -> String {
    let mut out = String::from("[theme.colors]\n");
    out.push_str(&format!("foreground = \"{}\"\n", colors.foreground));
    out.push_str(&format!("background = \"{}\"\n", colors.background));
    if let Some(c) = &colors.cursor {
        out.push_str(&format!("cursor = \"{c}\"\n"));
    }
    if let Some(c) = &colors.selection {
        out.push_str(&format!("selection = \"{c}\"\n"));
    }
    let ansi: Vec<String> = colors.ansi.iter().map(|c| format!("\"{c}\"")).collect();
    out.push_str(&format!(
        "ansi = [\n    {},\n    {},\n]\n",
        ansi[..8.min(ansi.len())].join(", "),
        ansi[8.min(ansi.len())..].join(", ")
    ));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plist_entry(name: &str, r: f64, g: f64, b: f64) -> String {
        format!(
            "<key>{name}</key>\n<dict>\n<key>Alpha Component</key>\n<real>1</real>\n\
             <key>Blue Component</key>\n<real>{b}</real>\n<key>Color Space</key>\n\
             <string>sRGB</string>\n<key>Green Component</key>\n<real>{g}</real>\n\
             <key>Red Component</key>\n<integer>{r}</integer>\n</dict>\n"
        )
    }

    #[test]
    fn itermcolors_are_converted() {
        let mut xml = String::from("<?xml version=\"1.0\"?>\n<plist version=\"1.0\">\n<dict>\n");
        for i in 0..16 {
            xml += &plist_entry(&format!("Ansi {i} Color"), 0.0, f64::from(i) / 15.0, 1.0);
        }
        xml += &plist_entry("Background Color", 0.0, 0.0, 0.0);
        xml += &plist_entry("Foreground Color", 1.0, 1.0, 1.0);
        xml += "</dict>\n</plist>\n";
        let colors = from_itermcolors(&xml).unwrap();
        assert_eq!(colors.background, "#000000");
        assert_eq!(colors.foreground, "#ffffff");
        assert_eq!(colors.ansi[15], "#00ffff");
        assert_eq!(colors.cursor, None);
        // The output is valid config.
        let config: crate::Config =
            toml::from_str(&to_toml(&colors)).expect("the printed block parses");
        assert_eq!(config.theme.colors.unwrap().ansi.len(), 16);
        assert!(from_itermcolors("<plist/>").is_err());
    }

    #[test]
    fn alacritty_themes_are_converted() {
        let mut text = String::from(
            "[colors.primary]\nbackground = '#1d1f21'\nforeground = '0xC5C8C6'\n\
             [colors.cursor]\ncursor = '#ffffff'\n",
        );
        for group in ["normal", "bright"] {
            text += &format!("[colors.{group}]\n");
            for (i, name) in ANSI_NAMES.iter().enumerate() {
                text += &format!("{name} = '#00000{i}'\n");
            }
        }
        let colors = from_alacritty(&text).unwrap();
        assert_eq!(colors.foreground, "#c5c8c6");
        assert_eq!(colors.cursor.as_deref(), Some("#ffffff"));
        assert_eq!(colors.ansi[9], "#000001");
        assert!(from_alacritty("[colors.primary]\nbackground = 'red'").is_err());
    }
}
