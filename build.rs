fn main() {
    #[cfg(target_os = "windows")]
    {
        let mut res = winres::WindowsResource::new();

        // Icon shown in Explorer, taskbar, Alt+Tab
        res.set_icon("ferrispad.ico");

        // "Details" tab in file Properties
        res.set("ProductName", "FerrisPad");
        res.set("FileDescription", "FerrisPad — A blazingly fast text editor");
        res.set("CompanyName", "Federico Conticello");
        res.set("LegalCopyright", "\u{00a9} 2025-2026 Federico Conticello — MIT License");
        res.set("OriginalFilename", "FerrisPad.exe");

        // Auto-sync version from Cargo.toml
        let version = env!("CARGO_PKG_VERSION");
        res.set("FileVersion", version);
        res.set("ProductVersion", version);

        res.compile().expect("Failed to compile Windows resources");
    }
}
