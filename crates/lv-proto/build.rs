fn main() -> Result<(), Box<dyn std::error::Error>> {
    tonic_build::configure()
        .build_server(true)
        .build_client(true)
        .compile_protos(
            &[
                "proto/litevikings/v1/common.proto",
                "proto/litevikings/v1/filesystem.proto",
                "proto/litevikings/v1/resources.proto",
                "proto/litevikings/v1/sessions.proto",
                "proto/litevikings/v1/search.proto",
                "proto/litevikings/v1/relations.proto",
                "proto/litevikings/v1/admin.proto",
            ],
            &["proto"],
        )?;
    Ok(())
}
