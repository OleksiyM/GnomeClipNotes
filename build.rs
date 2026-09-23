use std::{
    env, fs,
    path::{Path, PathBuf},
    process::Command,
};

const DOMAIN: &str = "gnome-clip-notes";

fn compile_catalogs(output: &Path) {
    println!("cargo:rerun-if-changed=po/LINGUAS");
    let linguas = fs::read_to_string("po/LINGUAS").expect("Could not read po/LINGUAS");
    for locale in linguas
        .lines()
        .map(|line| line.split('#').next().unwrap_or(""))
        .flat_map(str::split_whitespace)
    {
        let source = PathBuf::from("po").join(format!("{locale}.po"));
        println!("cargo:rerun-if-changed={}", source.display());
        let destination = output
            .join(locale)
            .join("LC_MESSAGES")
            .join(format!("{DOMAIN}.mo"));
        fs::create_dir_all(destination.parent().expect("catalog output directory"))
            .expect("Could not create catalog output directory");
        let status = Command::new("msgfmt")
            .args(["--check", "--check-format", "--output-file"])
            .arg(&destination)
            .arg(&source)
            .status()
            .expect("Install msgfmt from GNU gettext to compile translations");
        assert!(status.success(), "Could not compile {}", source.display());
    }
}

fn main() {
    println!("cargo:rerun-if-changed=data/resources.xml");
    println!("cargo:rerun-if-changed=data/io.github.OleksiyM.GnomeClipNotes.svg");
    let out_dir = PathBuf::from(env::var_os("OUT_DIR").expect("Cargo output directory"));
    let output = out_dir.join("resources.gresource");
    let status = Command::new("glib-compile-resources")
        .arg("data/resources.xml")
        .arg("--sourcedir=data")
        .arg("--target")
        .arg(output)
        .status()
        .expect("Install glib-compile-resources (glib2-devel on Fedora, libglib2.0-bin on Ubuntu)");
    assert!(status.success(), "Could not compile application resources");
    let locale_dir = out_dir.join("locales");
    compile_catalogs(&locale_dir);
    println!(
        "cargo:rustc-env=GCN_BUILD_LOCALE_DIR={}",
        locale_dir.display()
    );
}
