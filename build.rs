#[cfg(windows)]
fn main() {
    let mut res = winres::WindowsResource::new();
    res.set_manifest_file("app.manifest");
    res.set_icon("assets/app_icon.ico");
    if let Err(e) = res.compile() {
        eprintln!("Warning: Failed to compile windows resource: {}", e);
    }
}

#[cfg(not(windows))]
fn main() {}
