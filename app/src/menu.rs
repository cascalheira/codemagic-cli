//! The native menu bar (macOS menu bar, Windows window menu).
//!
//! Dioxus ships a minimal default — a "Window" submenu holding Quit and an
//! "Edit" submenu — which reads as obviously wrong on macOS, where Quit
//! belongs in the application menu. This replaces it with the conventional
//! layout for each platform.
//!
//! Menu items map onto the same [`Shortcut`] enum the keyboard bindings use,
//! so both routes end up in one handler in `BuildsScreen`. Items that aren't
//! app actions (opening a web page) come back as [`MenuCommand::OpenUrl`].

use dioxus::desktop::muda::{
    AboutMetadata, Menu, MenuItem, PredefinedMenuItem, Submenu, accelerator::Accelerator,
};

use crate::components::shortcuts::Shortcut;

const REPOSITORY: &str = "https://github.com/cascalheira/codemagic-cli";
const ISSUES: &str = "https://github.com/cascalheira/codemagic-cli/issues";

/// What a menu item does when clicked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MenuCommand {
    /// The same action a keyboard shortcut would trigger.
    Action(Shortcut),
    /// Open a page in the user's browser.
    OpenUrl(&'static str),
}

/// Resolves a muda item id to its command.
///
/// Ids that aren't ours (Dioxus registers its own dev-tools items in debug
/// builds) return `None` and are left alone.
pub fn command(id: &str) -> Option<MenuCommand> {
    Some(match id {
        "gantry-new-build" => MenuCommand::Action(Shortcut::NewBuild),
        "gantry-settings" => MenuCommand::Action(Shortcut::Settings),
        "gantry-refresh" => MenuCommand::Action(Shortcut::Refresh),
        "gantry-filter" => MenuCommand::Action(Shortcut::FocusFilter),
        "gantry-app-info" => MenuCommand::Action(Shortcut::AppInfo),
        "gantry-open-build" => MenuCommand::Action(Shortcut::OpenInBrowser),
        "gantry-shortcuts" => MenuCommand::Action(Shortcut::Help),
        "gantry-repository" => MenuCommand::OpenUrl(REPOSITORY),
        "gantry-issues" => MenuCommand::OpenUrl(ISSUES),
        _ => return None,
    })
}

/// Runs `handler` for every menu item the user picks.
pub fn use_menu_commands(mut handler: impl FnMut(MenuCommand) + 'static) {
    dioxus::desktop::use_muda_event_handler(move |event| {
        if let Some(command) = command(event.id().0.as_str()) {
            handler(command);
        }
    });
}

/// Every accelerator the menu bar uses. Named so [`build`] and the test that
/// proves they all parse can't drift apart.
const SETTINGS_KEY: &str = "CmdOrCtrl+,";
const NEW_BUILD_KEY: &str = "CmdOrCtrl+N";
const REFRESH_KEY: &str = "CmdOrCtrl+R";
const FILTER_KEY: &str = "CmdOrCtrl+F";
const OPEN_BUILD_KEY: &str = "CmdOrCtrl+Shift+O";
const APP_INFO_KEY: &str = "CmdOrCtrl+I";
const SHORTCUTS_KEY: &str = "CmdOrCtrl+/";
#[cfg_attr(not(test), allow(dead_code))]
const ALL_KEYS: &[&str] = &[
    SETTINGS_KEY, NEW_BUILD_KEY, REFRESH_KEY, FILTER_KEY,
    OPEN_BUILD_KEY, APP_INFO_KEY, SHORTCUTS_KEY,
];

/// A menu item with an id and an optional accelerator.
///
/// Accelerators are parsed rather than built by hand; an unparseable one is
/// dropped so a typo costs the shortcut, not the whole menu bar.
fn item(id: &str, label: &str, accelerator: Option<&str>) -> MenuItem {
    MenuItem::with_id(id, label, true, accelerator.and_then(|a| a.parse::<Accelerator>().ok()))
}

fn about_metadata() -> AboutMetadata {
    AboutMetadata {
        name: Some("Gantry".into()),
        version: Some(env!("CARGO_PKG_VERSION").into()),
        website: Some(REPOSITORY.into()),
        comments: Some("An unofficial client for Codemagic CI/CD.".into()),
        ..Default::default()
    }
}

