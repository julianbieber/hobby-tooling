//! `feathers_kit` — a small reusable UI kit layered on top of `bevy_feathers`.
//!
//! `bevy_feathers` is deliberately minimal: it ships interactive controls (button, checkbox,
//! slider, …) and a token-based theming system, but no layout/presentation widgets. This crate
//! fills that gap with composable builders (`card`, `panel`, `row`/`col`, `divider`, `badge`,
//! `status_dot`, `scroll_list`, `sidebar_item`, `icon_button`, `text*`) built on raw `bevy_ui`,
//! plus a handful of custom theme tokens and a [`KitPlugin`] that powers hover restyling and
//! mouse-wheel scrolling.
//!
//! It is intentionally domain-agnostic: nothing here knows about git, worktrees, or tasks.

// Bevy query/system signatures are inherently verbose; this mirrors bevy_feathers' own choice.
#![allow(clippy::type_complexity)]

use bevy::ecs::spawn::Spawn;
use bevy::input::mouse::{MouseScrollUnit, MouseWheel};
use bevy::picking::hover::Hovered;
use bevy::prelude::*;
use bevy::ui::ScrollPosition;

use bevy_feathers::constants::fonts;
// The 0.18 `button` builder was renamed to `button_bundle` and deprecated in 0.19 in favor of
// feathers' experimental BSN `button()` scene API; the kit stays on the bundle builder until that
// API stabilizes.
#[allow(deprecated)]
use bevy_feathers::controls::button_bundle;
use bevy_feathers::controls::{ButtonBundleProps, ButtonVariant};
use bevy_feathers::rounded_corners::RoundedCorners;
use bevy_feathers::theme::{
    ThemeBackgroundColor, ThemeBorderColor, ThemeTextColor, ThemeToken, ThemedText, UiTheme,
};

pub use bevy_feathers::controls::{
    ButtonBundleProps as FeathersButtonProps, ButtonVariant as Variant,
};
pub use bevy_feathers::dark_theme::create_dark_theme;
pub use bevy_feathers::rounded_corners::RoundedCorners as Corners;
pub use bevy_feathers::theme::{ThemeToken as Token, UiTheme as Theme};
pub use bevy_feathers::{FeathersPlugins, tokens as feathers_tokens};
pub use bevy_ui_widgets::{Activate, Button};

// ---------------------------------------------------------------------------
// Spacing scale (4px grid)
// ---------------------------------------------------------------------------

/// A 4px-based spacing scale, used for paddings and gaps throughout the kit.
pub mod sp {
    /// 4px
    pub const XS: f32 = 4.0;
    /// 8px
    pub const S: f32 = 8.0;
    /// 12px
    pub const M: f32 = 12.0;
    /// 16px
    pub const L: f32 = 16.0;
    /// 24px
    pub const XL: f32 = 24.0;
}

// ---------------------------------------------------------------------------
// Theme tokens
// ---------------------------------------------------------------------------

/// Custom design-token keys registered by [`install_kit_tokens`]. These extend the feathers
/// dark theme with surfaces, accents, and status colors that feathers does not provide.
pub mod kit_tokens {
    /// Sidebar surface background.
    pub const SIDEBAR_BG: &str = "kit.sidebar.bg";
    /// Selected/hovered sidebar item background.
    pub const SIDEBAR_ITEM_SEL: &str = "kit.sidebar.item.sel";
    /// Main content panel background.
    pub const PANEL_BG: &str = "kit.panel.bg";
    /// Card background.
    pub const CARD_BG: &str = "kit.card.bg";
    /// Card background when hovered.
    pub const CARD_BG_HOVER: &str = "kit.card.bg.hover";
    /// Card border.
    pub const CARD_BORDER: &str = "kit.card.border";
    /// Subtle divider line.
    pub const DIVIDER: &str = "kit.divider";
    /// Neutral chip/badge background.
    pub const CHIP_BG: &str = "kit.chip.bg";
    /// Accent color (selection, highlights).
    pub const ACCENT: &str = "kit.accent";
    /// Status dot: clean working tree.
    pub const DOT_CLEAN: &str = "kit.dot.clean";
    /// Status dot: dirty working tree.
    pub const DOT_DIRTY: &str = "kit.dot.dirty";
    /// Status dot: done / archived.
    pub const DOT_DONE: &str = "kit.dot.done";
    /// Positive/merged status label (purple).
    pub const STATUS_MERGED: &str = "kit.status.merged";
    /// Negative/closed status label (red).
    pub const STATUS_CLOSED: &str = "kit.status.closed";
}

/// Build a [`ThemeToken`] from a static token key (works with both feathers and kit token keys).
#[inline]
pub fn tok(s: &'static str) -> ThemeToken {
    ThemeToken::new_static(s)
}

