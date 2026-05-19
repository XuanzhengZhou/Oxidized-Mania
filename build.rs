fn main() {
    // 编译 sonic.c (BASS_FX 失败时回退，启用 SONIC_USE_SIN 提升质量)
    let sonic_path = "sonic.c";
    if std::path::Path::new(sonic_path).exists() {
        cc::Build::new()
            .file(sonic_path)
            .define("SONIC_USE_SIN", Some("1"))
            .opt_level(3)
            .compile("sonic");
        println!("cargo:rerun-if-changed=sonic.c");
    }

    let bass_dirs = ["./libs/bass24-osx", "./libs/bass24-linux", "./libs/bass24", "./libs", "."];
    let fx_dirs  = ["./libs/bass_fx24-osx", "./libs/bass_fx24-linux", "./libs/bass_fx24", "./libs", "."];

    for dir in &bass_dirs {
        let p = std::path::Path::new(dir);
        if p.join("libbass.dylib").exists() || p.join("bass.dll").exists() || p.join("libbass.so").exists() {
            println!("cargo:rustc-link-search=native={}", dir);
            println!("cargo:rustc-link-lib=dylib=bass");
            if cfg!(target_os = "macos") { println!("cargo:rustc-link-arg=-Wl,-rpath,{}", p.canonicalize().unwrap().display()); }
            break;
        }
    }
    for dir in &fx_dirs {
        let p = std::path::Path::new(dir);
        if p.join("libbass_fx.dylib").exists() || p.join("bass_fx.dll").exists() || p.join("libbass_fx.so").exists() {
            println!("cargo:rustc-link-search=native={}", dir);
            println!("cargo:rustc-link-lib=dylib=bass_fx");
            if cfg!(target_os = "macos") { println!("cargo:rustc-link-arg=-Wl,-rpath,{}", p.canonicalize().unwrap().display()); }
            break;
        }
    }
}
