/// Read text from an FLTK TextBuffer without leaking the C-allocated copy.
///
/// # Why This Exists
///
/// fltk-rs's `TextBuffer::text()` calls FLTK's `Fl_Text_Buffer_text()` which
/// returns a `malloc()`'d C string. The Rust wrapper copies this to a String
/// but never frees the original C pointer, causing a memory leak of the full
/// buffer size on every call.
///
/// This helper calls the FFI directly and properly frees the C allocation.
///
/// See `docs/temp/memory-optimization.md` for details on the leak discovery.
pub fn buffer_text_no_leak(buf: &fltk::text::TextBuffer) -> String {
    unsafe extern "C" {
        fn Fl_Text_Buffer_text(buf: *mut std::ffi::c_void) -> *mut std::ffi::c_char;
        fn free(ptr: *mut std::ffi::c_void);
    }

    // SAFETY: We call FLTK's C API directly to get the text pointer, copy it
    // to a Rust String, then free the C pointer. The sequence is:
    //   1. buf.as_ptr() returns the internal FLTK buffer pointer (valid while buf exists)
    //   2. Fl_Text_Buffer_text returns a malloc'd, null-terminated C string (or null if empty)
    //   3. CStr::from_ptr reads until null terminator (safe for valid C string)
    //   4. to_string_lossy handles any invalid UTF-8 gracefully
    //   5. free() releases the malloc'd memory (matches FLTK's allocation)
    //
    // Preconditions:
    //   - buf must be a valid TextBuffer (guaranteed by fltk-rs type system)
    //   - FLTK must be initialized (guaranteed by App existing before any buffers)
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
