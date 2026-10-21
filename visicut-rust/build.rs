//! Bettet unter Windows das VisiCut-Symbol (wie die Java-Version) in die
//! Programmdatei ein; der Explorer und die Taskleiste zeigen es dann.

fn main() {
    let icon = "../distribute/windows/VisiCut.ico";
    println!("cargo:rerun-if-changed={icon}");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        #[cfg(windows)]
        {
            let mut resource = winresource::WindowsResource::new();
            resource.set_icon(icon);
            resource.set("ProductName", "VisiCutRust");
            resource.set("FileDescription", "VisiCutRust");
            resource
                .compile()
                .expect("Windows-Ressourcen (VisiCut.ico) nicht eingebettet");
        }
    }
}
