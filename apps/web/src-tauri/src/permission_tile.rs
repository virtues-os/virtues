//! The drag tile that sits on System Settings while someone grants the
//! collector a privacy permission.
//!
//! Full Disk Access and Accessibility are granted to the collector binary at
//! `~/.virtues/bin/virtues-collector`. That folder is hidden, so the list's "+"
//! dialog does not show it, and the list also shows the Virtues app, whose
//! toggle grants the collector nothing. The tile holds the collector file
//! itself: dragging it into the list adds the right program in one move.
//!
//! What a spike on macOS 27 (2026-11-15) found, and what the tile does about it:
//! - The list accepts a dropped bare executable and records it by path.
//! - Every drop asks for Touch ID or a password, and a drop made while that
//!   prompt is up is refused. The tile hides while the prompt is up.
//! - Placed under the window, the tile went off the bottom of the screen.
//!   It sits under the window when there is room, inside the window's bottom
//!   edge when there is not, and always on the visible screen.
//! - Reading window bounds from CGWindowList needs no permission.
//!
//! The tile follows System Settings with a 200 ms poll. It closes when System
//! Settings quits, when the person clicks back, or when the page calls `close`
//! because the grant has landed.

use std::cell::RefCell;
use std::ffi::c_void;
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant};

use objc2::rc::Retained;
use objc2::runtime::{AnyObject, ProtocolObject};
use objc2::{define_class, msg_send, AnyThread, DefinedClass, MainThreadMarker, MainThreadOnly};
use objc2_app_kit::{
    NSApplication, NSBackingStoreType, NSBox, NSBoxType, NSButton, NSColor, NSDragOperation,
    NSDraggingContext, NSDraggingItem, NSDraggingSession, NSDraggingSource, NSEvent, NSFont,
    NSImage, NSImageView, NSPanel, NSRunningApplication, NSScreen, NSTextField, NSView,
    NSVisualEffectBlendingMode, NSVisualEffectMaterial, NSVisualEffectState, NSVisualEffectView,
    NSWindowCollectionBehavior, NSWindowStyleMask, NSWorkspace,
};
use objc2_foundation::{
    NSArray, NSDictionary, NSNumber, NSObjectProtocol, NSPoint, NSRect, NSSize, NSString, NSURL,
};
use tauri::AppHandle;

const SETTINGS_BUNDLE_ID: &str = "com.apple.systempreferences";
const TILE_W_MAX: f64 = 520.0;
const TILE_W_MIN: f64 = 340.0;
const TILE_H: f64 = 112.0;
const INSET: f64 = 16.0;
const GAP: f64 = 8.0;
/// Narrower than any System Settings main window. A System Settings window
/// this small in front of the main one is the Touch ID or password prompt.
const PROMPT_MAX_W: f64 = 420.0;
/// How long to wait for System Settings to show a window before giving up.
const LAUNCH_WAIT: Duration = Duration::from_secs(10);
/// How long System Settings may sit with no window before the tile closes.
const GONE_WAIT: Duration = Duration::from_secs(3);
/// The tile never outlives this, whatever happens.
const MAX_LIFE: Duration = Duration::from_secs(15 * 60);

/// Bumped by every show and close. The poll thread stops when it changes.
static GENERATION: AtomicU64 = AtomicU64::new(0);
/// True from mouse-down until the drag session ends. Moving or hiding the
/// panel mid-drag would cancel the drag.
static DRAGGING: AtomicBool = AtomicBool::new(false);

struct Tile {
    panel: Retained<NSPanel>,
    _chip: Retained<DragChip>,
    opened: Instant,
    /// When System Settings last had a visible main window.
    last_seen: Option<Instant>,
}

thread_local! {
    // Main thread only: AppKit objects.
    static TILE: RefCell<Option<Tile>> = const { RefCell::new(None) };
}

pub struct ChipIvars {
    url: Retained<NSURL>,
    icon: Retained<NSImage>,
}

