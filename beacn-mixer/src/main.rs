//! Beacn Mixer: a small native (GTK3 + WebKitGTK, Wayland-native) window that hosts the
//! Pipeweaver mixer UI served by the daemon on localhost. Installed as `pipeweaver-app`
//! so Pipeweaver's own "Open Interface" (and the Beacn tray/sidebar "Mixer") opens it.
//!
//! It shares the Beacn Utility's application id, so KDE treats both windows as one app
//! (one taskbar entry, one icon, one .desktop file). Single instance: launching it again
//! just brings the existing window forward.

use gtk::gdk::RGBA;
use gtk::gio::ApplicationFlags;
use gtk::glib::{self, ControlFlow};
use gtk::prelude::*;
use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::rc::Rc;
use std::time::Duration;
use webkit2gtk::{
    ContextMenu, ContextMenuAction, ContextMenuExt, ContextMenuItem, ContextMenuItemExt, LoadEvent, SettingsExt,
    WebContext, WebContextExt, WebView, WebViewExt,
};

const APP_ID: &str = "io.github.beacn_on_linux.beacn-utility";
const TITLE: &str = "Beacn Mixer";
const DEFAULT_URL: &str = "http://localhost:14565/";
const BG: &str = "#1b1e29";

fn url() -> String {
    std::env::var("BEACN_MIXER_URL").unwrap_or_else(|_| DEFAULT_URL.to_string())
}

fn state_dir() -> PathBuf {
    glib::user_config_dir().join("beacn-utility")
}

fn geometry_file() -> PathBuf {
    state_dir().join("mixer-window")
}

fn load_geometry() -> (i32, i32, bool) {
    std::fs::read_to_string(geometry_file())
        .ok()
        .and_then(|s| {
            let mut it = s.split_whitespace();
            let w = it.next()?.parse().ok()?;
            let h = it.next()?.parse().ok()?;
            let m = it.next().map(|v| v == "1").unwrap_or(false);
            Some((w, h, m))
        })
        .unwrap_or((1360, 820, false))
}

fn save_geometry(w: i32, h: i32, maximised: bool) {
    let _ = std::fs::create_dir_all(state_dir());
    let _ = std::fs::write(
        geometry_file(),
        format!("{w} {h} {}\n", if maximised { 1 } else { 0 }),
    );
}

fn waiting_page() -> String {
    format!(
        r#"<!doctype html><html><head><meta charset="utf-8"><style>
html,body{{margin:0;height:100%;background:{BG};color:#a0a6be;font:15px sans-serif;
display:flex;align-items:center;justify-content:center}}
.d{{width:10px;height:10px;border-radius:50%;background:#8e9bc1;display:inline-block;margin:0 4px;
animation:p 1.2s infinite ease-in-out}} .d:nth-child(2){{animation-delay:.2s}} .d:nth-child(3){{animation-delay:.4s}}
@keyframes p{{0%,80%,100%{{opacity:.25}}40%{{opacity:1}}}}
</style></head><body><div style="text-align:center"><div><span class="d"></span><span class="d"></span><span class="d"></span></div>
<p>Starting the audio mixer&hellip;</p></div></body></html>"#
    )
}

fn open_devices() {
    // The Beacn Utility is single-instance: launching it again opens its window.
    let _ = std::process::Command::new("beacn-utility")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map(|mut c| {
            std::thread::spawn(move || {
                let _ = c.wait();
            })
        });
}

