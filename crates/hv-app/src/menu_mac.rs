//! The macOS menu bar (F2-09), built with `muda` from the same actions and shortcuts as the
//! keymap, so a remapped shortcut shows up in the menu too.

use std::sync::Mutex;

use muda::accelerator::Accelerator;
use muda::{Menu, MenuEvent, MenuItem, PredefinedMenuItem, Submenu};
use winit::event_loop::EventLoopProxy;

use crate::app::AppEvent;
use crate::keymap::{Action, Chord, Keymap};

/// Builds the menu bar, installs it for the app and forwards clicks as [`AppEvent::Menu`].
/// Keep the returned menu alive for as long as it should be shown.
pub(crate) fn install(keymap: &Keymap, proxy: EventLoopProxy<AppEvent>) -> Option<Menu> {
    let proxy = Mutex::new(proxy);
    MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
        if let Ok(proxy) = proxy.lock() {
            let _ = proxy.send_event(AppEvent::Menu(event.id.0));
        }
    }));
    match build(keymap) {
        Ok(menu) => {
            menu.init_for_nsapp();
            Some(menu)
        }
        Err(err) => {
            log::warn!("cannot build the menu bar: {err}");
            None
        }
    }
}

fn build(keymap: &Keymap) -> muda::Result<Menu> {
    use hv_ui::strings;
    let item = |name: &str| -> MenuItem {
        let label = strings::action_label(name).unwrap_or(name);
        let accelerator = Action::parse(name)
            .and_then(|action| keymap.chord_for(&action).cloned())
            .and_then(|chord| accelerator(&chord));
        // The id is the action name; the click handler parses it back.
        MenuItem::with_id(name, label, true, accelerator)
    };
    let app = Submenu::with_items(
        "hivetty",
        true,
        &[
            &PredefinedMenuItem::about(None, None),
            &PredefinedMenuItem::separator(),
            &PredefinedMenuItem::services(None),
            &PredefinedMenuItem::separator(),
            &PredefinedMenuItem::hide(None),
            &PredefinedMenuItem::hide_others(None),
            &PredefinedMenuItem::show_all(None),
            &PredefinedMenuItem::separator(),
            &PredefinedMenuItem::quit(None),
        ],
    )?;
    let shell = Submenu::with_items(
        strings::MENU_SHELL,
        true,
        &[
            &item("new_tab"),
            &item("new_window"),
            &item("split_right"),
            &item("split_down"),
            &PredefinedMenuItem::separator(),
            &item("close_pane"),
        ],
    )?;
    let edit = Submenu::with_items(
        strings::MENU_EDIT,
        true,
        &[
            &item("copy"),
            &item("paste"),
            &PredefinedMenuItem::separator(),
            &item("find"),
        ],
    )?;
    let view = Submenu::with_items(
        strings::MENU_VIEW,
        true,
        &[
            &item("toggle_sidebar"),
            &item("toggle_zoom"),
            &PredefinedMenuItem::separator(),
            &item("font_bigger"),
            &item("font_smaller"),
            &item("font_reset"),
            &PredefinedMenuItem::separator(),
            &item("command_palette"),
        ],
    )?;
    let window = Submenu::with_items(
        strings::MENU_WINDOW,
        true,
        &[
            &PredefinedMenuItem::minimize(None),
            &PredefinedMenuItem::fullscreen(None),
            &PredefinedMenuItem::separator(),
            &item("next_tab"),
            &item("previous_tab"),
            &item("move_tab_to_new_window"),
        ],
    )?;
    Menu::with_items(&[&app, &shell, &edit, &view, &window])
}

/// The muda accelerator for one of our chords, if muda can express it.
fn accelerator(chord: &Chord) -> Option<Accelerator> {
    let key = match chord.key.as_str() {
        "left" => "ArrowLeft",
        "right" => "ArrowRight",
        "up" => "ArrowUp",
        "down" => "ArrowDown",
        "enter" => "Enter",
        "tab" => "Tab",
        "pageup" => "PageUp",
        "pagedown" => "PageDown",
        "=" => "Equal",
        "-" => "Minus",
        "[" => "BracketLeft",
        "]" => "BracketRight",
        other => other,
    };
    let mut text = String::new();
    for (on, name) in [
        (chord.ctrl, "ctrl+"),
        (chord.alt, "alt+"),
        (chord.shift, "shift+"),
        (chord.super_key, "cmd+"),
    ] {
        if on {
            text.push_str(name);
        }
    }
    text.push_str(key);
    text.parse().ok()
}