define_class!(
    // The draggable row inside the tile. An NSBox so it can draw a rounded
    // fill with typed API; `hitTest:` keeps the icon and label from taking
    // the click.
    #[unsafe(super(NSBox, NSView))]
    #[thread_kind = MainThreadOnly]
    #[name = "VirtuesPermissionDragChip"]
    #[ivars = ChipIvars]
    struct DragChip;

    impl DragChip {
        #[unsafe(method(hitTest:))]
        fn hit_test(&self, point: NSPoint) -> *mut NSView {
            let f = self.frame();
            let inside = point.x >= f.origin.x
                && point.x <= f.origin.x + f.size.width
                && point.y >= f.origin.y
                && point.y <= f.origin.y + f.size.height;
            if inside {
                (self as *const Self).cast_mut().cast()
            } else {
                std::ptr::null_mut()
            }
        }

        #[unsafe(method(acceptsFirstMouse:))]
        fn accepts_first_mouse(&self, _event: Option<&NSEvent>) -> bool {
            true
        }

        #[unsafe(method(mouseDownCanMoveWindow))]
        fn mouse_down_can_move_window(&self) -> bool {
            false
        }

        #[unsafe(method(mouseDown:))]
        fn mouse_down(&self, event: &NSEvent) {
            let ivars = self.ivars();
            let item = NSDraggingItem::initWithPasteboardWriter(
                NSDraggingItem::alloc(),
                ProtocolObject::from_ref(&*ivars.url),
            );
            let contents: &AnyObject = &ivars.icon;
            // SAFETY: an NSImage is a valid dragging image.
            unsafe {
                item.setDraggingFrame_contents(
                    NSRect::new(NSPoint::new(10.0, 6.0), NSSize::new(32.0, 32.0)),
                    Some(contents),
                );
            }
            DRAGGING.store(true, Ordering::SeqCst);
            let items = NSArray::from_retained_slice(&[item]);
            self.beginDraggingSessionWithItems_event_source(&items, event, ProtocolObject::from_ref(self));
        }

        /// The back button's action: close the tile and return to Virtues.
        #[unsafe(method(closeTile:))]
        fn close_tile(&self, _sender: Option<&AnyObject>) {
            close_now(true);
        }
    }

    unsafe impl NSObjectProtocol for DragChip {}

    unsafe impl NSDraggingSource for DragChip {
        #[unsafe(method(draggingSession:sourceOperationMaskForDraggingContext:))]
        fn source_operation_mask(
            &self,
            _session: &NSDraggingSession,
            _context: NSDraggingContext,
        ) -> NSDragOperation {
            NSDragOperation::Copy | NSDragOperation::Link | NSDragOperation::Generic
        }

        #[unsafe(method(draggingSession:endedAtPoint:operation:))]
        fn ended(&self, _session: &NSDraggingSession, _point: NSPoint, _operation: NSDragOperation) {
            DRAGGING.store(false, Ordering::SeqCst);
        }
    }
);

impl DragChip {
    fn new(mtm: MainThreadMarker, frame: NSRect, url: Retained<NSURL>, icon: Retained<NSImage>) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(ChipIvars { url, icon });
        unsafe { msg_send![super(this), initWithFrame: frame] }
    }
}

/// Show the tile for `collector`, replacing any tile already up. Call after
/// asking System Settings to open the pane; the tile appears once it is in
/// front.
pub fn show(app: &AppHandle, collector: &Path) {
    let generation = GENERATION.fetch_add(1, Ordering::SeqCst) + 1;
    let path = collector.to_string_lossy().into_owned();
    let _ = app.run_on_main_thread(move || {
        let Some(mtm) = MainThreadMarker::new() else { return };
        close_panel();
        let tile = build(mtm, &path);
        TILE.with(|t| *t.borrow_mut() = Some(tile));
    });

    let app = app.clone();
    std::thread::spawn(move || loop {
        std::thread::sleep(Duration::from_millis(200));
        if GENERATION.load(Ordering::SeqCst) != generation {
            break;
        }
        let _ = app.run_on_main_thread(move || {
            if GENERATION.load(Ordering::SeqCst) == generation {
                tick();
            }
        });
    });
}

/// Close the tile if one is up. With `return_to_app`, bring Virtues to the
/// front, which is what someone expects once the grant has landed.
pub fn close(app: &AppHandle, return_to_app: bool) {
    let _ = app.run_on_main_thread(move || close_now(return_to_app));
}

