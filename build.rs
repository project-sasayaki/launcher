use winresource::VersionInfo;

fn main() {
    windows_reactor_setup::as_framework_dependent();

    // rust-i18n's i18n! macro compiles locales/*.yml into the binary, but the compiler does
    // not know it read them; this makes an edited text rebuild the crate.
    println!("cargo:rerun-if-changed=locales");

    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        // The resource version is taken from APP_VERSION in src/main.rs (the same constant
        // the UI shows), so the two can never disagree.
        println!("cargo:rerun-if-changed=src/main.rs");
        let mut res = winresource::WindowsResource::new();
        res.set_icon("src/assets/launcher.ico");
        let version = app_version();
        res.set("FileVersion", &version);
        res.set("ProductVersion", &version);
        // Explorer's "Details" tab reads "File version / Product version" from the *numeric*
        // VERSIONINFO fields, which winresource fills from CARGO_PKG_VERSION (0.1.0) by default;
        // set() above only writes the string table (the copy .NET and PowerShell read).
        // Both must agree, or the properties show 0.1.0.0 while the code says 1.0.0.1.
        match packed_version(&version) {
            Some(packed) => {
                res.set_version_info(VersionInfo::FILEVERSION, packed);
                res.set_version_info(VersionInfo::PRODUCTVERSION, packed);
            }
            None => println!(
                "cargo::warning=APP_VERSION `{version}` is not four numbers; the numeric VERSIONINFO fields are not set"
            ),
        }
        res.compile()
            .expect("failed to compile windows icon resource");
    }
}

/// `MAJOR.MINOR.PATCH.RELEASE` → the u64 VERSIONINFO expects (16 bits per part).
fn packed_version(ver: &str) -> Option<u64> {
    let parts: Vec<u64> = ver
        .split('.')
        .map(|s| s.trim().parse::<u64>().ok())
        .collect::<Option<Vec<u64>>>()?;
    match parts[..] {
        [major, minor, patch, release] => {
            Some((major << 48) | (minor << 32) | (patch << 16) | release)
        }
        _ => None,
    }
}

/// Reads the literal of `pub const APP_VERSION: &str = "…"` from `src/main.rs`.
fn app_version() -> String {
    std::fs::read_to_string("src/main.rs")
        .unwrap_or_default()
        .lines()
        .find_map(|line| {
            let rest = line.split_once("const APP_VERSION")?.1;
            let lit = rest.split_once('"')?.1;
            Some(lit.split('"').next()?.trim().to_string())
        })
        .unwrap_or_else(|| "0.0.0.0".to_string())
}
