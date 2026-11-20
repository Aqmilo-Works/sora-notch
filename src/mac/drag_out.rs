//! The macOS side of drag_out (drag_out.rs), its `os` module. Everything else there is shared.
//! A file is dragged out of the Tray with an AppKit drag session from the notch window's view (the file's own icon under
//! the pointer), started from the mouse event that is being handled, as drag-and-drop libraries do on the Mac.
#![allow(unused_imports, unused_unsafe)]
use super::*;
use std::cell::Cell;
use objc2::{define_class, msg_send, rc::Retained, runtime::ProtocolObject, AllocAnyThread, DefinedClass, MainThreadMarker, MainThreadOnly};
use objc2_app_kit::{NSApplication, NSDragOperation, NSDraggingContext, NSDraggingItem, NSDraggingSession, NSDraggingSource, NSView, NSWindow, NSWorkspace};
use objc2_foundation::{NSArray, NSObject, NSObjectProtocol, NSPoint, NSRect, NSSize, NSString, NSURL};

type Done = Box<dyn FnOnce(crate::AppResult<bool>) + Send>;
struct Ivars { done: Cell<Option<Done>> }
define_class!(
    // The drag's source: copies only, and reports how the drag ended.
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "SoraTrayDragSource"]
    #[ivars = Ivars]
    struct FileSource;

    unsafe impl NSObjectProtocol for FileSource {}

    unsafe impl NSDraggingSource for FileSource {
        #[unsafe(method(draggingSession:sourceOperationMaskForDraggingContext:))]
        fn operations(&self, _session: &NSDraggingSession, _context: NSDraggingContext) -> NSDragOperation { NSDragOperation::Copy }

        #[unsafe(method(draggingSession:endedAtPoint:operation:))]
        fn ended(&self, _session: &NSDraggingSession, _point: NSPoint, operation: NSDragOperation) {
            if let Some(done) = self.ivars().done.take() { done(Ok(operation != NSDragOperation::None)); }
        }
    }
);

pub(super) fn start_drag(path: &str, _bounds: (i32, i32, i32, i32), window: &tauri::WebviewWindow, done: Done) {
    let Some(mtm) = MainThreadMarker::new() else { return done(Err("The drag must start on the main thread".into())) };
    let Ok(ns) = window.ns_window() else { return done(Err("The notch window is not ready".into())) };
    let ns: &NSWindow = unsafe { &*(ns as *const NSWindow) };
    let (Some(view), Some(event)) = (ns.contentView(), NSApplication::sharedApplication(mtm).currentEvent()) else { return done(Err("No mouse drag to start from".into())) };
    let url = NSURL::fileURLWithPath(&NSString::from_str(path));
    let item = NSDraggingItem::initWithPasteboardWriter(NSDraggingItem::alloc(), ProtocolObject::from_ref(&*url));
    // The file's icon, 48 points, centred under the pointer.
    let icon = NSWorkspace::sharedWorkspace().iconForFile(&NSString::from_str(path));
    let at = view.convertPoint_fromView(event.locationInWindow(), None);
    unsafe { item.setDraggingFrame_contents(NSRect::new(NSPoint::new(at.x - 24.0, at.y - 24.0), NSSize::new(48.0, 48.0)), Some(&icon)) };
    let source = FileSource::alloc(mtm).set_ivars(Ivars { done: Cell::new(Some(done)) });
    let source: Retained<FileSource> = unsafe { msg_send![super(source), init] };
    let items = NSArray::from_retained_slice(&[item]);
    let _session = view.beginDraggingSessionWithItems_event_source(&items, &event, ProtocolObject::from_ref(&*source));
    // The session holds its source weakly: keep it alive until the drag ends (it reports once, then is not used again).
    std::mem::forget(source);
}
