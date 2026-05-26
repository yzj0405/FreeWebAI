fn main() {
    println!("cargo:dev=pnpm dev");
    tauri_build::build()
}