fn close_now(return_to_app: bool) {
    GENERATION.fetch_add(1, Ordering::SeqCst);
    let was_open = close_panel();
    if return_to_app && was_open {
        if let Some(mtm) = MainThreadMarker::new() {
            NSApplication::sharedApplication(mtm).activate();
        }
    }
}

fn close_panel() -> bool {
    TILE.with(|t| match t.borrow_mut().take() {
        Some(tile) => {
            tile.panel.orderOut(None);
            true
        }
        None => false,
    })
}

fn label(mtm: MainThreadMarker, text: &str, frame: NSRect, size: f64, secondary: bool) -> Retained<NSTextField> {
    let field = NSTextField::labelWithString(&NSString::from_str(text), mtm);
    field.setFrame(frame);
    if secondary {
        field.setFont(Some(&NSFont::systemFontOfSize(size)));
        field.setTextColor(Some(&NSColor::secondaryLabelColor()));
    } else {
        field.setFont(Some(&NSFont::boldSystemFontOfSize(size)));
    }
    field
}

fn symbol(name: &str) -> Option<Retained<NSImage>> {
    NSImage::imageWithSystemSymbolName_accessibilityDescription(&NSString::from_str(name), None)
}

fn build(mtm: MainThreadMarker, path: &str) -> Tile {
    let w = TILE_W_MAX;
    let h = TILE_H;
    let panel = NSPanel::initWithContentRect_styleMask_backing_defer(
        NSPanel::alloc(mtm),
        NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(w, h)),
        NSWindowStyleMask::Borderless | NSWindowStyleMask::NonactivatingPanel,
        NSBackingStoreType::Buffered,
        false,
    );
    // SAFETY: the panel is kept in TILE and only ever ordered out, never closed.
    unsafe { panel.setReleasedWhenClosed(false) };
    panel.setFloatingPanel(true);
    // Virtues is not the active app while the tile is up, and panels hide on
    // deactivate by default.
    panel.setHidesOnDeactivate(false);
    panel.setBecomesKeyOnlyIfNeeded(true);
    panel.setOpaque(false);
    panel.setBackgroundColor(Some(&NSColor::clearColor()));
    panel.setHasShadow(true);
    panel.setCollectionBehavior(
        NSWindowCollectionBehavior::CanJoinAllSpaces | NSWindowCollectionBehavior::FullScreenAuxiliary,
    );

    let background = NSVisualEffectView::initWithFrame(
        NSVisualEffectView::alloc(mtm),
        NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(w, h)),
    );
    background.setMaterial(NSVisualEffectMaterial::Popover);
    background.setBlendingMode(NSVisualEffectBlendingMode::BehindWindow);
    // Draw as active even though Virtues is in the background.
    background.setState(NSVisualEffectState::Active);
    background.setWantsLayer(true);
    // SAFETY: a layer-backed view has a CALayer, which takes these messages.
    unsafe {
        let layer: *mut AnyObject = msg_send![&*background, layer];
        if !layer.is_null() {
            let _: () = msg_send![layer, setCornerRadius: 14.0f64];
            let _: () = msg_send![layer, setMasksToBounds: true];
        }
    }
    background.setAutoresizingMask(objc2_app_kit::NSAutoresizingMaskOptions::ViewWidthSizable);

    if let Some(arrow) = symbol("arrow.up") {
        let view = NSImageView::imageViewWithImage(&arrow, mtm);
        view.setFrame(NSRect::new(NSPoint::new(16.0, h - 38.0), NSSize::new(20.0, 22.0)));
        view.setContentTintColor(Some(&NSColor::controlAccentColor()));
        background.addSubview(&view);
    }
    background.addSubview(&label(
        mtm,
        "Drag virtues-collector into the list above",
        NSRect::new(NSPoint::new(44.0, h - 34.0), NSSize::new(w - 60.0, 18.0)),
        13.0,
        false,
    ));
    background.addSubview(&label(
        mtm,
        "macOS asks for Touch ID or your password to add it.",
        NSRect::new(NSPoint::new(44.0, h - 52.0), NSSize::new(w - 60.0, 16.0)),
        11.0,
        true,
    ));

    let icon = NSApplication::sharedApplication(mtm)
        .applicationIconImage()
        .unwrap_or_else(|| NSWorkspace::sharedWorkspace().iconForFile(&NSString::from_str(path)));
    let chip_frame = NSRect::new(NSPoint::new(52.0, 12.0), NSSize::new(w - 64.0, 44.0));
    let url = NSURL::fileURLWithPath(&NSString::from_str(path));
    let chip = DragChip::new(mtm, chip_frame, url, icon.clone());
    chip.setBoxType(NSBoxType::Custom);
    chip.setBorderWidth(0.0);
    chip.setCornerRadius(10.0);
    chip.setFillColor(&NSColor::quaternaryLabelColor());
    chip.setTitlePosition(objc2_app_kit::NSTitlePosition::NoTitle);
    chip.setContentViewMargins(NSSize::new(0.0, 0.0));
    chip.setAutoresizingMask(objc2_app_kit::NSAutoresizingMaskOptions::ViewWidthSizable);
    if let Some(content) = chip.contentView() {
        let icon_view = NSImageView::imageViewWithImage(&icon, mtm);
        icon_view.setFrame(NSRect::new(NSPoint::new(10.0, 6.0), NSSize::new(32.0, 32.0)));
        content.addSubview(&icon_view);
        content.addSubview(&label(
            mtm,
            "virtues-collector",
            NSRect::new(NSPoint::new(50.0, 13.0), NSSize::new(chip_frame.size.width - 60.0, 18.0)),
            13.0,
            false,
        ));
    }
    background.addSubview(&chip);

    if let Some(chevron) = symbol("chevron.left") {
        // SAFETY: the chip outlives the button (both live in the panel), and
        // it implements `closeTile:`.
        let back = unsafe {
            NSButton::buttonWithImage_target_action(&chevron, Some(&chip), Some(objc2::sel!(closeTile:)), mtm)
        };
        back.setBordered(false);
        back.setFrame(NSRect::new(NSPoint::new(12.0, 20.0), NSSize::new(32.0, 28.0)));
        back.setToolTip(Some(&NSString::from_str("Back to Virtues")));
        background.addSubview(&back);
    }

    panel.setContentView(Some(&background));
    Tile { panel, _chip: chip, opened: Instant::now(), last_seen: None }
}

