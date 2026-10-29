//! Bettet unter Windows das VisiCutR-Symbol in die Programmdatei ein; der
//! Explorer und die Taskleiste zeigen es dann.

fn main() {
    let icon = "resources/icon/VisiCutR.ico";
    println!("cargo:rerun-if-changed={icon}");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        #[cfg(windows)]
        {
            let mut resource = winresource::WindowsResource::new();
            resource.set_icon(icon);
            resource.set("ProductName", "VisiCutR");
            resource.set("FileDescription", "VisiCutR");
            resource
                .compile()
                .expect("Windows-Ressourcen (VisiCut.ico) nicht eingebettet");
        }
    }
}
