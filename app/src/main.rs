//! Opens the DevUtil workbench in a native window.
//!
//! On macOS this is an AppKit window (`nsh-platform-macos` 0.3.0). Linux uses
//! GTK 4 and Windows uses the NSH Windows backend. `nsh run` and
//! `nsh package` mount the same `App` from `nsh.json` (`"root": "App"`)
//! against the published NSH 0.3.0 crates. No `NSH_SOURCE` checkout is
//! required.

#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

#[cfg(target_os = "linux")]
use nsh_platform_linux as platform;
#[cfg(target_os = "macos")]
use nsh_platform_macos as platform;
#[cfg(target_os = "windows")]
use nsh_platform_windows as platform;

#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
fn main() -> impl std::process::Termination {
    env_logger::init();
    platform::set_ui_sets(vec![nsh_base::registry(), devutil::registry()]);
    let theme = devutil::app_theme();
    platform::run_app(
        platform::AppSource::Rust(
            platform::Root::of::<devutil::App>()
                .tokens(theme.into_tokens())
                .app_id("app.devutil.nsh"),
        ),
        platform::AppOptions {
            title: "DevUtil".to_owned(),
            app_id: "app.devutil.nsh".to_owned(),
            size: (1100, 720),
            appearance: platform::Appearance::System,
            ..Default::default()
        },
    )
}

#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
compile_error!(
    "Run this binary on macOS, Windows, or Linux. Use `nsh run` for the other NSH targets."
);
