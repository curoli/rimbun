fn main() {
    // sqlx::migrate! embeds migration files at compile time.
    println!("cargo:rerun-if-changed=../../migrations");
}