/// Builds the platform-appropriate menu bar.
pub fn build() -> Menu {
    let menu = Menu::new();

    // macOS puts app-level items (About, Settings, Quit) in a leading
    // submenu whose title the OS replaces with the bundle name. Windows and
    // Linux have no such menu, so those items live under File instead.
    #[cfg(target_os = "macos")]
    {
        let app_menu = Submenu::new("Gantry", true);
        let _ = app_menu.append_items(&[
            &PredefinedMenuItem::about(Some("About Gantry"), Some(about_metadata())),
            &PredefinedMenuItem::separator(),
            &item("gantry-settings", "Settings…", Some(SETTINGS_KEY)),
            &PredefinedMenuItem::separator(),
            &PredefinedMenuItem::services(None),
            &PredefinedMenuItem::separator(),
            &PredefinedMenuItem::hide(None),
            &PredefinedMenuItem::hide_others(None),
            &PredefinedMenuItem::show_all(None),
            &PredefinedMenuItem::separator(),
            &PredefinedMenuItem::quit(None),
        ]);
        let _ = menu.append(&app_menu);
    }

    let file_menu = Submenu::new("File", true);
    let _ = file_menu.append(&item("gantry-new-build", "New Build…", Some(NEW_BUILD_KEY)));
    #[cfg(not(target_os = "macos"))]
    {
        let _ = file_menu.append_items(&[
            &PredefinedMenuItem::separator(),
            &item("gantry-settings", "Settings", Some(SETTINGS_KEY)),
        ]);
    }
    let _ = file_menu.append_items(&[
        &PredefinedMenuItem::separator(),
        &PredefinedMenuItem::close_window(None),
    ]);
    #[cfg(not(target_os = "macos"))]
    {
        let _ = file_menu.append(&PredefinedMenuItem::quit(Some("Exit")));
    }

    // Cut/copy/paste are more than decoration on macOS: without these items
    // the standard editing shortcuts don't reach the webview at all.
    let edit_menu = Submenu::new("Edit", true);
    let _ = edit_menu.append_items(&[
        &PredefinedMenuItem::undo(None),
        &PredefinedMenuItem::redo(None),
        &PredefinedMenuItem::separator(),
        &PredefinedMenuItem::cut(None),
        &PredefinedMenuItem::copy(None),
        &PredefinedMenuItem::paste(None),
        &PredefinedMenuItem::separator(),
        &PredefinedMenuItem::select_all(None),
    ]);

    let view_menu = Submenu::new("View", true);
    let _ = view_menu.append_items(&[
        &item("gantry-refresh", "Refresh", Some(REFRESH_KEY)),
        &item("gantry-filter", "Filter by Workflow", Some(FILTER_KEY)),
        &PredefinedMenuItem::separator(),
        &item("gantry-open-build", "Open Build in Codemagic", Some(OPEN_BUILD_KEY)),
        // No "&": muda reads it as a Windows mnemonic marker and strips it,
        // leaving a double space in the label.
        &item("gantry-app-info", "App and Workflow IDs", Some(APP_INFO_KEY)),
        &PredefinedMenuItem::separator(),
        &PredefinedMenuItem::fullscreen(None),
    ]);

    let window_menu = Submenu::new("Window", true);
    let _ = window_menu.append_items(&[
        &PredefinedMenuItem::minimize(None),
        &PredefinedMenuItem::maximize(None),
    ]);
    #[cfg(target_os = "macos")]
    {
        let _ = window_menu.append_items(&[
            &PredefinedMenuItem::separator(),
            &PredefinedMenuItem::bring_all_to_front(None),
        ]);
    }

    let help_menu = Submenu::new("Help", true);
    let _ = help_menu.append_items(&[
        &item("gantry-shortcuts", "Keyboard Shortcuts", Some(SHORTCUTS_KEY)),
        &PredefinedMenuItem::separator(),
        &item("gantry-repository", "Gantry on GitHub", None),
        &item("gantry-issues", "Report an Issue", None),
    ]);
    // Windows and Linux have no application menu to hold About.
    #[cfg(not(target_os = "macos"))]
    {
        let _ = help_menu.append_items(&[
            &PredefinedMenuItem::separator(),
            &PredefinedMenuItem::about(Some("About Gantry"), Some(about_metadata())),
        ]);
    }

    let _ = menu.append_items(&[&file_menu, &edit_menu, &view_menu, &window_menu, &help_menu]);

    // Tells macOS which submenus get its automatic behaviour: the window list
    // appended to Window, and the searchable help field in Help.
    #[cfg(target_os = "macos")]
    {
        window_menu.set_as_windows_menu_for_nsapp();
        help_menu.set_as_help_menu_for_nsapp();
    }

    menu
}

#[cfg(test)]
mod tests {
    use super::{MenuCommand, command};
    use crate::components::shortcuts::Shortcut;

    #[test]
    fn known_ids_map_to_their_action() {
        assert_eq!(
            command("gantry-new-build"),
            Some(MenuCommand::Action(Shortcut::NewBuild))
        );
        assert_eq!(
            command("gantry-app-info"),
            Some(MenuCommand::Action(Shortcut::AppInfo))
        );
    }

    #[test]
    fn help_items_open_urls() {
        assert!(matches!(
            command("gantry-repository"),
            Some(MenuCommand::OpenUrl(_))
        ));
    }

    /// `item` drops an unparseable accelerator with `.ok()`, so a typo would
    /// silently cost a shortcut. This is the guard against that.
    #[test]
    fn every_accelerator_parses() {
        use super::ALL_KEYS;
        use dioxus::desktop::muda::accelerator::Accelerator;
        for key in ALL_KEYS {
            assert!(key.parse::<Accelerator>().is_ok(), "{key} doesn't parse");
        }
    }

    /// Dioxus registers its own items in debug builds; we must ignore them.
    #[test]
    fn foreign_ids_are_ignored() {
        assert_eq!(command("dioxus-toggle-dev-tools"), None);
        assert_eq!(command(""), None);
    }
}
