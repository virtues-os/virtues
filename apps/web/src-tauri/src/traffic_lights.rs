//! The traffic lights in the Mac app's window header.
//!
//! The title bar is an overlay (main.rs), so the page runs to the window's top
//! edge and the three buttons sit in the page's header row
//! (WindowHeader.svelte). `place` puts them on that row's centre line, and
//! `end` tells the page where they finish, so the header's controls can follow
//! them.
//!
//! AppKit lays the title bar out again on every resize, a zoom included, and
//! puts the buttons back at its own spot each time: 14pt down, above the row's
//! 20pt centre line. So main.rs calls `place` after every resize, not once.
//! tao's `traffic_light_position` re-applies only when its own view redraws,
//! and the webview covers that view, so it lost them after the first resize.
//!
//! The buttons' size and spacing depend on the SDK the app links: 14×16 and
//! 20pt apart with the macOS 15 SDK, 14×14 and 23pt apart with the macOS 26
//! SDK. Both are read from AppKit, never assumed.

use std::ffi::c_void;

use objc2::rc::Retained;
use objc2::{MainThreadMarker, MainThreadOnly};
use objc2_app_kit::{
    NSBackingStoreType, NSButton, NSWindow, NSWindowButton, NSWindowStyleMask, NSWindowTitleVisibility,
};
use objc2_foundation::{NSPoint, NSRect, NSSize};

/// The header row's centre line: 20pt down its 40pt (`--chrome-row-h`).
const CENTRE_Y: f64 = 20.0;
/// The close button's centre: as far in from the window's left edge as the
/// row's centre line is down from its top.
const CLOSE_CENTRE_X: f64 = 20.0;

fn buttons(window: &NSWindow) -> Option<[Retained<NSButton>; 3]> {
    Some([
        window.standardWindowButton(NSWindowButton::CloseButton)?,
        window.standardWindowButton(NSWindowButton::MiniaturizeButton)?,
        window.standardWindowButton(NSWindowButton::ZoomButton)?,
    ])
}

/// Puts the lights on the header row's centre line. Does nothing in full
/// screen, where macOS shows them in its own bar.
pub fn place(ns_window: *mut c_void) {
    if MainThreadMarker::new().is_none() {
        return;
    }
    // SAFETY: tauri's `ns_window()` pointer, used on the main thread.
    let Some(window) = (unsafe { ns_window.cast::<NSWindow>().as_ref() }) else { return };
    if window.styleMask().contains(NSWindowStyleMask::FullScreen) {
        return;
    }
    let Some(buttons) = buttons(window) else { return };
    let pitch = buttons[1].frame().origin.x - buttons[0].frame().origin.x;

    // The buttons sit in the title bar view, inside its container. The
    // container grows to the header row's height so the buttons fit in it.
    let Some(bar) = (unsafe { buttons[0].superview() }) else { return };
    let Some(container) = (unsafe { bar.superview() }) else { return };
    let height = window.frame().size.height;
    let mut frame = container.frame();
    frame.size.height = 2.0 * CENTRE_Y;
    frame.origin.y = height - frame.size.height;
    container.setFrame(frame);

    for (i, button) in buttons.iter().enumerate() {
        // Window coordinates count up from the bottom edge.
        let centre = NSPoint::new(CLOSE_CENTRE_X + i as f64 * pitch, height - CENTRE_Y);
        let centre = bar.convertPoint_fromView(centre, None);
        let size = button.frame().size;
        button.setFrameOrigin(NSPoint::new(centre.x - size.width / 2.0, centre.y - size.height / 2.0));
    }
}

/// Where the zoom button ends once placed, in points from the window's left
/// edge. The page needs it before the main window exists, so it is read from
/// a window built the same way that is never shown. `None` when AppKit gives
/// that window no buttons; the page then keeps its own default.
pub fn end(mtm: MainThreadMarker) -> Option<f64> {
    let style = NSWindowStyleMask::Titled
        | NSWindowStyleMask::Closable
        | NSWindowStyleMask::Miniaturizable
        | NSWindowStyleMask::Resizable
        | NSWindowStyleMask::FullSizeContentView;
    let rect = NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(800.0, 600.0));
    // SAFETY: a plain window that is never shown or closed. Dropping it
    // releases it.
    let window = unsafe {
        let window = NSWindow::initWithContentRect_styleMask_backing_defer(
            NSWindow::alloc(mtm),
            rect,
            style,
            NSBackingStoreType::Buffered,
            true,
        );
        window.setReleasedWhenClosed(false);
        window
    };
    window.setTitlebarAppearsTransparent(true);
    window.setTitleVisibility(NSWindowTitleVisibility::Hidden);
    let [close, miniaturize, zoom] = buttons(&window)?;
    let pitch = miniaturize.frame().origin.x - close.frame().origin.x;
    Some(CLOSE_CENTRE_X + 2.0 * pitch + zoom.frame().size.width / 2.0)
}
