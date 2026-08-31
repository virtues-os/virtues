#include "bindings/bindings.h"

#import <UIKit/UIKit.h>
#import <objc/runtime.h>

// WKWebView mounts a form-navigation bar (◀ ▶ chevrons + Done) above every
// keyboard it raises. That bar is web-form chrome — it exists to hop between
// the fields of a login page — and in an app whose whole surface is one chat
// composer it is dead weight the native chat apps don't carry. WebKit offers
// no API to decline it; the view that vends it is WKContentView's
// `inputAccessoryView`, so the accepted fix (Capacitor, Cordova, Telegram all
// ship a variant) is to swap that method's implementation for one returning
// nil. Class looked up by name because WKContentView is not a public symbol;
// if an iOS release renames it the lookup fails soft and the bar comes back —
// cosmetic, not a crash.
static id virtues_nil_accessory(id self, SEL _cmd) { return nil; }

static void virtues_remove_keyboard_accessory(void) {
	Class cls = NSClassFromString(@"WKContentView");
	if (!cls) return;
	Method m = class_getInstanceMethod(cls, @selector(inputAccessoryView));
	if (m) method_setImplementation(m, (IMP)virtues_nil_accessory);
}

int main(int argc, char * argv[]) {
	// WebKit is linked into the binary (wry), so the class is registered
	// before start_app spins the runloop — no lazy-load race to wait out.
	virtues_remove_keyboard_accessory();
	ffi::start_app();
	return 0;
}
