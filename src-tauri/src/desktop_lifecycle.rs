use std::sync::atomic::{AtomicBool, Ordering};

use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{App, AppHandle, Manager, Window, WindowEvent};

const MAIN_WINDOW_LABEL: &str = "main";
const SHOW_MENU_ID: &str = "show-main-window";
const QUIT_MENU_ID: &str = "quit-application";
const TRAY_ID: &str = "coding-tools-mcp";

static TRAY_READY: AtomicBool = AtomicBool::new(false);
static EXPLICIT_EXIT_REQUESTED: AtomicBool = AtomicBool::new(false);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CloseDecision {
    HideToTray,
    AllowClose,
}

fn close_decision(
    is_main_window: bool,
    tray_ready: bool,
    explicit_exit_requested: bool,
) -> CloseDecision {
    if is_main_window && tray_ready && !explicit_exit_requested {
        CloseDecision::HideToTray
    } else {
        CloseDecision::AllowClose
    }
}

fn prevent_app_exit(ui_recreating: bool, explicit_exit_requested: bool) -> bool {
    ui_recreating && !explicit_exit_requested
}

pub fn install_tray(app: &mut App) -> tauri::Result<()> {
    let show_item = MenuItem::with_id(app, SHOW_MENU_ID, "显示窗口", true, None::<&str>)?;
    let quit_item = MenuItem::with_id(app, QUIT_MENU_ID, "退出", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show_item, &quit_item])?;

    let mut builder = TrayIconBuilder::with_id(TRAY_ID)
        .menu(&menu)
        .show_menu_on_left_click(false)
        .tooltip("Coding Tools MCP")
        .on_menu_event(|app, event| match event.id().as_ref() {
            SHOW_MENU_ID => show_main_window(app),
            QUIT_MENU_ID => request_explicit_exit(app),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if matches!(
                event,
                TrayIconEvent::Click {
                    button: MouseButton::Left,
                    button_state: MouseButtonState::Up,
                    ..
                }
            ) {
                show_main_window(tray.app_handle());
            }
        });

    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }

    builder.build(app)?;
    TRAY_READY.store(true, Ordering::SeqCst);
    Ok(())
}

pub fn handle_window_event(window: &Window, event: &WindowEvent) {
    if let WindowEvent::CloseRequested { api, .. } = event {
        let decision = close_decision(
            window.label() == MAIN_WINDOW_LABEL,
            TRAY_READY.load(Ordering::SeqCst),
            EXPLICIT_EXIT_REQUESTED.load(Ordering::SeqCst),
        );
        if decision == CloseDecision::HideToTray {
            api.prevent_close();
            if let Err(error) = window.hide() {
                eprintln!("failed to hide main window to tray: {error}");
            }
        }
    }
}

pub fn should_prevent_app_exit(ui_recreating: bool) -> bool {
    prevent_app_exit(
        ui_recreating,
        EXPLICIT_EXIT_REQUESTED.load(Ordering::SeqCst),
    )
}

fn show_main_window(app: &AppHandle) {
    let Some(window) = app.get_webview_window(MAIN_WINDOW_LABEL) else {
        return;
    };
    let _ = window.unminimize();
    let _ = window.show();
    let _ = window.set_focus();
}

fn request_explicit_exit(app: &AppHandle) {
    EXPLICIT_EXIT_REQUESTED.store(true, Ordering::SeqCst);
    app.exit(0);
}

#[cfg(test)]
mod tests {
    use super::{close_decision, prevent_app_exit, CloseDecision};

    #[test]
    fn main_window_hides_only_when_tray_is_ready() {
        assert_eq!(close_decision(true, true, false), CloseDecision::HideToTray);
        assert_eq!(
            close_decision(true, false, false),
            CloseDecision::AllowClose
        );
    }

    #[test]
    fn non_main_window_and_explicit_exit_are_never_hidden() {
        assert_eq!(
            close_decision(false, true, false),
            CloseDecision::AllowClose
        );
        assert_eq!(close_decision(true, true, true), CloseDecision::AllowClose);
    }

    #[test]
    fn explicit_exit_overrides_ui_recreate_keepalive() {
        assert!(prevent_app_exit(true, false));
        assert!(!prevent_app_exit(true, true));
        assert!(!prevent_app_exit(false, false));
    }
}
