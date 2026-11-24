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
        fn CFStringGetLength(string: *const c_void) -> isize;
        fn CFStringGetMaximumSizeForEncoding(length: isize, encoding: u32) -> isize;
        fn CFStringGetCString(
            string: *const c_void,
            buffer: *mut u8,
            buffer_size: isize,
            encoding: u32,
        ) -> u8;
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
            // Textos curtos chegam como ponteiro marcado (endereço ímpar): só
            // as funções do CoreFoundation podem lê-los, nunca uma referência Rust.
            let capacity =
                CFStringGetMaximumSizeForEncoding(CFStringGetLength(value), UTF8_ENCODING);
            if capacity < 0 {
                return None;
            }
            let mut buffer = vec![0u8; capacity as usize + 1];
            if CFStringGetCString(value, buffer.as_mut_ptr(), buffer.len() as isize, UTF8_ENCODING)
                == 0
            {
                return None;
            }
            let end = buffer.iter().position(|byte| *byte == 0).unwrap_or(buffer.len());
            buffer.truncate(end);
            String::from_utf8(buffer).ok()
        }
    }

    const UTF8_ENCODING: u32 = 0x0800_0100;

    #[cfg(test)]
    mod tests {
        use super::*;

        #[link(name = "CoreFoundation", kind = "framework")]
        extern "C" {
            fn CFStringCreateWithCString(
                allocator: *const c_void,
                text: *const u8,
                encoding: u32,
            ) -> *const c_void;
        }

        #[test]
        fn reads_short_and_long_strings() {
            // "Zoom" cabe num ponteiro marcado; o texto longo, com acento, não.
            for text in ["Zoom", "Reunião semanal de planejamento da equipe — sala 2"] {
                let bytes = format!("{text}\0");
                let value = unsafe {
                    CFStringCreateWithCString(std::ptr::null(), bytes.as_ptr(), UTF8_ENCODING)
                };
                assert_eq!(cf_string(value).as_deref(), Some(text));
                unsafe { CFRelease(value) };
            }
        }

        #[test]
        fn ignores_null() {
            assert_eq!(cf_string(std::ptr::null()), None);
        }
    }
}
