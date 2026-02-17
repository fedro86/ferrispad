/// Read text from an FLTK TextBuffer without leaking the C-allocated copy.
/// fltk-rs's `TextBuffer::text()` leaks the C `char*` returned by
/// `Fl_Text_Buffer_text`. This helper calls the FFI directly and frees it.
pub fn buffer_text_no_leak(buf: &fltk::text::TextBuffer) -> String {
    unsafe extern "C" {
        fn Fl_Text_Buffer_text(buf: *mut std::ffi::c_void) -> *mut std::ffi::c_char;
        fn free(ptr: *mut std::ffi::c_void);
    }

    unsafe {
        let inner = buf.as_ptr() as *mut std::ffi::c_void;
        let ptr = Fl_Text_Buffer_text(inner);
        if ptr.is_null() {
            return String::new();
        }
        let cstr = std::ffi::CStr::from_ptr(ptr);
        let result = cstr.to_string_lossy().into_owned();
        free(ptr as *mut std::ffi::c_void);
        result
    }
}
