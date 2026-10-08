//! Embeds the app icon in the Windows executable.

fn main() {
    println!("cargo:rerun-if-changed=assets/sf2studio.ico");
    #[cfg(windows)]
    {
        let mut resource = winresource::WindowsResource::new();
        resource.set_icon("assets/sf2studio.ico");
        if let Err(error) = resource.compile() {
            println!("cargo:warning=could not embed the icon: {error}");
        }
    }
}