/// Register all custom kit colors into the given theme. Call this after seeding the theme with
/// [`create_dark_theme`] and **before** any UI is spawned, otherwise color lookups fall back to
/// the feathers "missing token" error color.
pub fn install_kit_tokens(theme: &mut UiTheme) {
    use kit_tokens as k;
    let c = Color::srgb_u8;
    // The feathers dark theme leaves the generic text tokens unset, so seed them here.
    theme.set_color("feathers.text.main", c(0xd8, 0xdd, 0xe6));
    theme.set_color("feathers.text.dim", c(0x8b, 0x93, 0xa1));
    theme.set_color(k::SIDEBAR_BG, c(0x1b, 0x1d, 0x22));
    theme.set_color(k::SIDEBAR_ITEM_SEL, c(0x2a, 0x2f, 0x3a));
    theme.set_color(k::PANEL_BG, c(0x16, 0x18, 0x1c));
    theme.set_color(k::CARD_BG, c(0x24, 0x27, 0x2e));
    theme.set_color(k::CARD_BG_HOVER, c(0x2d, 0x31, 0x3a));
    theme.set_color(k::CARD_BORDER, c(0x35, 0x39, 0x42));
    theme.set_color(k::DIVIDER, c(0x2b, 0x2f, 0x36));
    theme.set_color(k::CHIP_BG, c(0x33, 0x38, 0x42));
    theme.set_color(k::ACCENT, c(0x6c, 0x8c, 0xff));
    theme.set_color(k::DOT_CLEAN, c(0x5f, 0xae, 0x7e));
    theme.set_color(k::DOT_DIRTY, c(0xd9, 0xa2, 0x3b));
    theme.set_color(k::DOT_DONE, c(0x6c, 0x8c, 0xff));
    theme.set_color(k::STATUS_MERGED, c(0xa9, 0x7b, 0xff));
    theme.set_color(k::STATUS_CLOSED, c(0xe0, 0x6c, 0x6c));
}

// ---------------------------------------------------------------------------
// Fonts
// ---------------------------------------------------------------------------

/// Preloaded font handles (the embedded feathers fonts), inserted by [`KitPlugin`] at startup.
#[derive(Resource, Clone)]
pub struct KitFonts {
    /// Regular sans font.
    pub regular: Handle<Font>,
    /// Bold sans font.
    pub bold: Handle<Font>,
    /// Monospace font (used for branch names, SHAs, …).
    pub mono: Handle<Font>,
}

fn load_kit_fonts(mut commands: Commands, assets: Res<AssetServer>) {
    commands.insert_resource(KitFonts {
        regular: assets.load(fonts::REGULAR),
        bold: assets.load(fonts::BOLD),
        mono: assets.load(fonts::MONO),
    });
}

fn install_kit_tokens_sys(mut theme: ResMut<UiTheme>) {
    install_kit_tokens(&mut theme);
}

// ---------------------------------------------------------------------------
// Markers & components
// ---------------------------------------------------------------------------

/// Marks a node whose background swaps between two tokens on hover. Skipped while [`KitSelected`]
/// is present so a selection highlight is not stomped by hover restyling.
#[derive(Component, Clone, Copy)]
pub struct KitHoverable {
    /// Background token when not hovered.
    pub normal: &'static str,
    /// Background token when hovered.
    pub hover: &'static str,
}

/// Marks a currently-selected item; suppresses [`KitHoverable`] restyling on that entity.
#[derive(Component, Default)]
pub struct KitSelected;

/// Marks a node with `Overflow::scroll_y()` that should respond to the mouse wheel when hovered.
#[derive(Component, Default)]
pub struct KitScrollable;

// ---------------------------------------------------------------------------
// Text builders
// ---------------------------------------------------------------------------

/// Regular themed text at the given size.
pub fn text(fonts: &KitFonts, s: impl Into<String>, size: f32, token: ThemeToken) -> impl Bundle {
    (
        Text::new(s.into()),
        TextFont {
            font: fonts.regular.clone().into(),
            font_size: FontSize::Px(size),
            ..default()
        },
        ThemeTextColor(token),
    )
}

/// Bold themed text at the given size.
pub fn text_bold(
    fonts: &KitFonts,
    s: impl Into<String>,
    size: f32,
    token: ThemeToken,
) -> impl Bundle {
    (
        Text::new(s.into()),
        TextFont {
            font: fonts.bold.clone().into(),
            font_size: FontSize::Px(size),
            ..default()
        },
        ThemeTextColor(token),
    )
}

