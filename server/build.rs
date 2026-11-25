// Garantiza que existe la carpeta de la web que se embebe en el binario.
// Si Flutter todavía no se ha compilado, deja una página provisional.
use std::{fs, path::Path};

fn main() {
    let dir = Path::new("../web/build/web");
    if !dir.join("index.html").exists() {
        fs::create_dir_all(dir).expect("crear ../web/build/web");
        fs::write(
            dir.join("index.html"),
            "<!doctype html><meta charset=utf-8><title>GordurasFM</title>\
             <p>GordurasFM está funcionando. La interfaz web todavía no se ha compilado.</p>",
        )
        .expect("escribir index.html provisional");
    }
    println!("cargo:rerun-if-changed=../web/build/web");
    println!("cargo:rerun-if-changed=migrations");
}
