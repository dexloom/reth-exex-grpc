fn main() {
    tonic_build::compile_protos("proto/exex.proto --experimental_allow_proto3_optional")
        .unwrap_or_else(|e| panic!("Failed to compile protos {:?}", e));
}