/// One window from CGWindowList, in its coordinates (origin at the top left
/// of the primary screen).
struct Win {
    pid: i32,
    layer: i64,
    rect: NSRect,
}

#[link(name = "CoreGraphics", kind = "framework")]
extern "C" {
    fn CGWindowListCopyWindowInfo(option: u32, relative_to_window: u32) -> *mut c_void;
    fn CGRectMakeWithDictionaryRepresentation(dict: *const c_void, rect: *mut NSRect) -> bool;
}

const ON_SCREEN_ONLY: u32 = 1 << 0;
const EXCLUDE_DESKTOP: u32 = 1 << 4;

/// On-screen windows, front to back.
fn windows() -> Vec<Win> {
    // SAFETY: a Copy function, so we own the returned CFArray. CFArray and
    // CFDictionary are toll-free bridged to NSArray and NSDictionary.
    let raw = unsafe { CGWindowListCopyWindowInfo(ON_SCREEN_ONLY | EXCLUDE_DESKTOP, 0) };
    let Some(list) = (unsafe { Retained::from_raw(raw.cast::<NSArray<NSDictionary<NSString, AnyObject>>>()) }) else {
        return Vec::new();
    };
    let number = |d: &NSDictionary<NSString, AnyObject>, key: &str| -> Option<i64> {
        let value = d.objectForKey(&NSString::from_str(key))?;
        Some(value.downcast_ref::<NSNumber>()?.longLongValue())
    };
    let mut out = Vec::new();
    for d in list.iter() {
        let (Some(pid), Some(layer)) = (number(&d, "kCGWindowOwnerPID"), number(&d, "kCGWindowLayer")) else {
            continue;
        };
        let Some(bounds) = d.objectForKey(&NSString::from_str("kCGWindowBounds")) else { continue };
        let mut rect = NSRect::ZERO;
        // SAFETY: the bounds value is a CFDictionary in CGRect form.
        let ok = unsafe { CGRectMakeWithDictionaryRepresentation(Retained::as_ptr(&bounds).cast(), &mut rect) };
        if ok {
            out.push(Win { pid: pid as i32, layer, rect });
        }
    }
    out
}

