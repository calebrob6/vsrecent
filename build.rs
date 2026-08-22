fn main() {
    embed_resource::compile("vsrecent.rc", embed_resource::NONE)
        .manifest_required()
        .unwrap();
}
