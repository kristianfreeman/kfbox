//! Lets the page draw at the display's own rate. The web view holds a page's
//! frames near 60 a second unless told otherwise, even on a 120 Hz `ProMotion`
//! display; kfbox's waveforms scroll with the music, so it asks for the full
//! rate.
//!
//! The switch is one of the web view's feature flags, the one Safari shows as
//! "Prefer Page Rendering Updates near 60fps" under Develop › Feature Flags. It
//! is set through `WKPreferences`' private `_features` and
//! `_setEnabled:forFeature:`, checked for before each call: a web view without
//! them, or without the flag, is left as it is.

use std::ffi::c_void;

use objc2::runtime::{AnyClass, AnyObject, Bool};
use objc2::{msg_send, sel};
use objc2_foundation::NSString;

const FLAG: &str = "PreferPageRenderingUpdatesNear60FPSEnabled";

/// Turns the 60 fps preference off for `webview`, a `WKWebView`. True if the
/// flag was found and set. Call on the main thread.
pub fn full_rate(webview: *mut c_void) -> bool {
    let Some(class) = AnyClass::get(c"WKPreferences") else {
        return false;
    };
    let webview = webview.cast::<AnyObject>();
    if webview.is_null() || !class.metaclass().responds_to(sel!(_features)) || !class.responds_to(sel!(_setEnabled:forFeature:)) {
        return false;
    }
    let wanted = NSString::from_str(FLAG);
    // SAFETY: `webview` is the window's live WKWebView, and every selector sent
    // is one the web view answers: `configuration` and `preferences` are public, and
    // `_features` and `_setEnabled:forFeature:` were checked for above. Each
    // feature is asked for its `key` only if it answers to it.
    unsafe {
        let configuration: *mut AnyObject = msg_send![webview, configuration];
        if configuration.is_null() {
            return false;
        }
        let preferences: *mut AnyObject = msg_send![configuration, preferences];
        let features: *mut AnyObject = msg_send![class, _features];
        if preferences.is_null() || features.is_null() {
            return false;
        }
        let count: usize = msg_send![features, count];
        for i in 0..count {
            let feature: *mut AnyObject = msg_send![features, objectAtIndex: i];
            if feature.is_null() || !(*feature).class().responds_to(sel!(key)) {
                continue;
            }
            let key: *mut AnyObject = msg_send![feature, key];
            if key.is_null() {
                continue;
            }
            let same: Bool = msg_send![key, isEqualToString: &*wanted];
            if same.as_bool() {
                let _: () = msg_send![preferences, _setEnabled: Bool::NO, forFeature: feature];
                return true;
            }
        }
    }
    false
}
