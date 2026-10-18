//! Icône et informations de version intégrées à l'exécutable Windows
//! (affichées par l'Explorateur).

fn main() {
    println!("cargo:rerun-if-changed=assets/pstart-rs.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let mut res = winresource::WindowsResource::new();
    res.set_icon("assets/pstart-rs.ico");
    res.set("ProductName", "PStart-rs");
    res.set("FileDescription", "PStart-rs - lanceur portable");
    res.set("LegalCopyright", "MIT");
    if let Err(e) = res.compile() {
        println!("cargo:warning=ressources Windows non intégrées : {e}");
    }
}