/// Monospace themed text at the given size (branch names, SHAs, …).
pub fn text_mono(
    fonts: &KitFonts,
    s: impl Into<String>,
    size: f32,
    token: ThemeToken,
) -> impl Bundle {
    (
        Text::new(s.into()),
        TextFont {
            font: fonts.mono.clone().into(),
            font_size: FontSize::Px(size),
            ..default()
        },
        ThemeTextColor(token),
    )
}

// ---------------------------------------------------------------------------
// Layout primitives
// ---------------------------------------------------------------------------

/// A horizontal flex row, vertically centered, with the given column gap.
pub fn row(gap: f32, children: impl Bundle) -> impl Bundle {
    (
        Node {
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
            column_gap: Val::Px(gap),
            ..default()
        },
        children,
    )
}

/// A vertical flex column with the given row gap.
pub fn col(gap: f32, children: impl Bundle) -> impl Bundle {
    (
        Node {
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(gap),
            ..default()
        },
        children,
    )
}

/// A flex spacer that pushes following siblings to the far edge.
pub fn spacer_grow() -> impl Bundle {
    Node {
        flex_grow: 1.0,
        ..default()
    }
}

/// A background panel (column layout, padded) painted with the given background token.
pub fn panel(bg_token: &'static str, children: impl Bundle) -> impl Bundle {
    (
        Node {
            flex_direction: FlexDirection::Column,
            padding: UiRect::all(Val::Px(sp::M)),
            ..default()
        },
        ThemeBackgroundColor(tok(bg_token)),
        children,
    )
}

/// A 1px horizontal divider line.
pub fn divider() -> impl Bundle {
    (
        Node {
            width: Val::Percent(100.0),
            height: Val::Px(1.0),
            ..default()
        },
        ThemeBackgroundColor(tok(kit_tokens::DIVIDER)),
    )
}

/// An elevated card surface (column layout, padded, rounded, bordered) with hover restyling.
pub fn card(children: impl Bundle) -> impl Bundle {
    (
        Node {
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(sp::S),
            padding: UiRect::all(Val::Px(sp::M)),
            border: UiRect::all(Val::Px(1.0)),
            border_radius: BorderRadius::all(Val::Px(8.0)),
            ..default()
        },
        ThemeBackgroundColor(tok(kit_tokens::CARD_BG)),
        ThemeBorderColor(tok(kit_tokens::CARD_BORDER)),
        KitHoverable {
            normal: kit_tokens::CARD_BG,
            hover: kit_tokens::CARD_BG_HOVER,
        },
        Hovered::default(),
        children,
    )
}

/// A small rounded badge/chip with a label, painted with the given background token.
pub fn badge(
    fonts: &KitFonts,
    s: impl Into<String>,
    bg_token: &'static str,
    text_token: ThemeToken,
) -> impl Bundle {
    (
        Node {
            padding: UiRect::axes(Val::Px(sp::S), Val::Px(2.0)),
            align_items: AlignItems::Center,
            justify_content: JustifyContent::Center,
            border_radius: BorderRadius::all(Val::Px(6.0)),
            ..default()
        },
        ThemeBackgroundColor(tok(bg_token)),
        children![text(fonts, s, 11.0, text_token)],
    )
}

/// Working-tree / task status, expressed as a colored dot.
#[derive(Clone, Copy)]
pub enum DotState {
    /// Clean working tree.
    Clean,
    /// Dirty working tree.
    Dirty,
    /// Done / archived.
    Done,
}

/// An 8px circular status dot.
pub fn status_dot(state: DotState) -> impl Bundle {
    let token = match state {
        DotState::Clean => kit_tokens::DOT_CLEAN,
        DotState::Dirty => kit_tokens::DOT_DIRTY,
        DotState::Done => kit_tokens::DOT_DONE,
    };
    (
        Node {
            width: Val::Px(8.0),
            height: Val::Px(8.0),
            border_radius: BorderRadius::MAX,
            ..default()
        },
        ThemeBackgroundColor(tok(token)),
    )
}

/// A vertically-scrollable column. `overrides` lets the caller attach a marker (e.g. a container
/// tag) and `children` are the initial contents.
pub fn scroll_list(overrides: impl Bundle, children: impl Bundle) -> impl Bundle {
    (
        Node {
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(sp::S),
            overflow: Overflow::scroll_y(),
            flex_grow: 1.0,
            padding: UiRect::all(Val::Px(sp::M)),
            ..default()
        },
        ScrollPosition::default(),
        KitScrollable,
        Hovered::default(),
        overrides,
        children,
    )
}

