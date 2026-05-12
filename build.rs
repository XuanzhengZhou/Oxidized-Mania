fn main() {
    // 编译 sonic.c
    let sonic_path = "sonic.c";
    if std::path::Path::new(sonic_path).exists() {
        cc::Build::new()
            .file(sonic_path)
            .opt_level(3)
            .compile("sonic");
        println!("cargo:rerun-if-changed=sonic.c");
    }

    // BASS 库
    for dir in &["./libs", "."] {
        let p = std::path::Path::new(dir);
        if p.join("libbass.dylib").exists() || p.join("bass.dll").exists() || p.join("libbass.so").exists() {
            println!("cargo:rustc-link-search=native={}", dir);
            println!("cargo:rustc-link-lib=dylib=bass");
            if cfg!(target_os = "macos") {
                println!("cargo:rustc-link-arg=-Wl,-rpath,{}", p.canonicalize().unwrap().display());
            }
            return;
        }
    }
    println!("cargo:rustc-link-lib=dylib=bass");
}
