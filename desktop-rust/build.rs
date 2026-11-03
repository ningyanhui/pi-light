// SPDX-License-Identifier: Apache-2.0
// Embed the clean-room brand ICO into the Windows PE resource table so Explorer
// and the taskbar show π, not eframe's default "E".

fn main() {
    println!("cargo:rerun-if-changed=assets/pi-light.ico");
    println!("cargo:rerun-if-changed=assets/pi-light-256.png");
    #[cfg(windows)]
    embed_windows_icon();
}

#[cfg(windows)]
fn embed_windows_icon() {
    let mut res = winres::WindowsResource::new();
    res.set_icon("assets/pi-light.ico");
    res.set("ProductName", "pi-light");
    res.set("FileDescription", "pi-light");
    res.set("LegalCopyright", "Apache-2.0");
    res.set("OriginalFilename", "pi-light.exe");
    res.compile()
        .expect("embed desktop-rust/assets/pi-light.ico via winres");
}