/// A full-width clickable sidebar row. Emits [`Activate`] when clicked. `overrides` carries the
/// caller's marker (and, for the selected item, [`KitSelected`] + a selected background token).
pub fn sidebar_item(overrides: impl Bundle, children: impl Bundle) -> impl Bundle {
    (
        Node {
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
            column_gap: Val::Px(sp::S),
            width: Val::Percent(100.0),
            padding: UiRect::axes(Val::Px(sp::M), Val::Px(sp::S)),
            border_radius: BorderRadius::all(Val::Px(6.0)),
            ..default()
        },
        ThemeBackgroundColor(tok(kit_tokens::SIDEBAR_BG)),
        KitHoverable {
            normal: kit_tokens::SIDEBAR_BG,
            hover: kit_tokens::SIDEBAR_ITEM_SEL,
        },
        Hovered::default(),
        Button,
        overrides,
        children,
    )
}

/// A single-line editable text field: a themed, focus-ringed container around a
/// `bevy_text::EditableText`. Click to focus, then type; editing is driven by
/// [`bevy_ui_widgets::EditableTextInputPlugin`], which [`KitPlugin`] registers.
///
/// `overrides` lands on the **inner editable entity**, so callers can attach a marker there and
/// later read the value via `Query<(&Marker, &EditableText)>` (`editable.value().to_string()`),
/// or restrict input with a `bevy::text::EditableTextFilter`.
pub fn text_input(fonts: &KitFonts, initial: &str, overrides: impl Bundle) -> impl Bundle {
    (
        Node {
            height: Val::Px(28.0),
            align_items: AlignItems::Center,
            padding: UiRect::axes(Val::Px(sp::S), Val::Px(2.0)),
            border_radius: BorderRadius::all(Val::Px(6.0)),
            flex_grow: 1.0,
            ..default()
        },
        ThemeBackgroundColor(bevy_feathers::tokens::TEXT_INPUT_BG),
        bevy_feathers::focus::FocusWithinIndicator,
        children![(
            Node {
                flex_grow: 1.0,
                ..default()
            },
            bevy::text::EditableText::new(initial),
            bevy::text::TextLayout {
                linebreak: bevy::text::LineBreak::NoWrap,
                ..default()
            },
            TextFont {
                font: fonts.regular.clone().into(),
                font_size: FontSize::Px(13.0),
                ..default()
            },
            ThemeTextColor(bevy_feathers::tokens::TEXT_INPUT_TEXT),
            bevy::text::TextCursorStyle::default(),
            bevy::input_focus::tab_navigation::TabIndex(0),
            overrides,
        )],
    )
}

/// A labeled feathers button. `overrides` is where a marker component (carrying the click action)
/// goes; the label inherits the button's themed font and color.
#[allow(deprecated)]
pub fn icon_button(
    variant: ButtonVariant,
    overrides: impl Bundle,
    label: impl Into<String>,
) -> impl Bundle {
    button_bundle(
        ButtonBundleProps {
            variant,
            corners: RoundedCorners::All,
        },
        overrides,
        Spawn((Text::new(label.into()), ThemedText)),
    )
}

// ---------------------------------------------------------------------------
// Plugin & systems
// ---------------------------------------------------------------------------

/// Installs kit tokens + fonts (PreStartup) and the hover/scroll systems (Update).
pub struct KitPlugin;

impl Plugin for KitPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(PreStartup, (install_kit_tokens_sys, load_kit_fonts));
        app.add_systems(Update, (hover_restyle, scroll_system));
        // Drives [`text_input`] editing (click-to-focus, keyboard, selection, IME). Not part of
        // `FeathersPlugins`, so register it here.
        if !app.is_plugin_added::<bevy_ui_widgets::EditableTextInputPlugin>() {
            app.add_plugins(bevy_ui_widgets::EditableTextInputPlugin);
        }
    }
}

/// Swap background color on hover for [`KitHoverable`] nodes that are not selected.
fn hover_restyle(
    theme: Res<UiTheme>,
    mut q: Query<
        (&Hovered, &KitHoverable, &mut BackgroundColor),
        (Changed<Hovered>, Without<KitSelected>),
    >,
) {
    for (hovered, h, mut bg) in &mut q {
        let key = if hovered.get() { h.hover } else { h.normal };
        bg.0 = theme.color(&tok(key));
    }
}

/// Apply mouse-wheel deltas to whichever [`KitScrollable`] is currently hovered.
fn scroll_system(
    mut wheel: MessageReader<MouseWheel>,
    mut q: Query<(&Hovered, &mut ScrollPosition), With<KitScrollable>>,
) {
    let mut dy = 0.0;
    for ev in wheel.read() {
        dy += match ev.unit {
            MouseScrollUnit::Line => ev.y * 24.0,
            MouseScrollUnit::Pixel => ev.y,
        };
    }
    if dy == 0.0 {
        return;
    }
    for (hovered, mut scroll) in &mut q {
        if hovered.get() {
            scroll.0.y = (scroll.0.y - dy).max(0.0);
        }
    }
}
