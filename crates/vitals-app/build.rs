fn main() {
    println!("cargo:rerun-if-changed=assets/vitals.rc");
    println!("cargo:rerun-if-changed=assets/vitals.ico");
    println!("cargo:rerun-if-changed=assets/vitals.manifest");
    embed_resource::compile("assets/vitals.rc", embed_resource::NONE)
        .manifest_required()
        .expect("could not embed Vitals resources");
}
