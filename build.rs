fn main() {
    // Media Foundation (mfplat.dll) is missing on Windows "N" editions without the
    // Media Feature Pack. Delay-loading it lets the application start anyway; it then
    // checks for the DLL before use (`decoder::mf_startup`) and falls back to MJPEG.
    // The GNU linker cannot delay-load, so GNU builds still need the DLL at start-up.
    if std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc") {
        println!("cargo:rustc-link-arg=/DELAYLOAD:mfplat.dll");
        println!("cargo:rustc-link-lib=delayimp");
    }
}
