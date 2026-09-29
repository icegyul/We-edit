//! Embeds the application icon and version information in the Windows executable.

fn main() {
    println!("cargo:rerun-if-changed=../../assets/icons/openpremier.ico");
    #[cfg(windows)]
    {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("../../assets/icons/openpremier.ico");
        res.set("ProductName", "OpenPremier");
        res.set("FileDescription", "OpenPremier video editor");
        res.set("CompanyName", "OpenPremier contributors");
        res.set("LegalCopyright", "GNU GPL v3 or later");
        if let Err(e) = res.compile() {
            println!("cargo:warning=could not embed the Windows icon: {e}");
        }
    }
}
