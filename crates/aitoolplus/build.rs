use std::path::Path;

fn main() {
    #[cfg(target_os = "windows")]
    {
        println!("cargo:rerun-if-changed=assets/app.ico");
        let manifest_dir = env!("CARGO_MANIFEST_DIR");
        let icon_path = Path::new(manifest_dir).join("assets/app.ico");

        let mut res = winresource::WindowsResource::new();
        if icon_path.exists()
            && let Some(icon_str) = icon_path.to_str()
        {
            res.set_icon(icon_str);
        }
        res.set("ProductName", "AI ToolPlus");
        res.set(
            "FileDescription",
            "AI ToolPlus - AI workbench on GPUI",
        );
        res.set("LegalCopyright", "GPL-3.0-only");
        if let Err(e) = res.compile() {
            eprintln!("cargo:warning=Failed to compile Windows resources: {}", e);
        }
    }
}

