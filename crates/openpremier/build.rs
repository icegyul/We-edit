//! Embeds the application icon and version information in the Windows executable.

fn main() {
    println!("cargo:rerun-if-changed=../../assets/icons/we-edit.ico");
    #[cfg(windows)]
    {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("../../assets/icons/we-edit.ico");
        res.set("ProductName", "We edit");
        res.set("FileDescription", "We edit video editor");
        res.set("CompanyName", "We edit");
        res.set("LegalCopyright", "GNU GPL v3 or later");
        if let Err(e) = res.compile() {
            println!("cargo:warning=could not embed the Windows icon: {e}");
        }
    }
}