fn settings_pid() -> Option<i32> {
    let apps = NSRunningApplication::runningApplicationsWithBundleIdentifier(&NSString::from_str(SETTINGS_BUNDLE_ID));
    let app = apps.firstObject()?;
    Some(unsafe { msg_send![&*app, processIdentifier] })
}

fn frontmost_pid() -> Option<i32> {
    let app = NSWorkspace::sharedWorkspace().frontmostApplication()?;
    Some(unsafe { msg_send![&*app, processIdentifier] })
}

fn tick() {
    let Some(mtm) = MainThreadMarker::new() else { return };
    let mut close = false;
    TILE.with(|t| {
        let mut slot = t.borrow_mut();
        let Some(tile) = slot.as_mut() else { return };
        if DRAGGING.load(Ordering::SeqCst) {
            return;
        }
        let now = Instant::now();
        if now.duration_since(tile.opened) > MAX_LIFE {
            close = true;
            return;
        }

        let mine: Vec<Win> = match settings_pid() {
            Some(pid) => windows().into_iter().filter(|w| w.pid == pid && w.layer == 0).collect(),
            None => Vec::new(),
        };
        let Some(main) = mine.iter().max_by(|a, b| area(a.rect).total_cmp(&area(b.rect))) else {
            tile.panel.orderOut(None);
            let waited = now.duration_since(tile.last_seen.unwrap_or(tile.opened));
            if waited > if tile.last_seen.is_some() { GONE_WAIT } else { LAUNCH_WAIT } {
                close = true;
            }
            return;
        };
        tile.last_seen = Some(now);

        let in_front = frontmost_pid() == Some(main.pid);
        let prompt_up = mine.first().is_some_and(|w| w.rect.size.width < PROMPT_MAX_W);
        if !in_front || prompt_up {
            tile.panel.orderOut(None);
            return;
        }
        place(mtm, &tile.panel, main.rect);
        tile.panel.orderFrontRegardless();
    });
    if close {
        close_now(false);
    }
}

fn area(r: NSRect) -> f64 {
    r.size.width * r.size.height
}

/// Put the tile under System Settings' window, or inside its bottom edge when
/// there is no room below, kept within the visible part of that screen.
fn place(mtm: MainThreadMarker, panel: &NSPanel, cg: NSRect) {
    let screens = NSScreen::screens(mtm);
    let Some(primary) = screens.firstObject() else { return };
    let primary_h = primary.frame().size.height;
    // CGWindowList measures from the primary screen's top; AppKit from its bottom.
    let win = NSRect::new(
        NSPoint::new(cg.origin.x, primary_h - cg.origin.y - cg.size.height),
        cg.size,
    );
    let centre = NSPoint::new(win.origin.x + win.size.width / 2.0, win.origin.y + win.size.height / 2.0);
    let screen = screens
        .iter()
        .find(|s| {
            let f = s.frame();
            centre.x >= f.origin.x
                && centre.x < f.origin.x + f.size.width
                && centre.y >= f.origin.y
                && centre.y < f.origin.y + f.size.height
        })
        .unwrap_or(primary);
    let visible = screen.visibleFrame();

    let w = (win.size.width - 2.0 * INSET).clamp(TILE_W_MIN, TILE_W_MAX);
    let mut x = win.origin.x + (win.size.width - w) / 2.0;
    // Just below the window when it fits, so the tile covers none of the list.
    // Otherwise inside the window's bottom edge.
    let below = win.origin.y - GAP - TILE_H;
    let mut y = if below >= visible.origin.y + 8.0 { below } else { win.origin.y + INSET };
    x = x.clamp(visible.origin.x + 8.0, visible.origin.x + visible.size.width - w - 8.0);
    y = y.clamp(visible.origin.y + 8.0, visible.origin.y + visible.size.height - TILE_H - 8.0);

    let frame = NSRect::new(NSPoint::new(x, y), NSSize::new(w, TILE_H));
    if panel.frame() != frame {
        panel.setFrame_display(frame, true);
    }
}
