//! Opens the DevUtil workbench in a native window.
//!
//! On macOS this is an AppKit window (`nsh-platform-macos` 0.1.1). Linux uses
//! GTK 4 and Windows uses the NSH Windows backend. `nsh run` can host the
//! same `App` through `App.tsx` when `NSH_SOURCE` points at an NSH 0.1.1
//! checkout.

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
    platform::run_app(
        platform::AppSource::Rust(
            platform::Root::of::<devutil::App>().theme(devutil::app_theme()),
        ),
        platform::AppOptions {
            title: "DevUtil".to_owned(),
            #[cfg(any(target_os = "linux", target_os = "windows"))]
            app_id: "app.devutil.nsh".to_owned(),
            size: (1100, 720),
            appearance: platform::Appearance::System,
            script: std::env::var("NSH_SCRIPT").ok(),
            ..Default::default()
        },
    )
}

#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
compile_error!(
    "Run this binary on macOS, Windows, or Linux. Use `nsh run` for the other NSH targets."
);