fn build_window(app: &gtk::Application) -> gtk::ApplicationWindow {
    let (w, h, maximised) = load_geometry();
    let window = gtk::ApplicationWindow::builder()
        .application(app)
        .title(TITLE)
        .default_width(w)
        .default_height(h)
        .icon_name(APP_ID)
        .build();
    if maximised {
        window.maximize();
    }

    // Persistent storage (localStorage etc.) for the mixer UI
    let data = glib::user_data_dir().join("beacn-utility").join("mixer-webdata");
    let cache = glib::user_cache_dir().join("beacn-utility").join("mixer-webcache");
    let manager = webkit2gtk::WebsiteDataManager::builder()
        .base_data_directory(data.to_string_lossy().as_ref())
        .base_cache_directory(cache.to_string_lossy().as_ref())
        .build();
    let context = WebContext::builder().website_data_manager(&manager).build();
    context.set_cache_model(webkit2gtk::CacheModel::DocumentViewer);

    let webview = WebView::with_context(&context);
    webview.set_background_color(&RGBA::parse(BG).unwrap_or(RGBA::BLACK));
    if let Some(settings) = WebViewExt::settings(&webview) {
        settings.set_enable_developer_extras(std::env::var("BEACN_MIXER_DEVTOOLS").is_ok());
        settings.set_enable_back_forward_navigation_gestures(false);
        settings.set_hardware_acceleration_policy(webkit2gtk::HardwareAccelerationPolicy::OnDemand);
    }

    // Keep the title fixed (the page's own <title> would say "Pipeweaver")
    window.set_title(TITLE);

    // When the daemon isn't up yet, show a waiting page and keep retrying.
    let waiting = Rc::new(Cell::new(false));
    {
        let waiting = waiting.clone();
        webview.connect_load_failed(move |view, _event, failing_uri, _err| {
            if failing_uri.starts_with("http") {
                waiting.set(true);
                view.load_alternate_html(&waiting_page(), failing_uri, None);
                let view = view.clone();
                let waiting = waiting.clone();
                glib::timeout_add_local(Duration::from_millis(1000), move || {
                    if waiting.get() {
                        waiting.set(false);
                        view.load_uri(&url());
                    }
                    ControlFlow::Break
                });
                return true;
            }
            false
        });
    }
    webview.connect_load_changed(|_view, ev| {
        if ev == LoadEvent::Finished {
            // nothing for now
        }
    });

    // Open external links (docs etc.) in the default browser instead of inside the mixer.
    webview.connect_decide_policy(|_view, decision, kind| {
        use webkit2gtk::{NavigationPolicyDecision, PolicyDecisionExt, PolicyDecisionType, URIRequestExt};
        if kind == PolicyDecisionType::NewWindowAction
            || kind == PolicyDecisionType::NavigationAction
        {
            if let Some(nav) = decision.downcast_ref::<NavigationPolicyDecision>()
                && let Some(action) = webkit2gtk::NavigationPolicyDecisionExt::navigation_action(nav)
                && let Some(action) = Some(action)
                && let Some(req) = action.request()
                && let Some(uri) = req.uri()
            {
                let local = uri.starts_with("http://localhost:14565")
                    || uri.starts_with("http://127.0.0.1:14565")
                    || uri.starts_with(&url())
                    || uri.starts_with("about:")
                    || uri.starts_with("data:");
                if !local {
                    let _ = gtk::gio::AppInfo::launch_default_for_uri(
                        &uri,
                        None::<&gtk::gio::AppLaunchContext>,
                    );
                    decision.ignore();
                    return true;
                }
            }
        }
        false
    });

    // App-like context menu: Devices, Reload. No "Back", "Inspect" etc.
    webview.connect_context_menu(|view, menu: &ContextMenu, _ev, _hit| {
        let keep_editing = menu
            .items()
            .iter()
            .filter(|i| {
                matches!(
                    i.stock_action(),
                    ContextMenuAction::Copy
                        | ContextMenuAction::Cut
                        | ContextMenuAction::Paste
                        | ContextMenuAction::SelectAll
                )
            })
            .cloned()
            .collect::<Vec<ContextMenuItem>>();
        menu.remove_all();
        for item in &keep_editing {
            menu.append(item);
        }
        if !keep_editing.is_empty() {
            menu.append(&ContextMenuItem::new_separator());
        }
        let group = gtk::gio::SimpleActionGroup::new();
        let devices = gtk::gio::SimpleAction::new("devices", None);
        devices.connect_activate(|_, _| open_devices());
        let reload = gtk::gio::SimpleAction::new("reload", None);
        {
            let view = view.clone();
            reload.connect_activate(move |_, _| view.reload());
        }
        group.add_action(&devices);
        group.add_action(&reload);
        menu.append(&ContextMenuItem::from_gaction(&devices, "Beacn Devices…", None));
        menu.append(&ContextMenuItem::from_gaction(&reload, "Reload", None));
        // keep the group alive for the menu lifetime
        unsafe {
            menu.set_data("beacn-actions", group);
        }
        false
    });

    // Keyboard: Ctrl+R / F5 reload, Ctrl+W / Ctrl+Q close, Ctrl+D devices
    {
        let view = webview.clone();
        window.connect_key_press_event(move |win, ev| {
            use gtk::gdk::{ModifierType, keys::constants as k};
            let ctrl = ev.state().contains(ModifierType::CONTROL_MASK);
            let key = ev.keyval();
            if key == k::F5 || (ctrl && (key == k::r || key == k::R)) {
                view.reload();
                return glib::Propagation::Stop;
            }
            if ctrl && (key == k::w || key == k::q) {
                win.close();
                return glib::Propagation::Stop;
            }
            if ctrl && key == k::d {
                open_devices();
                return glib::Propagation::Stop;
            }
            glib::Propagation::Proceed
        });
    }

    let maximised_state = Rc::new(Cell::new(maximised));
    let size = Rc::new(RefCell::new((w, h)));
    {
        let maximised_state = maximised_state.clone();
        window.connect_window_state_event(move |_, ev| {
            maximised_state.set(
                ev.new_window_state()
                    .contains(gtk::gdk::WindowState::MAXIMIZED),
            );
            glib::Propagation::Proceed
        });
    }
    {
        let size = size.clone();
        let maximised_state = maximised_state.clone();
        window.connect_configure_event(move |win, _| {
            if !maximised_state.get() {
                *size.borrow_mut() = win.size();
            }
            false
        });
    }
    window.connect_delete_event(move |_, _| {
        let (w, h) = *size.borrow();
        save_geometry(w, h, maximised_state.get());
        glib::Propagation::Proceed
    });

    window.add(&webview);
    webview.load_uri(&url());
    window.show_all();
    window
}

fn main() -> glib::ExitCode {
    // Prefer native Wayland; GDK falls back to X11 on its own if there's no Wayland.
    if std::env::var_os("GDK_BACKEND").is_none() && std::env::var_os("WAYLAND_DISPLAY").is_some() {
        unsafe { std::env::set_var("GDK_BACKEND", "wayland,x11") };
    }
    glib::set_prgname(Some(APP_ID));
    glib::set_application_name(TITLE);

    let app = gtk::Application::new(Some(APP_ID), ApplicationFlags::default());
    app.connect_activate(|app| {
        if let Some(win) = app.windows().first() {
            win.present();
        } else {
            build_window(app);
        }
    });
    app.run_with_args::<&str>(&[])
}
