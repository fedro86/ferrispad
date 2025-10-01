fn main() {
    // Embed Windows icon
    #[cfg(target_os = "windows")]
    {
        let mut res = winres::WindowsResource::new();
        res.set_icon("ferrispad.ico");
        res.compile().unwrap();
    }
}
