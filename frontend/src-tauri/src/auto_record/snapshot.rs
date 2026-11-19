use super::detect::{ObservedInput, ObservedWindow};

pub fn capture() -> (Vec<ObservedWindow>, Vec<ObservedInput>) {
    #[cfg(target_os = "macos")]
    {
        macos::capture()
    }
    #[cfg(not(target_os = "macos"))]
    {
        (Vec::new(), Vec::new())
    }
}

#[cfg(target_os = "macos")]
mod macos {
    use super::{ObservedInput, ObservedWindow};
    use std::ffi::c_void;

    const ON_SCREEN_ONLY: u32 = 1;

    #[link(name = "CoreGraphics", kind = "framework")]
    extern "C" {
        static kCGWindowOwnerName: *const c_void;
        static kCGWindowName: *const c_void;
        fn CGWindowListCopyWindowInfo(option: u32, relative_to_window: u32) -> *mut c_void;
    }

    #[link(name = "CoreFoundation", kind = "framework")]
    extern "C" {
        fn CFRelease(value: *const c_void);
        fn CFGetTypeID(value: *const c_void) -> usize;
        fn CFStringGetTypeID() -> usize;
        fn CFArrayGetCount(array: *const c_void) -> isize;
        fn CFArrayGetValueAtIndex(array: *const c_void, index: isize) -> *const c_void;
        fn CFDictionaryGetValue(dictionary: *const c_void, key: *const c_void) -> *const c_void;
    }

    pub fn capture() -> (Vec<ObservedWindow>, Vec<ObservedInput>) {
        (windows(), inputs())
    }

    fn inputs() -> Vec<ObservedInput> {
        let Ok(processes) = cidre::core_audio::System::processes() else {
            return Vec::new();
        };
        let mut inputs = Vec::new();
        for process in processes {
            let Ok(bundle) = process.bundle_id() else {
                continue;
            };
            let bundle_id = bundle.to_string();
            if bundle_id.is_empty() {
                continue;
            }
            inputs.push(ObservedInput {
                input_active: process.is_running_input().unwrap_or(false),
                bundle_id,
            });
        }
        inputs
    }

    fn windows() -> Vec<ObservedWindow> {
        let list = unsafe { CGWindowListCopyWindowInfo(ON_SCREEN_ONLY, 0) };
        if list.is_null() {
            return Vec::new();
        }
        let mut windows = Vec::new();
        let count = unsafe { CFArrayGetCount(list) };
        for index in 0..count {
            let dictionary = unsafe { CFArrayGetValueAtIndex(list, index) };
            if dictionary.is_null() {
                continue;
            }
            let owner = dictionary_string(dictionary, unsafe { kCGWindowOwnerName });
            let title = dictionary_string(dictionary, unsafe { kCGWindowName });
            if owner.is_empty() && title.is_empty() {
                continue;
            }
            windows.push(ObservedWindow { owner, title });
        }
        unsafe { CFRelease(list) };
        windows
    }

    fn dictionary_string(dictionary: *const c_void, key: *const c_void) -> String {
        if key.is_null() {
            return String::new();
        }
        let value = unsafe { CFDictionaryGetValue(dictionary, key) };
        cf_string(value).unwrap_or_default()
    }

    fn cf_string(value: *const c_void) -> Option<String> {
        if value.is_null() {
            return None;
        }
        unsafe {
            if CFGetTypeID(value) != CFStringGetTypeID() {
                return None;
            }
            let string = &*(value as *const cidre::cf::String);
            Some(string.to_string())
        }
    }
}
