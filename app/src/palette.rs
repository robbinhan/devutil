//! Workbench colors that are not shadcn tokens. `theme.json` sets each name
//! for light and dark; reading them here subscribes, the same way
//! `theme().primary` does.

use nsh_base::theme;
use nsh_core::prelude::*;

fn token(name: &str) -> AdaptiveColor {
    theme()
        .color(name)
        .unwrap_or(AdaptiveColor::TRANSPARENT)
}

pub fn danger() -> AdaptiveColor {
    token("danger")
}

pub fn success() -> AdaptiveColor {
    token("success")
}

pub fn warning() -> AdaptiveColor {
    token("warning")
}

pub fn info() -> AdaptiveColor {
    token("info")
}

pub fn primary_active() -> AdaptiveColor {
    token("primary-active")
}

pub fn list_head() -> AdaptiveColor {
    token("list-head")
}

pub fn list_active() -> AdaptiveColor {
    token("list-active")
}
