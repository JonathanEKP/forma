// `sqlx::migrate!` embeds the migrations at compile time, and Cargo does not
// know it read them: without this, adding a file to `migrations/sqlite` leaves
// an incremental build running the old set.
fn main() {
    println!("cargo:rerun-if-changed=../../migrations/sqlite");
}
