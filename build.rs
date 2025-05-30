use std::{
    env,
    error::Error,
    ffi::OsStr,
    fs::read_dir,
    path::Path,
    process::{Command, exit},
    str,
};

const LLVM_MAJOR_VERSION: usize = 18;

fn main() {
    if let Err(error) = run() {
        eprintln!("{}", error);
        exit(1);
    }
}

fn run() -> Result<(), Box<dyn Error>> {
    let version = llvm_config("--version")?;

    if !version.starts_with(&format!("{LLVM_MAJOR_VERSION}.",)) {
        return Err(format!(
            "failed to find correct version ({LLVM_MAJOR_VERSION}.x.x) of llvm-config (found {version})"
        )
        .into());
    }

    println!("cargo::metadata=PREFIX={}", llvm_config("--prefix")?);
    println!("cargo::metadata=CMAKE_DIR={}", llvm_config("--cmakedir")?);
    println!("cargo:rerun-if-changed=wrapper.h");
    println!("cargo:rustc-link-search={}", llvm_config("--libdir")?);

    for entry in read_dir(llvm_config("--libdir")?)? {
        if let Some(name) = entry?.path().file_name().and_then(OsStr::to_str) {
            if name.starts_with("libMLIR") {
                if let Some(name) = parse_archive_name(name) {
                    println!("cargo:rustc-link-lib=static={name}");
                }
            }
        }
    }

    println!("cargo:rustc-link-lib=MLIR");

    for name in llvm_config("--libnames")?.trim().split(' ') {
        if let Some(name) = parse_archive_name(name) {
            println!("cargo:rustc-link-lib={name}");
        }
    }

    for flag in llvm_config("--system-libs")?.trim().split(' ') {
        let flag = flag.trim_start_matches("-l");

        if flag.starts_with('/') {
            // llvm-config returns absolute paths for dynamically linked libraries.
            let path = Path::new(flag);

            println!(
                "cargo:rustc-link-search={}",
                path.parent().unwrap().display()
            );
            println!(
                "cargo:rustc-link-lib={}",
                path.file_stem()
                    .unwrap()
                    .to_str()
                    .unwrap()
                    .trim_start_matches("lib")
            );
        } else {
            println!("cargo:rustc-link-lib={flag}");
        }
    }

    if let Some(name) = get_system_libcpp() {
        println!("cargo:rustc-link-lib={name}");
    }

    bindgen::builder()
        .header("wrapper.h")
        .clang_arg(format!("-I{}", llvm_config("--includedir")?))
        .clang_arg(format!("-DMLIR_SYS_MAJOR_VERSION={LLVM_MAJOR_VERSION}"))
        .parse_callbacks(Box::new(bindgen::CargoCallbacks::new()))
        .allowlist_item("[Mm]lir.*")
        .generate()?
        .write_to_file(Path::new(&env::var("OUT_DIR")?).join("bindings.rs"))?;

    Ok(())
}

fn get_system_libcpp() -> Option<&'static str> {
    if cfg!(target_env = "msvc") {
        None
    } else if cfg!(target_os = "macos") {
        Some("c++")
    } else {
        Some("stdc++")
    }
}

fn llvm_config(argument: &str) -> Result<String, Box<dyn Error>> {
    let prefix = env::var(format!("MLIR_SYS_{LLVM_MAJOR_VERSION}0_PREFIX"))
        .map(|path| Path::new(&path).join("bin"))
        .unwrap_or_default();
    let llvm_config_exe = if cfg!(target_os = "windows") {
        "llvm-config.exe"
    } else {
        "llvm-config"
    };

    let call = format!(
        "{} --link-static {argument}",
        prefix.join(llvm_config_exe).display(),
    );

    Ok(str::from_utf8(
        &if cfg!(target_os = "windows") {
            Command::new("cmd").args(["/C", &call]).output()?
        } else {
            Command::new("sh").arg("-c").arg(&call).output()?
        }
        .stdout,
    )?
    .trim()
    .to_string())
}

fn parse_archive_name(name: &str) -> Option<&str> {
    if let Some(name) = name.strip_prefix("lib") {
        name.strip_suffix(".a")
    } else {
        None
    }
}
